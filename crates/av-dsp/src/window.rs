//! Analysis windows. Built once per configuration, never per hop.

/// Window shape.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum WindowKind {
    #[default]
    Hann,
    /// 4-term Blackman-Harris: lower leakage, wider main lobe.
    BlackmanHarris,
    /// Kaiser with shape `beta` (the FFT CHOP defaults to 15: very low leakage, wide lobe).
    Kaiser { beta: f32 },
}

impl WindowKind {
    /// Fills `out` with the periodic (DFT-even) window, the right choice for spectral analysis.
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
                Self::BlackmanHarris => {
                    0.35875 - 0.48829 * x.cos() + 0.14128 * (2.0 * x).cos()
                        - 0.01168 * (3.0 * x).cos()
                }
                Self::Kaiser { beta } => {
                    // Periodic form: centre at n/2, so the window is symmetric over n + 1 points.
                    let r = 2.0 * i as f64 / n - 1.0;
                    bessel_i0(beta as f64 * (1.0 - r * r).max(0.0).sqrt()) * kaiser_norm
                }
            } as f32;
        }
    }
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

    #[test]
    fn hann_coherent_gain_and_enbw() {
        let mut w = vec![0.0; 2048];
        WindowKind::Hann.fill(&mut w);
        assert!((sum(&w) / 2048.0 - 0.5).abs() < 1e-6);
        assert!((enbw_bins(&w) - 1.5).abs() < 1e-6);
        assert_eq!(w[0], 0.0);
    }

    #[test]
    fn blackman_harris_enbw() {
        let mut w = vec![0.0; 4096];
        WindowKind::BlackmanHarris.fill(&mut w);
        assert!((enbw_bins(&w) - 2.0044).abs() < 1e-3);
    }

    #[test]
    fn kaiser_is_symmetric_peaks_at_one_and_matches_known_enbw() {
        let mut w = vec![0.0; 4096];
        WindowKind::Kaiser { beta: 15.0 }.fill(&mut w);
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
    fn bessel_reference_values() {
        assert!((bessel_i0(0.0) - 1.0).abs() < 1e-12);
        assert!((bessel_i0(1.0) - 1.266_065_877_752_008_4).abs() < 1e-12);
        assert!((bessel_i0(15.0) / 339_649.373_297_913_9 - 1.0).abs() < 1e-10);
    }
}
