//! The output axis (FFT CHOP "Spectrum" page): perceptual scales, the warp-blended frequency
//! grid, and the re-mapping of the linear FFT magnitude onto it.
//!
//! Every output bin `i` has a frequency `target_hz[i]`: the chosen scale sampled uniformly
//! between its values at the axis ends, crossfaded with a linear-in-Hz grid by the warp blend
//! (0 = linear whatever the scale, 1 = the scale's own axis) and clamped to `[0, fmax]`. An
//! output bin reads the magnitude at its fractional FFT position with a linear or Catmull-Rom
//! kernel; where it owns two or more FFT bins (the coarse end of a perceptual axis) it can
//! report their peak or RMS instead, so a narrow partial between two taps is never lost. An
//! exactly 1:1 grid is detected and copied (Raw RFFT Bins).
//!
//! Ported from ZER0 `src-tauri/src/live_fft/dsp.rs` sections 4 (MIT, same author), itself a
//! port of the FFT CHOP `computeTargetHzGrid` / `PerceptualWarping`; the formulas are the
//! published HTK mel, Moore & Glasberg ERB polynomial, Traunmuller Bark and MIDI pitch. The
//! AVX2 kernels of ZER0 are not ported yet; these are its portable reference kernels.

use av_protocol::config::{Scale, WarpAggregation, WarpInterp};

pub fn htk_hz_to_mel(hz: f64) -> f64 {
    2595.0 * (1.0 + hz / 700.0).log10()
}

pub fn htk_mel_to_hz(mel: f64) -> f64 {
    700.0 * (10f64.powf(mel / 2595.0) - 1.0)
}

/// Moore & Glasberg's (1983) ERB *bandwidth* polynomial used as a warp with its own `hz / 123`
/// unit (the CHOP's ERB axis: a roughly square-root warp).
pub fn erb_rate_glasberg(hz: f64) -> f64 {
    let x = hz / 123.0;
    6.230 * (x * x) + 93.390 * x + 28.520
}

/// Inverse of [`erb_rate_glasberg`] (the positive root of the quadratic).
pub fn erb_rate_to_hz(erb: f64) -> f64 {
    let (a, b, c) = (6.230, 93.390, 28.520);
    let x = (-b + (b * b - 4.0 * a * (c - erb)).max(0.0).sqrt()) / (2.0 * a);
    x * 123.0
}

/// Traunmuller (1990) Bark with his low (< 2 Bark) and high (> 20.1 Bark) corrections.
pub fn hz_to_bark(hz: f64) -> f64 {
    let mut z = (26.81 * hz) / (1960.0 + hz) - 0.53;
    if z < 2.0 {
        z += 0.15 * (2.0 - z);
    } else if z > 20.1 {
        z += 0.22 * (z - 20.1);
    }
    z
}

/// Exact inverse of [`hz_to_bark`]: both corrections are linear and undone first.
pub fn bark_to_hz(bark: f64) -> f64 {
    let mut z = bark;
    if z < 2.0 {
        z = (z - 0.3) / 0.85;
    } else if z > 20.1 {
        z = (z + 4.422) / 1.22;
    }
    ((1960.0 * (z + 0.53)) / (26.81 - (z + 0.53))).max(0.0)
}

/// MIDI pitch (A4 = 69 = 440 Hz).
pub fn hz_to_chroma(hz: f64) -> f64 {
    12.0 * (hz.max(1e-5) / 440.0).log2() + 69.0
}

pub fn chroma_to_hz(chroma: f64) -> f64 {
    440.0 * 2f64.powf((chroma - 69.0) / 12.0)
}

