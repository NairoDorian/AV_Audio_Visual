//! Per-bar band edges and the sparse bin-to-bar weight matrix (README 3.1 rule 3: per-bar
//! measurement, never interpolation of seven bands).
//!
//! Units. `P[k]` is the calibrated power of FFT bin `k`: a sine of amplitude `A` centred on bin
//! `k` gives `P[k] = A^2 / 2`. Its power spectral density is `P[k] / (ENBW * df)`. A bar's power
//! is its average PSD over `[lo, hi]` times a reference bandwidth:
//!
//! - `LevelRef::Band`: the bar's own width, so a sine inside one bar reads its true power
//!   (0 dB for a full-scale sine).
//! - `LevelRef::ThirdOctave` (default, pre-plan 5.3): the third-octave width at the bar's centre,
//!   so pink noise reads flat and bar height does not depend on the bar count.
//!
//! Bars wider than two bins integrate the PSD with fractional overlap at the edges. Narrower
//! bars (the low end, where a 2048 FFT has 23 Hz bins) interpolate the PSD at the centre
//! frequency, so neighbouring low bars do not share one blocky bin value.

use av_protocol::config::BandLayout;

use crate::weighting::Weighting;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LevelRef {
    Band,
    #[default]
    ThirdOctave,
}

/// Third-octave bandwidth in Hz at centre `fc`.
pub fn third_octave_width(fc: f64) -> f64 {
    fc * (2f64.powf(1.0 / 6.0) - 2f64.powf(-1.0 / 6.0))
}

fn mel(f: f64) -> f64 {
    2595.0 * (1.0 + f / 700.0).log10()
}

fn mel_inv(m: f64) -> f64 {
    700.0 * (10f64.powf(m / 2595.0) - 1.0)
}

/// `bar_count + 1` edges in Hz from `min_hz` to `max_hz`.
pub fn edges(layout: BandLayout, bar_count: usize, min_hz: f64, max_hz: f64) -> Vec<f64> {
    (0..=bar_count)
        .map(|i| {
            let t = i as f64 / bar_count as f64;
            match layout {
                BandLayout::Log => min_hz * (max_hz / min_hz).powf(t),
                BandLayout::Linear => min_hz + (max_hz - min_hz) * t,
                BandLayout::Mel => mel_inv(mel(min_hz) + (mel(max_hz) - mel(min_hz)) * t),
            }
        })
        .collect()
}

/// Everything needed to turn a bin power spectrum into bar powers. Built once per config.
#[derive(Debug, Clone)]
pub struct BandMap {
    /// Row offsets into `bins`/`weights`; length `bar_count + 1`.
    starts: Vec<u32>,
    bins: Vec<u32>,
    weights: Vec<f32>,
    /// Geometric centre of each bar in Hz.
    pub centers: Vec<f32>,
    /// Bar edges in Hz, `bar_count + 1`.
    pub edges: Vec<f32>,
    /// Weighting gain per bar in dB.
    pub weighting_db: Vec<f32>,
}

/// Inputs to [`BandMap::build`].
#[derive(Debug, Clone, Copy)]
pub struct BandSpec {
    pub layout: BandLayout,
    pub bar_count: usize,
    pub min_hz: f64,
    pub max_hz: f64,
    pub sample_rate: u32,
    pub fft_size: usize,
    pub enbw_bins: f64,
    pub level_ref: LevelRef,
    pub weighting: Weighting,
}

