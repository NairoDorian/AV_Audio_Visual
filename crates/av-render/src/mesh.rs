//! Scene -> triangles. Pure and allocation-free once the buffers have grown: the same geometry as
//! the Studio preview painters (`src/lib/render/*.ts`), produced on the CPU in normalized device
//! coordinates so no per-target uniform is needed (AGENTS.md invariant 8: a shared uniform written
//! per target gives later targets the wrong origin).
//!
//! Every colour is premultiplied here (`rgb * a`), and the pipeline blends with
//! `One, OneMinusSrcAlpha`, so a non-sRGB UNORM target holds valid premultiplied values for DWM.

use av_protocol::scene::{Scene, VisualStyle};

use crate::color::{Palette, Rgba, parse};

/// One vertex: position in clip space, premultiplied colour.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: [f32; 2],
    pub color: [f32; 4],
}

/// A triangle list in pixels, converted to clip space on push.
#[derive(Debug, Clone, Default)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    sx: f32,
    sy: f32,
}

impl Mesh {
    /// Clears the mesh for a `width` x `height` pixel target (keeps the capacity).
    pub fn begin(&mut self, width: f32, height: f32) {
        self.vertices.clear();
        self.sx = 2.0 / width.max(1.0);
        self.sy = 2.0 / height.max(1.0);
    }

    #[inline]
    fn v(&mut self, x: f32, y: f32, c: Rgba) {
        self.vertices.push(Vertex {
            pos: [x * self.sx - 1.0, 1.0 - y * self.sy],
            color: c.premultiplied(),
        });
    }

    /// A quad from four corners (a, b, c, d in order around the quad).
    #[allow(clippy::too_many_arguments)]
    pub fn quad(&mut self, p: [(f32, f32); 4], c: [Rgba; 4]) {
        for i in [0usize, 1, 2, 0, 2, 3] {
            self.v(p[i].0, p[i].1, c[i]);
        }
    }

    /// An axis-aligned rectangle, `bottom` colour at `y1`, `top` at `y0`.
    pub fn rect(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, top: Rgba, bottom: Rgba) {
        self.quad(
            [(x0, y0), (x1, y0), (x1, y1), (x0, y1)],
            [top, top, bottom, bottom],
        );
    }

    /// A line segment `width` pixels thick (no caps).
    pub fn segment(&mut self, a: (f32, f32), b: (f32, f32), width: f32, ca: Rgba, cb: Rgba) {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-6 {
            return;
        }
        let (nx, ny) = (-dy / len * width * 0.5, dx / len * width * 0.5);
        self.quad(
            [
                (a.0 + nx, a.1 + ny),
                (b.0 + nx, b.1 + ny),
                (b.0 - nx, b.1 - ny),
                (a.0 - nx, a.1 - ny),
            ],
            [ca, cb, cb, ca],
        );
    }
}

/// Peak-pools `src` into `out` (each bucket's maximum; more slots than values repeat values).
pub fn pool_peak(src: &[f32], out: &mut [f32]) {
    let n = out.len();
    let len = src.len();
    if len == 0 {
        out.fill(0.0);
        return;
    }
    let bucket = len as f32 / n as f32;
    for (b, o) in out.iter_mut().enumerate() {
        let i0 = ((b as f32 * bucket) as usize).min(len - 1);
        let i1 = (((b + 1) as f32 * bucket) as usize).max(i0 + 1).min(len);
        *o = src[i0..i1].iter().fold(0.0f32, |m, &v| m.max(v));
    }
}

/// Mirrored cross-sum at ring point `k` of `2n` (docs/super-fft.md 3.1).
#[inline]
pub fn ring_signal(k: usize, s: &[f32]) -> f32 {
    let n = s.len();
    let j = if k < n { k } else { k - n };
    (s[j] + s[n - 1 - j]) * 0.5
}

/// Angle of point `k` of `points`, the seam straddling 3 o'clock.
#[inline]
pub fn ring_angle(k: usize, points: usize) -> f32 {
    (k as f32 + 0.5) / points as f32 * std::f32::consts::TAU
}

/// What the composer draws besides the scene: a display-count override and the target's scale.
#[derive(Debug, Clone, Copy)]
pub struct View {
    pub width: f32,
    pub height: f32,
    /// Device pixels per CSS pixel (line widths and spacing are in CSS pixels).
    pub scale: f32,
    /// Display bars / ring bins override (`None` = the scene's own).
    pub display_count: Option<u32>,
}

/// Reusable buffers for composing frames.
#[derive(Debug, Default)]
pub struct Composer {
    pooled: Vec<f32>,
    palette: Palette,
    palette_key: Vec<String>,
}