/// Frequency of output bin `i` of `n_out`: the perceptual grid blended with a linear one by
/// `warp_blend`, clamped to `[0, fmax]`. The last bin is exactly `fmax` for every scale.
pub fn target_hz_at(
    scale: Scale,
    fmax: f64,
    i: usize,
    n_out: usize,
    warp_blend: f64,
    log_floor_hz: f64,
) -> f64 {
    if n_out > 1 && i + 1 == n_out {
        // Pinned rather than computed: exp(ln(fmax)) can land one ulp below.
        return fmax;
    }
    let frac = if n_out > 1 {
        i as f64 / (n_out - 1) as f64
    } else {
        0.0
    };
    let log_axis = || {
        let lo = log_floor_hz.max(1.0).ln();
        (lo + frac * (fmax.ln() - lo)).exp()
    };
    let mel_axis = || {
        let (m0, m1) = (htk_hz_to_mel(0.0), htk_hz_to_mel(fmax));
        htk_mel_to_hz(m0 + frac * (m1 - m0))
    };
    let perceptual = match scale {
        Scale::Log => log_axis(),
        Scale::Mel => mel_axis(),
        Scale::Erb => {
            let (e0, e1) = (erb_rate_glasberg(0.0), erb_rate_glasberg(fmax));
            erb_rate_to_hz(e0 + frac * (e1 - e0))
        }
        Scale::Bark => {
            let (b0, b1) = (hz_to_bark(0.0), hz_to_bark(fmax));
            bark_to_hz(b0 + frac * (b1 - b0))
        }
        // Pitch classes need a positive bottom: a fixed 20 Hz, not the log floor.
        Scale::Chroma => {
            let (c0, c1) = (hz_to_chroma(20.0), hz_to_chroma(fmax));
            chroma_to_hz(c0 + frac * (c1 - c0))
        }
        Scale::Melog => 0.5 * (mel_axis() + log_axis()),
        Scale::Linear => frac * fmax,
    };
    let linear = frac * fmax;
    ((1.0 - warp_blend) * linear + warp_blend * perceptual).clamp(0.0, fmax)
}

/// The axis of one configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Axis {
    pub scale: Scale,
    pub fmax: f64,
    pub n_out: usize,
    pub warp_blend: f64,
    pub log_floor_hz: f64,
    pub interp: WarpInterp,
    pub aggregation: WarpAggregation,
}

/// Re-maps the linear magnitude spectrum onto the output grid. Tables are built once per
/// configuration; [`Warp::apply`] never allocates.
#[derive(Debug, Clone, Default)]
pub struct Warp {
    target_hz: Vec<f64>,
    i0: Vec<u32>,
    w: Vec<f32>,
    nlin: usize,
    max_index: usize,
    interp: WarpInterp,
    aggregation: WarpAggregation,
    is_identity: bool,
    /// Output bins that own two or more FFT bins: the bin, its first FFT bin, and how many.
    agg_idx: Vec<u32>,
    agg_lo: Vec<u32>,
    agg_cnt: Vec<u32>,
    /// Interpolation covers `[0, interp_end)`; everything above is aggregated.
    interp_end: usize,
}

