//! The wgpu device, the one pipeline, surface targets and offscreen capture.
//!
//! - **Instance:** DX12 on Windows with the DirectComposition-visual swapchain, the only path
//!   with per-pixel transparency (`Dx12SwapchainKind::DxgiFromVisual`, set in code rather than
//!   through `WGPU_DX12_PRESENTATION_SYSTEM` as CursorFX does); Metal on macOS, Vulkan elsewhere.
//! - **Format and alpha:** a non-sRGB UNORM format and premultiplied alpha (AGENTS.md invariant
//!   9: an `*_Srgb` target encodes `c * a` after blending and DWM shows invalid premultiplied
//!   values). Pipeline blend: `One, OneMinusSrcAlpha`.
//! - **Targets:** [`SurfaceTarget`] declares its pending texture before its surface and releases
//!   it on every reconfigure and drop path (invariant 12: Vulkan panics otherwise).
//!
//! Conceptual source: CursorFX `src-tauri/src/overlay/mod.rs` (MIT OR Apache-2.0, same author):
//! format/alpha choice, `desired_maximum_frame_latency: 1`, adapter limits, pending-first targets.

use std::future::Future;
use std::sync::Arc;
use std::task::{Context, Poll, Wake};

use crate::mesh::{Mesh, Vertex};

#[derive(Debug)]
pub enum RenderError {
    NoAdapter(String),
    Device(String),
    Surface(String),
    Unsupported(String),
}

impl std::error::Error for RenderError {}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoAdapter(e) => write!(f, "no GPU adapter: {e}"),
            Self::Device(e) => write!(f, "GPU device: {e}"),
            Self::Surface(e) => write!(f, "surface: {e}"),
            Self::Unsupported(e) => write!(f, "unsupported: {e}"),
        }
    }
}

/// Runs a future to completion on this thread (wgpu's native futures resolve without a reactor).
pub fn block_on<F: Future>(f: F) -> F::Output {
    struct Unpark(std::thread::Thread);
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Arc::new(Unpark(std::thread::current())).into();
    let mut cx = Context::from_waker(&waker);
    let mut f = std::pin::pin!(f);
    loop {
        match f.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending => std::thread::park(),
        }
    }
}

/// The instance every target shares.
pub fn instance() -> wgpu::Instance {
    let mut backend_options = wgpu::BackendOptions::default();
    backend_options.dx12.presentation_system = wgpu::Dx12SwapchainKind::DxgiFromVisual;
    wgpu::Instance::new(wgpu::InstanceDescriptor {
        // DX12 only on Windows: its composition-visual swapchain is the one transparent path
        // (a Vulkan surface on Windows offers no premultiplied alpha).
        backends: if cfg!(target_os = "windows") {
            wgpu::Backends::DX12
        } else if cfg!(target_os = "macos") {
            wgpu::Backends::METAL
        } else {
            wgpu::Backends::VULKAN
        },
        flags: wgpu::InstanceFlags::default(),
        backend_options,
        display: None,
        memory_budget_thresholds: Default::default(),
    })
}

/// Picks the non-sRGB UNORM format of a surface's (or the offscreen) formats.
pub fn pick_format(formats: &[wgpu::TextureFormat]) -> Option<wgpu::TextureFormat> {
    use wgpu::TextureFormat as F;
    [F::Bgra8Unorm, F::Rgba8Unorm]
        .into_iter()
        .find(|f| formats.contains(f))
        .or_else(|| formats.iter().copied().find(|f| !f.is_srgb()))
}

/// Premultiplied first; an opaque surface is accepted only when `allow_opaque`.
pub fn pick_alpha(
    modes: &[wgpu::CompositeAlphaMode],
    allow_opaque: bool,
) -> Option<wgpu::CompositeAlphaMode> {
    use wgpu::CompositeAlphaMode as M;
    let mut order = vec![M::PreMultiplied, M::PostMultiplied, M::Inherit];
    if allow_opaque {
        order.push(M::Opaque);
    }
    order.into_iter().find(|m| modes.contains(m))
}

/// Device, queue, the pipeline per target format, and a growable vertex buffer.
pub struct Gpu {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    shader: wgpu::ShaderModule,
    layout: wgpu::PipelineLayout,
    pipelines: Vec<(wgpu::TextureFormat, wgpu::RenderPipeline)>,
    vertices: wgpu::Buffer,
}

impl std::fmt::Debug for Gpu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gpu")
            .field("adapter", &self.adapter.get_info().name)
            .finish_non_exhaustive()
    }
}