impl Composer {
    /// Builds the mesh for one frame of `scene`: the output bins (`bars`, 0..1) and the tapered
    /// raw-audio window (`wave`). Allocates only when a buffer must grow.
    pub fn compose(
        &mut self,
        scene: &Scene,
        bars: &[f32],
        wave: Option<&[f32]>,
        view: View,
        mesh: &mut Mesh,
    ) {
        mesh.begin(view.width, view.height);
        if self.palette_key != scene.palette {
            self.palette = Palette::new(&scene.palette);
            self.palette_key.clone_from(&scene.palette);
        }
        if bars.is_empty() {
            return;
        }
        let opacity = scene.opacity.clamp(0.0, 1.0);
        let colors = &scene.colors;
        let scope_color = colors
            .scope
            .as_deref()
            .and_then(parse)
            .unwrap_or_else(|| self.palette.last());
        match scene.style {
            VisualStyle::Concentric => {
                self.ring(scene, bars, view, opacity, mesh);
                if let Some(w) = wave.filter(|_| scene.scope.enabled) {
                    let r = scene.ring.radius * view.width.min(view.height) * 0.5;
                    let half = r * scene.scope.size;
                    scope(
                        mesh,
                        w,
                        (view.width / 2.0, view.height / 2.0),
                        half,
                        half * 0.5 * scene.scope.gain,
                        Traces {
                            horizontal_mirror: scene.scope.horizontal_mirror,
                            vertical_mirror: scene.scope.vertical_mirror,
                            copies: scene.scope.copies.max(1),
                            width: scene.scope.line_width * view.scale,
                            color: scope_color.with_alpha(opacity),
                            clip: Some(r),
                        },
                    );
                }
            }
            _ => {
                self.bars(scene, bars, view, opacity, mesh);
                if let Some(w) = wave.filter(|_| scene.scope.enabled) {
                    scope(
                        mesh,
                        w,
                        (view.width / 2.0, view.height / 2.0),
                        view.width / 2.0 * scene.scope.size,
                        view.height / 2.0 * scene.scope.gain,
                        Traces {
                            horizontal_mirror: scene.scope.horizontal_mirror,
                            vertical_mirror: scene.scope.vertical_mirror,
                            copies: 1,
                            width: scene.scope.line_width * view.scale,
                            color: scope_color.with_alpha(opacity),
                            clip: None,
                        },
                    );
                }
            }
        }
    }

    fn bars(&mut self, scene: &Scene, bars: &[f32], view: View, opacity: f32, mesh: &mut Mesh) {
        let n = view.display_count.unwrap_or(scene.bar_count).max(1) as usize;
        let values: &[f32] = if n >= bars.len() {
            bars
        } else {
            self.pooled.resize(n, 0.0);
            pool_peak(bars, &mut self.pooled);
            &self.pooled
        };
        let (w, h) = (view.width, view.height);
        let slot = w / values.len() as f32;
        let bw = (slot * scene.bar_width).max(1.0);
        let custom = scene.colors.bars.as_deref().and_then(parse);
        let mirrored = scene.style == VisualStyle::MirroredBars;
        let bottom = custom.unwrap_or_else(|| self.palette.at(0.0));
        for (i, &v) in values.iter().enumerate() {
            let v = v.clamp(0.0, 1.0);
            let bh = v * h;
            if bh < 0.5 {
                continue;
            }
            let x0 = i as f32 * slot + (slot - bw) / 2.0;
            let top = custom.unwrap_or_else(|| self.palette.at(v));
            if mirrored {
                let mid = h / 2.0;
                let half = bh / 2.0;
                let edge = custom.unwrap_or_else(|| self.palette.at(0.5 + v * 0.5));
                let centre = custom.unwrap_or_else(|| self.palette.at(0.5));
                mesh.rect(
                    x0,
                    mid - half,
                    x0 + bw,
                    mid,
                    edge.with_alpha(opacity),
                    centre.with_alpha(opacity),
                );
                mesh.rect(
                    x0,
                    mid,
                    x0 + bw,
                    mid + half,
                    centre.with_alpha(opacity),
                    edge.with_alpha(opacity),
                );
            } else {
                mesh.rect(
                    x0,
                    h - bh,
                    x0 + bw,
                    h,
                    top.with_alpha(opacity),
                    bottom.with_alpha(opacity),
                );
            }
        }
    }