impl Warp {
    /// Builds the tables for `axis` over `nlin` FFT bins spanning `0..=nyquist`.
    pub fn new(axis: Axis, nyquist: f64, nlin: usize) -> Self {
        let n_out = axis.n_out;
        let target_hz: Vec<f64> = (0..n_out)
            .map(|i| {
                target_hz_at(
                    axis.scale,
                    axis.fmax,
                    i,
                    n_out,
                    axis.warp_blend,
                    axis.log_floor_hz,
                )
            })
            .collect();
        let mut i0 = vec![0u32; n_out];
        let mut w = vec![0f32; n_out];
        let denom = if nlin > 1 { (nlin - 1) as f64 } else { 1.0 };
        let max_i0 = nlin.saturating_sub(2);
        let mut max_index = 0;
        let mut is_identity = n_out == nlin;
        let pos = |hz: f64| {
            if nyquist > 0.0 {
                hz / nyquist * denom
            } else {
                0.0
            }
        };
        for (i, &hz) in target_hz.iter().enumerate() {
            let mut frac = pos(hz);
            // Snap integer positions so an exact 1:1 grid is detected (memcpy bypass).
            let rounded = frac.round();
            if (frac - rounded).abs() < 1e-6 {
                frac = rounded;
            }
            let lo = frac.floor().clamp(0.0, max_i0 as f64) as usize;
            let weight = (frac - lo as f64).clamp(0.0, 1.0) as f32;
            i0[i] = lo as u32;
            w[i] = weight;
            max_index = max_index.max((lo + 2).min(nlin.saturating_sub(1)));
            if is_identity && ((lo as f64 + weight as f64) - i as f64).abs() > 1e-5 {
                is_identity = false;
            }
        }

        // Aggregation ranges: bin i owns the FFT bins between the midpoints to its neighbours,
        // `[ceil((p[i-1]+p[i])/2), floor((p[i]+p[i+1])/2)]`; only ranges of two or more count.
        let (mut agg_idx, mut agg_lo, mut agg_cnt) = (Vec::new(), Vec::new(), Vec::new());
        if axis.aggregation != WarpAggregation::Off
            && !is_identity
            && n_out >= 2
            && nlin >= 2
            && nyquist > 0.0
        {
            let last = (nlin - 1) as f64;
            for i in 0..n_out {
                let p = pos(target_hz[i]);
                let lo_pos = if i == 0 {
                    p
                } else {
                    0.5 * (pos(target_hz[i - 1]) + p)
                };
                let hi_pos = if i + 1 == n_out {
                    p
                } else {
                    0.5 * (p + pos(target_hz[i + 1]))
                };
                let lo = (lo_pos - 1e-9).ceil().max(0.0);
                let hi = (hi_pos + 1e-9).floor().min(last);
                if hi - lo < 1.0 {
                    continue;
                }
                let (lo, hi) = (lo as usize, hi as usize);
                agg_idx.push(i as u32);
                agg_lo.push(lo as u32);
                agg_cnt.push((hi - lo + 1) as u32);
                max_index = max_index.max(hi);
            }
        }
        let mut interp_end = n_out;
        for &i in agg_idx.iter().rev() {
            if i as usize + 1 != interp_end {
                break;
            }
            interp_end -= 1;
        }
        Self {
            target_hz,
            i0,
            w,
            nlin,
            max_index,
            interp: axis.interp,
            aggregation: axis.aggregation,
            is_identity,
            agg_idx,
            agg_lo,
            agg_cnt,
            interp_end,
        }
    }

    pub fn output_bins(&self) -> usize {
        self.i0.len()
    }

    /// Frequency of every output bin.
    pub fn target_hz(&self) -> &[f64] {
        &self.target_hz
    }

    pub fn is_identity(&self) -> bool {
        self.is_identity
    }

    /// Highest FFT bin the warp reads; magnitudes above it need not be computed.
    pub fn max_linear_index(&self) -> usize {
        self.max_index
    }

    /// Output bins formed by aggregation rather than interpolation.
    pub fn aggregated_bins(&self) -> usize {
        self.agg_idx.len()
    }

    /// Fills `out` (length [`Warp::output_bins`]) from `linear`, which needs only the bins up
    /// to [`Warp::max_linear_index`] (the analyzer computes no more).
    pub fn apply(&self, linear: &[f32], out: &mut [f32]) {
        let n_out = self.i0.len().min(out.len());
        if n_out == 0 {
            return;
        }
        if self.is_identity && linear.len() >= n_out {
            out[..n_out].copy_from_slice(&linear[..n_out]);
            return;
        }
        if linear.len() <= self.max_index || self.nlin < 2 {
            out.fill(0.0);
            return;
        }
        let n_interp = if self.aggregation != WarpAggregation::Off {
            self.interp_end.min(n_out)
        } else {
            n_out
        };
        let (i0, w, dst) = (
            &self.i0[..n_interp],
            &self.w[..n_interp],
            &mut out[..n_interp],
        );
        match self.interp {
            WarpInterp::Cubic if linear.len() >= 4 => warp_cubic(linear, i0, w, dst),
            _ => warp_linear(linear, i0, w, dst),
        }
        self.apply_aggregation(linear, out);
    }