const INITIAL_VERTICES: u64 = 64 * 1024;

impl Gpu {
    /// Requests an adapter (compatible with `surface` when given) and a device with the
    /// adapter's own limits (a large monitor can exceed the default 8,192 px texture limit).
    pub fn new(
        instance: wgpu::Instance,
        surface: Option<&wgpu::Surface<'_>>,
    ) -> Result<Self, RenderError> {
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            force_fallback_adapter: false,
            compatible_surface: surface,
            apply_limit_buckets: false,
        }))
        .map_err(|e| RenderError::NoAdapter(e.to_string()))?;
        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("av-render"),
            required_features: wgpu::Features::empty(),
            required_limits: adapter.limits(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
        }))
        .map_err(|e| RenderError::Device(e.to_string()))?;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("av-render mesh"),
            source: wgpu::ShaderSource::Wgsl(include_str!("mesh.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("av-render mesh"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let vertices = Self::vertex_buffer(&device, INITIAL_VERTICES);
        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            shader,
            layout,
            pipelines: Vec::new(),
            vertices,
        })
    }

    fn vertex_buffer(device: &wgpu::Device, count: u64) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("av-render vertices"),
            size: count * std::mem::size_of::<Vertex>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    pub fn adapter_name(&self) -> String {
        let i = self.adapter.get_info();
        format!("{} ({:?})", i.name, i.backend)
    }

    fn pipeline(&mut self, format: wgpu::TextureFormat) -> &wgpu::RenderPipeline {
        if let Some(i) = self.pipelines.iter().position(|(f, _)| *f == format) {
            return &self.pipelines[i].1;
        }
        let blend = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        };
        let pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("av-render mesh"),
                layout: Some(&self.layout),
                vertex: wgpu::VertexState {
                    module: &self.shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4],
                    })],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &self.shader,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState {
                            color: blend,
                            alpha: blend,
                        }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            });
        self.pipelines.push((format, pipeline));
        &self.pipelines.last().expect("just pushed").1
    }

    /// Clears `view` to `clear` (premultiplied) and draws `mesh`; submits immediately, so the
    /// shared vertex buffer can be rewritten for the next target (queue-ordered).
    pub fn draw(
        &mut self,
        view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        mesh: &Mesh,
        clear: [f64; 4],
    ) {
        let needed = mesh.vertices.len() as u64;
        if needed * std::mem::size_of::<Vertex>() as u64 > self.vertices.size() {
            self.vertices = Self::vertex_buffer(&self.device, needed.next_power_of_two());
        }
        if needed > 0 {
            self.queue
                .write_buffer(&self.vertices, 0, bytemuck::cast_slice(&mesh.vertices));
        }
        self.pipeline(format);
        let pipeline = &self
            .pipelines
            .iter()
            .find(|(f, _)| *f == format)
            .expect("created above")
            .1;
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("av-render frame"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("av-render frame"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: clear[0],
                            g: clear[1],
                            b: clear[2],
                            a: clear[3],
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if needed > 0 {
                pass.set_pipeline(pipeline);
                pass.set_vertex_buffer(0, self.vertices.slice(..));
                pass.draw(0..needed as u32, 0..1);
            }
        }
        self.queue.submit([encoder.finish()]);
    }

    /// Renders `mesh` into a `width` x `height` RGBA8 texture and reads it back (row-major,
    /// premultiplied). Tests and snapshots; allocates.
    pub fn render_offscreen(
        &mut self,
        width: u32,
        height: u32,
        mesh: &Mesh,
        clear: [f64; 4],
    ) -> Vec<u8> {
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("av-render offscreen"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        self.draw(&view, format, mesh, clear);
        let row = (width * 4).div_ceil(256) * 256;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("av-render readback"),
            size: (row * height) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        let Ok(data) = slice.get_mapped_range() else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            let start = (y * row) as usize;
            out.extend_from_slice(&data[start..start + (width * 4) as usize]);
        }
        drop(data);
        buffer.unmap();
        out
    }
}

/// One window's swapchain. `pending` is declared first so a drop releases the acquired texture
/// before the surface; [`SurfaceTarget::release_pending`] runs on every reconfigure path anyway.
pub struct SurfaceTarget {
    pending: Option<wgpu::SurfaceTexture>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
}