    fn ring(&mut self, scene: &Scene, bars: &[f32], view: View, opacity: f32, mesh: &mut Mesh) {
        let ring = &scene.ring;
        let n = view.display_count.unwrap_or(ring.bins).max(2) as usize;
        self.pooled.resize(n, 0.0);
        pool_peak(bars, &mut self.pooled);
        let floor = ring.floor.clamp(0.0, 0.99);
        for v in self.pooled.iter_mut() {
            *v = if *v <= floor {
                0.0
            } else {
                ((*v - floor) / (1.0 - floor) * ring.gain).min(1.0)
            };
        }
        let s = &self.pooled;
        let (cx, cy) = (view.width / 2.0, view.height / 2.0);
        let r = ring.radius * view.width.min(view.height) * 0.5;
        let points = 2 * n;
        let colors = &scene.colors;
        let pal = &self.palette;
        // Radial gradient as the preview's: first stop to R, the palette spread over R..2R.
        let radial = |d: f32, custom: &Option<String>| -> Rgba {
            custom.as_deref().and_then(parse).unwrap_or_else(|| {
                let u = d / (2.0 * r).max(1e-6);
                if u <= 0.5 {
                    pal.at(0.0)
                } else {
                    pal.at((u - 0.5) * 2.0)
                }
            })
        };
        if ring.base_circle {
            let c = colors
                .base_circle
                .as_deref()
                .and_then(parse)
                .unwrap_or_else(|| pal.at(0.0))
                .with_alpha(0.3 * opacity);
            let segs = 128;
            for i in 0..segs {
                let (a0, a1) = (
                    i as f32 / segs as f32 * std::f32::consts::TAU,
                    (i + 1) as f32 / segs as f32 * std::f32::consts::TAU,
                );
                mesh.segment(
                    (cx + a0.cos() * r, cy + a0.sin() * r),
                    (cx + a1.cos() * r, cy + a1.sin() * r),
                    view.scale,
                    c,
                    c,
                );
            }
        }
        if ring.outer_bars || ring.inner_bars || ring.span_bars {
            let width = (std::f32::consts::TAU * r / points as f32 - ring.bar_spacing * view.scale)
                .max(1.0);
            for k in 0..points {
                let c = ring_signal(k, s);
                if c <= 0.0 {
                    continue;
                }
                let th = ring_angle(k, points);
                let (co, si) = (th.cos(), th.sin());
                let d = c * ring.bar_scale;
                let alpha = (0.35 + 0.65 * c) * opacity;
                let mut bar = |a: f32, b: f32, custom: &Option<String>| {
                    let (ra, rb) = (r * a, r * b);
                    mesh.segment(
                        (cx + co * ra, cy + si * ra),
                        (cx + co * rb, cy + si * rb),
                        width,
                        radial(ra, custom).with_alpha(alpha),
                        radial(rb, custom).with_alpha(alpha),
                    );
                };
                if ring.span_bars {
                    bar(1.0 - d, 1.0 + d, &colors.span_bars);
                } else {
                    if ring.outer_bars {
                        bar(1.0, 1.0 + d, &colors.outer_bars);
                    }
                    if ring.inner_bars {
                        bar(1.0 - d, 1.0, &colors.inner_bars);
                    }
                }
            }
        }
        let line_width = 1.5 * view.scale;
        let mut loop_line = |sign: f32, custom: &Option<String>, alpha: f32| {
            let at = |k: usize| {
                let rr = r * (1.0 + sign * ring_signal(k % points, s));
                let th = ring_angle(k % points, points);
                ((cx + th.cos() * rr, cy + th.sin() * rr), rr)
            };
            for k in 0..points {
                let (a, ra) = at(k);
                let (b, rb) = at(k + 1);
                mesh.segment(
                    a,
                    b,
                    line_width,
                    radial(ra, custom).with_alpha(alpha * opacity),
                    radial(rb, custom).with_alpha(alpha * opacity),
                );
            }
        };
        if ring.inner_line {
            loop_line(-1.0, &colors.inner_line, 0.75);
        }
        if ring.outer_line {
            loop_line(1.0, &colors.outer_line, 1.0);
        }
    }
}

/// How the scope traces are laid out (docs/super-fft.md 4.3).
#[derive(Debug, Clone, Copy)]
pub struct Traces {
    pub horizontal_mirror: bool,
    pub vertical_mirror: bool,
    pub copies: u32,
    pub width: f32,
    pub color: Rgba,
    /// Clip to this radius around the centre (inside the ring).
    pub clip: Option<f32>,
}

/// Sign pairs of every trace variant before rotation (base, time-reversed, inverted, both).
pub fn variants(h: bool, v: bool) -> &'static [(f32, f32)] {
    match (h, v) {
        (false, false) => &[(1.0, 1.0)],
        (true, false) => &[(1.0, 1.0), (-1.0, 1.0)],
        (false, true) => &[(1.0, 1.0), (1.0, -1.0)],
        (true, true) => &[(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)],
    }
}