    /// Overwrites the coarse bins with the peak (or RMS) of the FFT bins each owns.
    fn apply_aggregation(&self, src: &[f32], dst: &mut [f32]) {
        if self.aggregation == WarpAggregation::Off {
            return;
        }
        let rms = self.aggregation == WarpAggregation::Rms;
        for ((&i, &lo), &cnt) in self.agg_idx.iter().zip(&self.agg_lo).zip(&self.agg_cnt) {
            let Some(slot) = dst.get_mut(i as usize) else {
                continue;
            };
            let lo = lo as usize;
            if lo >= src.len() {
                *slot = 0.0;
                continue;
            }
            let range = &src[lo..(lo + cnt as usize).min(src.len())];
            *slot = if rms {
                (sum_of_squares(range) / range.len() as f32).sqrt()
            } else {
                max_of(range)
            };
        }
    }
}

/// `a + w * (b - a)`. Precondition: every `i0 + 1 < src.len()`.
fn warp_linear(src: &[f32], i0: &[u32], w: &[f32], dst: &mut [f32]) {
    for ((d, &i0), &w) in dst.iter_mut().zip(i0).zip(w) {
        let i0 = i0 as usize;
        let a = src[i0];
        let b = src[i0 + 1];
        *d = a + w * (b - a);
    }
}

/// Catmull-Rom over four taps (clamped to the spectrum), never below zero.
fn warp_cubic(src: &[f32], i0: &[u32], w: &[f32], dst: &mut [f32]) {
    let last = src.len() - 1;
    for ((d, &i0), &t) in dst.iter_mut().zip(i0).zip(w) {
        let i0 = i0 as usize;
        let p0 = src[i0.saturating_sub(1)];
        let p1 = src[i0];
        let p2 = src[(i0 + 1).min(last)];
        let p3 = src[(i0 + 2).min(last)];
        let v = 0.5
            * (2.0 * p1
                + (-p0 + p2) * t
                + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t * t
                + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t * t * t);
        *d = v.max(0.0);
    }
}

#[inline]
fn vmax(v: f32, m: f32) -> f32 {
    if v > m { v } else { m }
}

/// Largest value (-inf for an empty slice; a NaN never wins), in 8 independent lanes so the
/// loop vectorises instead of waiting on one `max` chain.
#[inline]
pub(crate) fn max_of(values: &[f32]) -> f32 {
    let mut lanes = [f32::NEG_INFINITY; 8];
    let (chunks, rest) = values.as_chunks::<8>();
    for chunk in chunks {
        for (lane, &v) in lanes.iter_mut().zip(chunk) {
            *lane = vmax(v, *lane);
        }
    }
    let mut m = lanes.iter().fold(f32::NEG_INFINITY, |m, &v| vmax(v, m));
    for &v in rest {
        m = vmax(v, m);
    }
    m
}

