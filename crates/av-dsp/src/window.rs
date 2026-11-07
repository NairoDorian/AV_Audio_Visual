//! Analysis windows. Built once per configuration, never per hop.

/// Window shape.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WindowKind {
    #[default]
    Hann,
    /// 4-term Blackman-Harris: lower leakage, wider main lobe.
    BlackmanHarris,
}

impl WindowKind {
    /// Fills `out` with the periodic (DFT-even) window, the right choice for spectral analysis.
    pub fn fill(self, out: &mut [f32]) {
        let n = out.len() as f64;
        for (i, w) in out.iter_mut().enumerate() {
            let x = std::f64::consts::TAU * i as f64 / n;
            *w = match self {
                Self::Hann => 0.5 - 0.5 * x.cos(),
                Self::BlackmanHarris => {
                    0.35875 - 0.48829 * x.cos() + 0.14128 * (2.0 * x).cos()
                        - 0.01168 * (3.0 * x).cos()
                }
            } as f32;
        }
    }
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
}