/// Draws the scope traces centred on `c`: each spans `2 * half` pixels and full scale reaches
/// `amp` pixels; copy `i` is rotated by `i * 180 / copies` degrees.
pub fn scope(mesh: &mut Mesh, wave: &[f32], c: (f32, f32), half: f32, amp: f32, t: Traces) {
    let n = wave.len();
    if n < 2 {
        return;
    }
    let step = 2.0 * half / (n - 1) as f32;
    let clip2 = t.clip.map(|r| r * r);
    for i in 0..t.copies {
        let a = i as f32 * std::f32::consts::PI / t.copies as f32;
        let (co, si) = (a.cos(), a.sin());
        for &(sx, sy) in variants(t.horizontal_mirror, t.vertical_mirror) {
            let point = |j: usize| {
                let x = sx * (-half + j as f32 * step);
                let y = -sy * wave[j] * amp;
                (c.0 + x * co - y * si, c.1 + x * si + y * co)
            };
            let mut prev = point(0);
            for j in 1..n {
                let p = point(j);
                let inside = |q: (f32, f32)| {
                    clip2.is_none_or(|r2| {
                        let (dx, dy) = (q.0 - c.0, q.1 - c.1);
                        dx * dx + dy * dy <= r2
                    })
                };
                if inside(prev) && inside(p) {
                    mesh.segment(prev, p, t.width, t.color, t.color);
                }
                prev = p;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use av_protocol::scene::{Scene, builtin};

    fn view(w: f32, h: f32) -> View {
        View {
            width: w,
            height: h,
            scale: 1.0,
            display_count: None,
        }
    }

    #[test]
    fn pooling_keeps_peaks_and_repeats_when_stretched() {
        let mut out = [0.0; 3];
        pool_peak(&[0.1, 0.9, 0.2, 0.3, 0.5, 0.4], &mut out);
        assert_eq!(out, [0.9, 0.3, 0.5]);
        let mut wide = [0.0; 4];
        pool_peak(&[0.2, 0.8], &mut wide);
        assert_eq!(wide, [0.2, 0.2, 0.8, 0.8]);
    }

    #[test]
    fn the_ring_signal_is_a_palindrome_repeated_twice() {
        let s = [0.1, 0.4, 0.9, 0.3, 0.0];
        for k in 0..10 {
            assert!((ring_signal(k, &s) - ring_signal(9 - k, &s)).abs() < 1e-6);
            assert!((ring_signal(k, &s) - ring_signal((k + 5) % 10, &s)).abs() < 1e-6);
        }
    }

    #[test]
    fn bars_fill_their_height_in_clip_space() {
        let scene = Scene {
            bar_count: 4,
            ..Scene::default()
        };
        let mut c = Composer::default();
        let mut m = Mesh::default();
        c.compose(
            &scene,
            &[1.0, 0.5, 0.0, 0.25],
            None,
            view(400.0, 100.0),
            &mut m,
        );
        // Three bars drawn (the zero bar is skipped), 6 vertices each.
        assert_eq!(m.vertices.len(), 18);
        let ys: Vec<f32> = m.vertices.iter().map(|v| v.pos[1]).collect();
        assert!(
            ys.iter().any(|&y| (y - 1.0).abs() < 1e-5),
            "full bar reaches the top"
        );
        assert!(ys.iter().all(|&y| (-1.0..=1.0).contains(&y)));
    }

    #[test]
    fn every_colour_is_premultiplied() {
        let mut scene = builtin("concentric");
        scene.opacity = 0.6;
        scene.scope.horizontal_mirror = true;
        scene.scope.copies = 3;
        scene.colors.outer_line = Some("#ff000080".into());
        let bars: Vec<f32> = (0..256).map(|i| i as f32 / 255.0).collect();
        let wave: Vec<f32> = (0..800).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
        let mut c = Composer::default();
        let mut m = Mesh::default();
        c.compose(&scene, &bars, Some(&wave), view(800.0, 600.0), &mut m);
        assert!(!m.vertices.is_empty());
        for v in &m.vertices {
            let [r, g, b, a] = v.color;
            assert!(
                r <= a + 1e-6 && g <= a + 1e-6 && b <= a + 1e-6,
                "{:?}",
                v.color
            );
            assert!(a <= 0.6 + 1e-6);
        }
    }

    #[test]
    fn scope_traces_multiply_and_clip_to_the_ring() {
        let wave = vec![0.0f32; 64];
        let count = |h, v, copies| {
            let mut m = Mesh::default();
            m.begin(200.0, 200.0);
            scope(
                &mut m,
                &wave,
                (100.0, 100.0),
                50.0,
                10.0,
                Traces {
                    horizontal_mirror: h,
                    vertical_mirror: v,
                    copies,
                    width: 1.0,
                    color: Rgba::WHITE,
                    clip: Some(40.0),
                },
            );
            m.vertices.len()
        };
        let one = count(false, false, 1);
        assert!(one > 0);
        assert_eq!(count(true, true, 2), one * 8);
    }
}
