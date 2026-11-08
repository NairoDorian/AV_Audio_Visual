//! The native displays: wallpaper, taskbar strip and overlay (blueprint section 10).
//!
//! One `av-render` thread owns the wgpu device, the host windows (through `av-platform`'s host
//! thread) and one surface per enabled placement. The main thread only sends it an [`Intent`]:
//! which placements are on, with which scene and fps. The render thread resolves geometry itself
//! (monitors, DPI, taskbar) and re-resolves it on display and Explorer events, so nothing here
//! queries windows per frame (invariant 10).
//!
//! Per loop: read the newest frame from the engine's slot ring once, then compose and present
//! every target that is due at its own fps cap (Mailbox presentation; the timer paces). A window
//! is shown only after its first frame (invariant 15). With no target the thread parks.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use av_engine::FrameReader;
use av_engine::slot_ring::FrameCopy;
use av_platform::geometry::{Rect, background_rect, pick_monitor, resolve_anchor, taskbar_zone};
use av_platform::{HostEvent, HostKind, HostWindow, Hosts};
use av_protocol::config::AppConfig;
use av_protocol::placement::InputMode;
use av_protocol::scene::Scene;
use av_render::{Composer, Gpu, Mesh, SurfaceTarget, TargetSpec, View};
use serde::Serialize;

/// Which placement a target is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Placement {
    Background,
    Taskbar,
    Overlay,
}

/// What the main thread wants on screen.
#[derive(Debug, Clone)]
pub struct Intent {
    pub config: AppConfig,
    /// The scene each enabled placement draws (already resolved from the store).
    pub scenes: Vec<(Placement, Scene)>,
}

/// One display's state, for the Studio.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayState {
    pub placement: Placement,
    /// `running`, `starting` or an error text.
    pub state: String,
    pub rect: [i32; 4],
    pub fps: f32,
}

enum Msg {
    Intent(Box<Intent>),
    Stop,
}

/// Where the current frames come from (the active engine changes with the source).
pub type FrameSource = Arc<dyn Fn() -> Option<FrameReader> + Send + Sync>;

pub struct Displays {
    tx: Sender<Msg>,
    thread: Option<JoinHandle<()>>,
    states: Arc<std::sync::Mutex<Vec<DisplayState>>>,
}

impl std::fmt::Debug for Displays {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Displays").finish_non_exhaustive()
    }
}

impl Displays {
    pub fn start(frames: FrameSource) -> Self {
        let (tx, rx) = mpsc::channel();
        let states = Arc::new(std::sync::Mutex::new(Vec::new()));
        let theirs = states.clone();
        let thread = std::thread::Builder::new()
            .name("av-render".into())
            .spawn(move || RenderThread::new(frames, theirs).run(rx))
            .ok();
        Self { tx, thread, states }
    }

    pub fn apply(&self, intent: Intent) {
        let _ = self.tx.send(Msg::Intent(Box::new(intent)));
    }