/// `sum(v^2)` in 8 lanes.
#[inline]
pub(crate) fn sum_of_squares(values: &[f32]) -> f32 {
    let mut lanes = [0.0f32; 8];
    let (chunks, rest) = values.as_chunks::<8>();
    for chunk in chunks {
        for (lane, &v) in lanes.iter_mut().zip(chunk) {
            *lane += v * v;
        }
    }
    lanes.iter().sum::<f32>() + rest.iter().map(|v| v * v).sum::<f32>()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn axis(scale: Scale, n_out: usize, blend: f64) -> Axis {
        Axis {
            scale,
            fmax: 24_000.0,
            n_out,
            warp_blend: blend,
            log_floor_hz: 20.0,
            interp: WarpInterp::Linear,
            aggregation: WarpAggregation::Peak,
        }
    }

    const SCALES: [Scale; 7] = [
        Scale::Log,
        Scale::Mel,
        Scale::Erb,
        Scale::Bark,
        Scale::Chroma,
        Scale::Linear,
        Scale::Melog,
    ];

    #[test]
    fn inverses_round_trip() {
        for hz in [0.5, 20.0, 440.0, 1000.0, 6500.0, 20_000.0] {
            assert!((htk_mel_to_hz(htk_hz_to_mel(hz)) - hz).abs() < 1e-6 * hz.max(1.0));
            assert!((erb_rate_to_hz(erb_rate_glasberg(hz)) - hz).abs() < 1e-6 * hz.max(1.0));
            assert!(
                (bark_to_hz(hz_to_bark(hz)) - hz).abs() < 1e-6 * hz.max(1.0),
                "{hz}"
            );
            assert!((chroma_to_hz(hz_to_chroma(hz)) - hz).abs() < 1e-6 * hz.max(1.0));
        }
        assert!((hz_to_chroma(440.0) - 69.0).abs() < 1e-12);
    }

    #[test]
    fn every_scale_is_monotonic_and_ends_at_fmax() {
        for scale in SCALES {
            for blend in [0.0, 0.5, 0.93, 1.0] {
                let w = Warp::new(axis(scale, 512, blend), 24_000.0, 16_385);
                let t = w.target_hz();
                assert_eq!(t[511], 24_000.0, "{scale:?}");
                assert!(t.windows(2).all(|p| p[1] >= p[0]), "{scale:?} {blend}");
            }
        }
        // Log starts at the floor, Chroma at 20 Hz, Linear at 0.
        let first = |s| Warp::new(axis(s, 64, 1.0), 24_000.0, 1025).target_hz()[0];
        assert!((first(Scale::Log) - 20.0).abs() < 1e-9);
        assert!((first(Scale::Chroma) - 20.0).abs() < 1e-9);
        assert_eq!(first(Scale::Linear), 0.0);
    }

    #[test]
    fn a_one_to_one_linear_grid_is_an_exact_copy() {
        let mut a = axis(Scale::Linear, 1025, 0.0);
        a.aggregation = WarpAggregation::Off;
        let w = Warp::new(a, 24_000.0, 1025);
        assert!(w.is_identity());
        let src: Vec<f32> = (0..1025).map(|i| (i as f32 * 0.37).sin().abs()).collect();
        let mut out = vec![0.0; 1025];
        w.apply(&src, &mut out);
        assert_eq!(out, src);
    }

    #[test]
    fn peak_aggregation_never_drops_a_narrow_partial() {
        // 64 log bins over 16,385 FFT bins: the top bins own hundreds of FFT bins each.
        let nlin = 16_385;
        let mut src = vec![0.0f32; nlin];
        src[12_345] = 1.0;
        let run = |agg| {
            let mut a = axis(Scale::Log, 64, 1.0);
            a.aggregation = agg;
            let w = Warp::new(a, 24_000.0, nlin);
            let mut out = vec![0.0; 64];
            w.apply(&src, &mut out);
            out.iter().fold(0.0f32, |m, &v| m.max(v))
        };
        assert_eq!(run(WarpAggregation::Peak), 1.0);
        let rms = run(WarpAggregation::Rms);
        assert!(rms > 0.0 && rms < 0.2, "{rms}");
        assert!(run(WarpAggregation::Off) < 1.0);
    }

    #[test]
    fn cubic_is_smooth_and_non_negative() {
        let nlin = 257;
        let src: Vec<f32> = (0..nlin)
            .map(|i| if i % 16 == 0 { 1.0 } else { 0.0 })
            .collect();
        let mut a = axis(Scale::Linear, 4096, 0.0);
        a.interp = WarpInterp::Cubic;
        let w = Warp::new(a, 24_000.0, nlin);
        let mut out = vec![0.0; 4096];
        w.apply(&src, &mut out);
        assert!(out.iter().all(|&v| v >= 0.0 && v.is_finite()));
        assert!(w.max_linear_index() < nlin);
    }
}