impl std::fmt::Debug for SurfaceTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SurfaceTarget")
            .field("size", &(self.config.width, self.config.height))
            .field("format", &self.config.format)
            .field("alpha", &self.config.alpha_mode)
            .finish()
    }
}

/// How a target presents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetSpec {
    pub width: u32,
    pub height: u32,
    /// Fifo paces on vblank; Mailbox never blocks the loop (lower-priority targets).
    pub vsync: bool,
    /// An opaque surface is acceptable (the wallpaper target).
    pub allow_opaque: bool,
}

impl SurfaceTarget {
    /// Creates the surface for a native window.
    ///
    /// # Safety
    ///
    /// `window` (and `display`) must stay valid until this target is dropped: the caller keeps the
    /// native window alive for at least as long (the host owns both and drops the target first).
    pub unsafe fn create(
        instance: &wgpu::Instance,
        window: raw_window_handle::RawWindowHandle,
        display: Option<raw_window_handle::RawDisplayHandle>,
    ) -> Result<wgpu::Surface<'static>, RenderError> {
        // SAFETY: forwarded from the caller (the window outlives the surface).
        unsafe {
            instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: display,
                raw_window_handle: window,
            })
        }
        .map_err(|e| RenderError::Surface(e.to_string()))
    }

    /// Configures `surface` for `gpu` with a non-sRGB format and premultiplied alpha.
    pub fn new(
        gpu: &Gpu,
        surface: wgpu::Surface<'static>,
        spec: TargetSpec,
    ) -> Result<Self, RenderError> {
        let caps = surface.get_capabilities(&gpu.adapter);
        let format = pick_format(&caps.formats)
            .ok_or_else(|| RenderError::Unsupported("no non-sRGB UNORM format".into()))?;
        let alpha_mode = pick_alpha(&caps.alpha_modes, spec.allow_opaque).ok_or_else(|| {
            RenderError::Unsupported(format!(
                "no transparent alpha mode (have {:?})",
                caps.alpha_modes
            ))
        })?;
        let present_mode = if spec.vsync {
            wgpu::PresentMode::Fifo
        } else if caps.present_modes.contains(&wgpu::PresentMode::Mailbox) {
            wgpu::PresentMode::Mailbox
        } else {
            wgpu::PresentMode::Fifo
        };
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: Default::default(),
            width: spec.width.max(1),
            height: spec.height.max(1),
            present_mode,
            desired_maximum_frame_latency: 1,
            alpha_mode,
            view_formats: vec![],
        };
        surface.configure(&gpu.device, &config);
        Ok(Self {
            pending: None,
            surface,
            config,
        })
    }

    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    pub fn alpha_mode(&self) -> wgpu::CompositeAlphaMode {
        self.config.alpha_mode
    }

    /// Drops an acquired but unpresented texture (before any reconfigure or drop).
    pub fn release_pending(&mut self) {
        self.pending = None;
    }

    pub fn resize(&mut self, gpu: &Gpu, width: u32, height: u32) {
        if (width, height) == self.size() || width == 0 || height == 0 {
            return;
        }
        self.release_pending();
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&gpu.device, &self.config);
    }

    /// Draws `mesh` (built for this target's size) and presents. Returns false when the frame was
    /// skipped (occluded, timeout, outdated: the surface is reconfigured for the next one).
    pub fn render(&mut self, gpu: &mut Gpu, mesh: &Mesh, clear: [f64; 4]) -> bool {
        self.release_pending();
        let texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&gpu.device, &self.config);
                return false;
            }
            _ => return false,
        };
        let view = texture.texture.create_view(&Default::default());
        gpu.draw(&view, self.config.format, mesh, clear);
        drop(view);
        gpu.queue.present(texture);
        true
    }
}

impl Drop for SurfaceTarget {
    fn drop(&mut self) {
        self.release_pending();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_avoid_srgb_and_alpha_prefers_premultiplied() {
        use wgpu::{CompositeAlphaMode as M, TextureFormat as F};
        assert_eq!(
            pick_format(&[F::Bgra8UnormSrgb, F::Bgra8Unorm]),
            Some(F::Bgra8Unorm)
        );
        assert_eq!(pick_format(&[F::Bgra8UnormSrgb]), None);
        assert_eq!(
            pick_alpha(&[M::Opaque, M::PreMultiplied], false),
            Some(M::PreMultiplied)
        );
        assert_eq!(pick_alpha(&[M::Opaque], false), None);
        assert_eq!(pick_alpha(&[M::Opaque], true), Some(M::Opaque));
    }
}