impl BandMap {
    pub fn build(spec: BandSpec) -> Self {
        let df = spec.sample_rate as f64 / spec.fft_size as f64;
        let n_bins = spec.fft_size / 2 + 1;
        let nyquist = spec.sample_rate as f64 / 2.0;
        let max_hz = spec.max_hz.min(nyquist);
        let min_hz = spec.min_hz.clamp(1.0, max_hz / 2.0);
        let e = edges(spec.layout, spec.bar_count, min_hz, max_hz);

        let mut starts = Vec::with_capacity(spec.bar_count + 1);
        let mut bins = Vec::new();
        let mut weights = Vec::new();
        let mut centers = Vec::with_capacity(spec.bar_count);
        let psd_scale = 1.0 / (spec.enbw_bins * df);

        for b in 0..spec.bar_count {
            starts.push(bins.len() as u32);
            let (lo, hi) = (e[b], e[b + 1]);
            let bw = hi - lo;
            let fc = (lo * hi).sqrt();
            centers.push(fc as f32);
            let bw_ref = match spec.level_ref {
                LevelRef::Band => bw,
                LevelRef::ThirdOctave => third_octave_width(fc),
            };
            if bw < 2.0 * df {
                // Interpolate the PSD at the centre frequency.
                let p = (fc / df).min((n_bins - 1) as f64);
                let k0 = p.floor() as usize;
                let t = p - k0 as f64;
                let k1 = (k0 + 1).min(n_bins - 1);
                let w = bw_ref * psd_scale;
                push(&mut bins, &mut weights, k0, (1.0 - t) * w);
                if k1 != k0 {
                    push(&mut bins, &mut weights, k1, t * w);
                }
            } else {
                // Integrate the PSD over [lo, hi]; bin k covers [(k - 0.5) df, (k + 0.5) df].
                let k_lo = ((lo / df) - 0.5).floor().max(0.0) as usize;
                let k_hi = (((hi / df) + 0.5).ceil() as usize).min(n_bins - 1);
                for k in k_lo..=k_hi {
                    let a = (k as f64 - 0.5) * df;
                    let z = (k as f64 + 0.5) * df;
                    let overlap = (hi.min(z) - lo.max(a)).max(0.0);
                    if overlap > 0.0 {
                        push(
                            &mut bins,
                            &mut weights,
                            k,
                            overlap / bw * bw_ref * psd_scale,
                        );
                    }
                }
            }
        }
        starts.push(bins.len() as u32);

        let weighting_db = centers
            .iter()
            .map(|&f| spec.weighting.db(f as f64) as f32)
            .collect();
        Self {
            starts,
            bins,
            weights,
            centers,
            edges: e.iter().map(|&v| v as f32).collect(),
            weighting_db,
        }
    }

    pub fn bar_count(&self) -> usize {
        self.centers.len()
    }

    /// Bar powers from bin powers. No allocation.
    pub fn apply(&self, bin_power: &[f32], out: &mut [f32]) {
        for (b, o) in out.iter_mut().enumerate().take(self.bar_count()) {
            let (s, e) = (self.starts[b] as usize, self.starts[b + 1] as usize);
            let mut acc = 0.0f32;
            for (&k, &w) in self.bins[s..e].iter().zip(&self.weights[s..e]) {
                acc += bin_power[k as usize] * w;
            }
            *o = acc;
        }
    }
}

fn push(bins: &mut Vec<u32>, weights: &mut Vec<f32>, k: usize, w: f64) {
    bins.push(k as u32);
    weights.push(w as f32);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(layout: BandLayout, bars: usize) -> BandSpec {
        BandSpec {
            layout,
            bar_count: bars,
            min_hz: 30.0,
            max_hz: 16_000.0,
            sample_rate: 48_000,
            fft_size: 2048,
            enbw_bins: 1.5,
            level_ref: LevelRef::Band,
            weighting: Weighting::Z,
        }
    }

    #[test]
    fn edges_are_monotonic_and_span_the_range() {
        for layout in [BandLayout::Log, BandLayout::Linear, BandLayout::Mel] {
            let e = edges(layout, 48, 30.0, 16_000.0);
            assert_eq!(e.len(), 49);
            assert!((e[0] - 30.0).abs() < 1e-9 && (e[48] - 16_000.0).abs() < 1e-6);
            assert!(e.windows(2).all(|w| w[1] > w[0]), "{layout:?}");
        }
    }

    #[test]
    fn log_edges_have_a_constant_ratio() {
        let e = edges(BandLayout::Log, 10, 100.0, 102_400.0);
        for w in e.windows(2) {
            assert!((w[1] / w[0] - 2.0).abs() < 1e-9);
        }
    }

    #[test]
    fn every_bar_reads_at_least_one_bin() {
        let m = BandMap::build(spec(BandLayout::Log, 128));
        for b in 0..m.bar_count() {
            assert!(m.starts[b + 1] > m.starts[b], "bar {b} is empty");
        }
    }

    #[test]
    fn max_hz_is_capped_at_nyquist() {
        let mut s = spec(BandLayout::Linear, 4);
        s.max_hz = 30_000.0;
        let m = BandMap::build(s);
        assert_eq!(*m.edges.last().unwrap(), 24_000.0);
    }

    #[test]
    fn flat_psd_with_third_octave_reference_reads_proportional_to_centre() {
        // White noise (flat PSD) has third-octave power proportional to fc.
        let mut s = spec(BandLayout::Log, 24);
        s.level_ref = LevelRef::ThirdOctave;
        let m = BandMap::build(s);
        let p = vec![1.0f32; 1025];
        let mut out = vec![0.0; 24];
        m.apply(&p, &mut out);
        for (b, (&o, &c)) in out.iter().zip(&m.centers).enumerate() {
            let expect = third_octave_width(c as f64) / (1.5 * 23.4375);
            assert!((o as f64 / expect - 1.0).abs() < 1e-3, "bar {b}");
        }
    }
}