    pub fn states(&self) -> Vec<DisplayState> {
        self.states.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// Hides and destroys every display, then stops the thread (shutdown order, blueprint 4.3).
    pub fn stop(&mut self) {
        let _ = self.tx.send(Msg::Stop);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Displays {
    fn drop(&mut self) {
        self.stop();
    }
}

struct Target {
    // Field order is drop order: the surface (and its pending texture) before the window.
    surface: SurfaceTarget,
    placement: Placement,
    window: HostWindow,
    kind: HostKind,
    rect: Rect,
    scale: f32,
    scene: Scene,
    interval: Duration,
    next: Instant,
    shown: bool,
    composer: Composer,
    mesh: Mesh,
    frames: u32,
    fps_window: Instant,
    fps: f32,
}

struct RenderThread {
    frames: FrameSource,
    states: Arc<std::sync::Mutex<Vec<DisplayState>>>,
    hosts: Option<Hosts>,
    gpu: Option<Gpu>,
    targets: Vec<Target>,
    errors: Vec<(Placement, String)>,
    intent: Option<Intent>,
    reader: Option<FrameReader>,
    reader_checked: Instant,
    copy: FrameCopy,
    have_frame: bool,
}

const READER_REFRESH: Duration = Duration::from_millis(500);
const IDLE_WAIT: Duration = Duration::from_millis(500);

impl RenderThread {
    fn new(frames: FrameSource, states: Arc<std::sync::Mutex<Vec<DisplayState>>>) -> Self {
        Self {
            frames,
            states,
            hosts: None,
            gpu: None,
            targets: Vec::new(),
            errors: Vec::new(),
            intent: None,
            reader: None,
            reader_checked: Instant::now() - READER_REFRESH,
            copy: FrameCopy::default(),
            have_frame: false,
        }
    }

    fn run(mut self, rx: Receiver<Msg>) {
        loop {
            let wait = self
                .targets
                .iter()
                .map(|t| t.next.saturating_duration_since(Instant::now()))
                .min()
                .unwrap_or(IDLE_WAIT);
            match rx.recv_timeout(wait) {
                Ok(Msg::Intent(i)) => {
                    self.intent = Some(*i);
                    self.reconcile();
                }
                Ok(Msg::Stop) | Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {}
            }
            let mut changed = false;
            let mut explorer = false;
            if let Some(h) = &self.hosts {
                while let Ok(e) = h.events.try_recv() {
                    changed = true;
                    explorer |= e == HostEvent::ExplorerRestarted;
                }
            }
            if explorer {
                // Taskbar and wallpaper parents are gone: rebuild those windows.
                self.drop_targets(|t| t.placement != Placement::Overlay);
            }
            if changed {
                // Let the shell settle before measuring.
                std::thread::sleep(Duration::from_millis(300));
                self.reconcile();
            }
            self.render_due();
        }
        self.drop_targets(|_| true);
        self.publish_states();
    }

    /// Creates, moves, restyles or removes targets to match the intent.
    fn reconcile(&mut self) {
        let Some(intent) = self.intent.clone() else {
            return;
        };
        let cfg = &intent.config;
        let wanted: Vec<Placement> = [
            (Placement::Background, cfg.background.enabled),
            (Placement::Taskbar, cfg.taskbar.enabled),
            (Placement::Overlay, cfg.overlay.enabled),
        ]
        .into_iter()
        .filter_map(|(p, on)| on.then_some(p))
        .collect();
        self.drop_targets(|t| !wanted.contains(&t.placement));
        self.errors.retain(|(p, _)| wanted.contains(p));
        if wanted.is_empty() {
            self.publish_states();
            return;
        }
        if self.hosts.is_none() {
            match Hosts::start() {
                Ok(h) => self.hosts = Some(h),
                Err(e) => {
                    self.errors = wanted.iter().map(|&p| (p, e.clone())).collect();
                    self.publish_states();
                    return;
                }
            }
        }
        let hosts = self.hosts.as_ref().expect("started above");
        let monitors = hosts.monitors();
        let bar = hosts.taskbar();
        let virtual_desktop = monitors.iter().fold(Rect::default(), |acc, m| {
            if acc.is_empty() {
                return m.rect;
            }
            let x = acc.x.min(m.rect.x);
            let y = acc.y.min(m.rect.y);
            Rect::new(
                x,
                y,
                acc.right().max(m.rect.right()) - x,
                acc.bottom().max(m.rect.bottom()) - y,
            )
        });
        for placement in wanted {
            let scene = intent
                .scenes
                .iter()
                .find(|(p, _)| *p == placement)
                .map(|(_, s)| s.clone())
                .unwrap_or_default();
            let resolved = match placement {
                Placement::Background => pick_monitor(&monitors, cfg.background.monitor.as_deref())
                    .map(|m| {
                        (
                            background_rect(m, cfg.background.scale),
                            m.scale,
                            HostKind::Background,
                            cfg.background.fps,
                        )
                    })
                    .ok_or_else(|| "no monitor".to_string()),
                Placement::Taskbar => match bar {
                    Some(bar) if bar.auto_hide => {
                        Err("the taskbar auto-hides; the strip needs a visible bar".to_string())
                    }
                    Some(bar) => {
                        let scale = monitors
                            .iter()
                            .find(|m| m.rect.clip(&bar.rect) == bar.rect)
                            .map_or(1.0, |m| m.scale);
                        Ok((
                            taskbar_zone(
                                &bar,
                                cfg.taskbar.zone,
                                cfg.taskbar.length,
                                cfg.taskbar.offset,
                                (cfg.taskbar.margin_dip * scale).round() as i32,
                            ),
                            scale,
                            HostKind::Taskbar,
                            cfg.taskbar.fps,
                        ))
                    }
                    None => Err("no taskbar found".to_string()),
                },
                Placement::Overlay => {
                    pick_monitor(&monitors, cfg.overlay.anchor.monitor.as_deref())
                        .map(|m| {
                            (
                                resolve_anchor(&cfg.overlay.anchor, m, virtual_desktop),
                                m.scale,
                                HostKind::Overlay {
                                    click_through: cfg.overlay.input == InputMode::Locked,
                                    capture_visible: cfg.overlay.capture_visible,
                                },
                                cfg.overlay.fps,
                            )
                        })
                        .ok_or_else(|| "no monitor".to_string())
                }
            };
            self.errors.retain(|(p, _)| *p != placement);
            match resolved {
                Ok((rect, scale, kind, fps)) if !rect.is_empty() => {
                    self.ensure(placement, kind, rect, scale, scene, fps);
                }
                Ok(_) => self
                    .errors
                    .push((placement, "the area is empty".to_string())),
                Err(e) => {
                    self.drop_targets(|t| t.placement == placement);
                    self.errors.push((placement, e));
                }
            }
        }
        self.publish_states();
    }

    fn ensure(
        &mut self,
        placement: Placement,
        kind: HostKind,
        rect: Rect,
        scale: f32,
        scene: Scene,
        fps: u32,
    ) {
        let interval = Duration::from_secs_f64(1.0 / fps.clamp(5, 240) as f64);
        if let Some(i) = self.targets.iter().position(|t| t.placement == placement) {
            if self.targets[i].kind == kind {
                let hosts = self.hosts.as_ref().expect("hosts exist with targets");
                let t = &mut self.targets[i];
                if t.rect != rect {
                    hosts.place(&t.window, rect);
                    if let Some(gpu) = &self.gpu {
                        t.surface.resize(gpu, rect.w as u32, rect.h as u32);
                    }
                    t.rect = rect;
                }
                t.scale = scale;
                t.scene = scene;
                t.interval = interval;
                return;
            }
            // Click-through or capture changed: these are creation-time styles.
            self.drop_targets(|t| t.placement == placement);
        }
        match self.create(placement, kind, rect, scale, scene, interval) {
            Ok(t) => self.targets.push(t),
            Err(e) => {
                tracing::warn!(target: "av_app::displays", ?placement, "display failed: {e}");
                self.errors.push((placement, e));
            }
        }
    }

    fn create(
        &mut self,
        placement: Placement,
        kind: HostKind,
        rect: Rect,
        scale: f32,
        scene: Scene,
        interval: Duration,
    ) -> Result<Target, String> {
        let hosts = self.hosts.as_ref().ok_or("no host thread")?;
        let window = hosts.create(kind, rect)?;
        let handle = window.raw_window_handle().ok_or("no window handle")?;
        let destroy = |e: String| {
            hosts.destroy(&window);
            e
        };
        let instance = match &self.gpu {
            Some(g) => g.instance.clone(),
            None => av_render::instance(),
        };
        // SAFETY: the host window outlives the surface: a target drops its surface before it
        // asks the host thread to destroy the window (see `drop_targets`).
        let display = Some(window.raw_display_handle());
        let surface = unsafe { SurfaceTarget::create(&instance, handle, display) }
            .map_err(|e| destroy(e.to_string()))?;
        if self.gpu.is_none() {
            let gpu = Gpu::new(instance, Some(&surface)).map_err(|e| destroy(e.to_string()))?;
            tracing::info!(target: "av_app::displays", adapter = %gpu.adapter_name(), "renderer ready");
            self.gpu = Some(gpu);
        }
        let gpu = self.gpu.as_ref().expect("created above");
        let surface = SurfaceTarget::new(
            gpu,
            surface,
            TargetSpec {
                width: rect.w as u32,
                height: rect.h as u32,
                vsync: false,
                allow_opaque: placement == Placement::Background,
            },
        )
        .map_err(|e| destroy(e.to_string()))?;
        tracing::info!(target: "av_app::displays", ?placement, alpha = ?surface.alpha_mode(), "display created");
        Ok(Target {
            surface,
            placement,
            window,
            kind,
            rect,
            scale,
            scene,
            interval,
            next: Instant::now(),
            shown: false,
            composer: Composer::default(),
            mesh: Mesh::default(),
            frames: 0,
            fps_window: Instant::now(),
            fps: 0.0,
        })
    }

    fn drop_targets(&mut self, which: impl Fn(&Target) -> bool) {
        let (gone, keep): (Vec<Target>, Vec<Target>) = self.targets.drain(..).partition(which);
        self.targets = keep;
        for t in gone {
            let window = t.window;
            drop(t); // surface first
            if let Some(h) = &self.hosts {
                h.destroy(&window);
            }
        }
    }

    fn render_due(&mut self) {
        if self.targets.is_empty() {
            return;
        }
        let now = Instant::now();
        if now.duration_since(self.reader_checked) >= READER_REFRESH {
            self.reader = (self.frames)();
            self.reader_checked = now;
        }
        if !self.targets.iter().any(|t| t.next <= now) {
            return;
        }
        self.have_frame = match &self.reader {
            Some(r) => r.read_latest(&mut self.copy, false).is_ok(),
            None => false,
        };
        let Some(gpu) = self.gpu.as_mut() else { return };
        let mut publish = false;
        for t in self.targets.iter_mut() {
            if t.next > now {
                continue;
            }
            t.next = (t.next + t.interval).max(now);
            let (bars, wave): (&[f32], Option<&[f32]>) = if self.have_frame {
                (self.copy.bars(), self.copy.waveform())
            } else {
                (&[], None)
            };
            let (w, h) = t.surface.size();
            t.composer.compose(
                &t.scene,
                bars,
                wave,
                View {
                    width: w as f32,
                    height: h as f32,
                    scale: t.scale,
                    display_count: None,
                },
                &mut t.mesh,
            );
            if t.surface.render(gpu, &t.mesh, [0.0; 4]) {
                t.frames += 1;
                if !t.shown {
                    if let Some(hosts) = &self.hosts {
                        hosts.show(&t.window);
                    }
                    t.shown = true;
                    publish = true;
                }
            }
            let elapsed = now.duration_since(t.fps_window);
            if elapsed >= Duration::from_secs(1) {
                t.fps = t.frames as f32 / elapsed.as_secs_f32();
                t.frames = 0;
                t.fps_window = now;
                publish = true;
            }
        }
        if publish {
            self.publish_states();
        }
    }

    fn publish_states(&self) {
        let mut out: Vec<DisplayState> = self
            .targets
            .iter()
            .map(|t| DisplayState {
                placement: t.placement,
                state: if t.shown { "running" } else { "starting" }.into(),
                rect: [t.rect.x, t.rect.y, t.rect.w, t.rect.h],
                fps: t.fps,
            })
            .collect();
        out.extend(self.errors.iter().map(|(p, e)| DisplayState {
            placement: *p,
            state: e.clone(),
            rect: [0; 4],
            fps: 0.0,
        }));
        if let Ok(mut s) = self.states.lock() {
            *s = out;
        }
    }
}
