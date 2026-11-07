//! Analysis windows (FFT CHOP "Window Type"). Built once per configuration, never per hop.
//!
//! All six CHOP windows, in the periodic (DFT-even) form, the right one for spectral analysis.
//! The level calibration (`2 / sum(w)` per layer, or coherent gain) is applied by the layer, so
//! the window itself is unnormalized here. Conceptual source: the FFT CHOP `generateWindow`,
//! ported through ZER0 `live_fft/dsp.rs` (MIT).

/// Window shape.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum WindowKind {
    #[default]
    Hann,
    /// 0.54 / 0.46: a raised floor, -43 dB first sidelobe.
    Hamming,
    /// 3-term Blackman (-58 dB).
    Blackman,
    /// 4-term Blackman-Harris: -92 dB sidelobes, wider main lobe.
    BlackmanHarris,
    /// No taper.
    Rectangular,
    /// Kaiser with shape `beta` (the FFT CHOP defaults to 15: very low leakage, wide lobe).
    Kaiser { beta: f32 },
}

impl WindowKind {
    /// The window for a persisted shape, with the Kaiser beta already resolved.
    pub fn from_shape(shape: av_protocol::config::WindowShape, beta: f32) -> Self {
        use av_protocol::config::WindowShape as S;
        match shape {
            S::Kaiser => Self::Kaiser { beta },
            S::Hann => Self::Hann,
            S::Hamming => Self::Hamming,
            S::Blackman => Self::Blackman,
            S::BlackmanHarris => Self::BlackmanHarris,
            S::Rectangular => Self::Rectangular,
        }
    }

    /// Fills `out` with the periodic window.
    pub fn fill(self, out: &mut [f32]) {
        let n = out.len() as f64;
        let kaiser_norm = match self {
            Self::Kaiser { beta } => 1.0 / bessel_i0(beta as f64),
            _ => 0.0,
        };
        for (i, w) in out.iter_mut().enumerate() {
            let x = std::f64::consts::TAU * i as f64 / n;
            *w = match self {
                Self::Hann => 0.5 - 0.5 * x.cos(),
                Self::Hamming => 0.54 - 0.46 * x.cos(),
                Self::Blackman => 0.42 - 0.5 * x.cos() + 0.08 * (2.0 * x).cos(),
                Self::BlackmanHarris => {
                    0.35875 - 0.48829 * x.cos() + 0.14128 * (2.0 * x).cos()
                        - 0.01168 * (3.0 * x).cos()
                }
                Self::Rectangular => 1.0,
                Self::Kaiser { beta } => {
                    // Periodic form: centre at n/2, so the window is symmetric over n + 1 points.
                    let r = 2.0 * i as f64 / n - 1.0;
                    bessel_i0(beta as f64 * (1.0 - r * r).max(0.0).sqrt()) * kaiser_norm
                }
            } as f32;
        }
    }
}

/// Kaiser beta whose highest sidelobe sits `sidelobe_db` below the main lobe (Kaiser & Schafer
/// 1980): 80 dB gives about 10.73, 114.3 dB about 15. FFT CHOP "Kaiser Beta Mode: Auto".
pub fn kaiser_beta_for_sidelobe_db(sidelobe_db: f64) -> f64 {
    let a = sidelobe_db;
    let beta = if a > 60.0 {
        0.12438 * (a + 6.3)
    } else if a > 13.26 {
        0.76609 * (a - 13.26).powf(0.4) + 0.09834 * (a - 13.26)
    } else {
        0.0
    };
    beta.clamp(0.0, 100.0)
}

/// Modified Bessel function of the first kind, order 0 (power series; converges for any x).
fn bessel_i0(x: f64) -> f64 {
    let q = x * x / 4.0;
    let (mut sum, mut term, mut k) = (1.0, 1.0, 1.0);
    while term > sum * 1e-17 {
        term *= q / (k * k);
        sum += term;
        k += 1.0;
    }
    sum
}

/// Sum of the window (coherent gain times length).
pub fn sum(w: &[f32]) -> f64 {
    w.iter().map(|&v| v as f64).sum()
}

/// Equivalent noise bandwidth in bins: `N * sum(w^2) / sum(w)^2` (1.5 for Hann).
pub fn enbw_bins(w: &[f32]) -> f64 {
    let s = sum(w);
    let s2: f64 = w.iter().map(|&v| (v as f64) * (v as f64)).sum();
    w.len() as f64 * s2 / (s * s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filled(kind: WindowKind, n: usize) -> Vec<f32> {
        let mut w = vec![0.0; n];
        kind.fill(&mut w);
        w
    }

    #[test]
    fn hann_coherent_gain_and_enbw() {
        let w = filled(WindowKind::Hann, 2048);
        assert!((sum(&w) / 2048.0 - 0.5).abs() < 1e-6);
        assert!((enbw_bins(&w) - 1.5).abs() < 1e-6);
        assert_eq!(w[0], 0.0);
    }

    /// Textbook coherent gains and ENBWs (periodic windows, Harris 1978).
    #[test]
    fn every_window_matches_its_published_gain_and_enbw() {
        let n = 4096;
        for (kind, gain, enbw) in [
            (WindowKind::Hamming, 0.54, 1.3628),
            (WindowKind::Blackman, 0.42, 1.7268),
            (WindowKind::BlackmanHarris, 0.35875, 2.0044),
            (WindowKind::Rectangular, 1.0, 1.0),
        ] {
            let w = filled(kind, n);
            assert!((sum(&w) / n as f64 - gain).abs() < 1e-6, "{kind:?} gain");
            assert!(
                (enbw_bins(&w) - enbw).abs() < 1e-3,
                "{kind:?} enbw {}",
                enbw_bins(&w)
            );
            // Periodic symmetry: w[i] == w[n - i].
            assert!((w[1] - w[n - 1]).abs() < 1e-6, "{kind:?}");
        }
    }

    #[test]
    fn kaiser_is_symmetric_peaks_at_one_and_matches_known_enbw() {
        let mut w = filled(WindowKind::Kaiser { beta: 15.0 }, 4096);
        assert!((w[2048] - 1.0).abs() < 1e-6);
        assert!((w[1] - w[4095]).abs() < 1e-6);
        // Kaiser ENBW grows with beta; beta = 15 is 2.2333 bins (independent f64 computation).
        let e = enbw_bins(&w);
        assert!((e - 2.2333).abs() < 1e-3, "ENBW {e}");
        // beta = 0 is rectangular.
        WindowKind::Kaiser { beta: 0.0 }.fill(&mut w);
        assert!(w.iter().all(|&v| (v - 1.0).abs() < 1e-6));
    }

    #[test]
    fn auto_beta_follows_kaiser_and_schafer() {
        assert!((kaiser_beta_for_sidelobe_db(80.0) - 10.7348).abs() < 1e-3);
        assert!((kaiser_beta_for_sidelobe_db(114.3) - 15.0).abs() < 0.01);
        assert_eq!(kaiser_beta_for_sidelobe_db(10.0), 0.0);
        let mid = kaiser_beta_for_sidelobe_db(40.0);
        assert!((mid - 5.4816).abs() < 1e-3, "{mid}");
    }

    #[test]
    fn bessel_reference_values() {
        assert!((bessel_i0(0.0) - 1.0).abs() < 1e-12);
        assert!((bessel_i0(1.0) - 1.266_065_877_752_008_4).abs() < 1e-12);
        assert!((bessel_i0(15.0) / 339_649.373_297_913_9 - 1.0).abs() < 1e-10);
    }
}
