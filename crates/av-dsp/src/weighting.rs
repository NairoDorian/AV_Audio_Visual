//! Frequency weighting curves (IEC 61672-1), applied per bar as a precomputed dB offset at the
//! bar's centre frequency.

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Weighting {
    /// Flat.
    #[default]
    Z,
    A,
    C,
}

impl Weighting {
    /// Gain in dB at `f` Hz (0 dB at 1 kHz for A and C).
    pub fn db(self, f: f64) -> f64 {
        const F1: f64 = 20.598_997;
        const F2: f64 = 107.652_65;
        const F3: f64 = 737.862_23;
        const F4: f64 = 12_194.217;
        let f2 = f * f;
        match self {
            Self::Z => 0.0,
            Self::A => {
                let num = F4 * F4 * f2 * f2;
                let den =
                    (f2 + F1 * F1) * ((f2 + F2 * F2) * (f2 + F3 * F3)).sqrt() * (f2 + F4 * F4);
                20.0 * (num / den).log10() + 2.000
            }
            Self::C => {
                let num = F4 * F4 * f2;
                let den = (f2 + F1 * F1) * (f2 + F4 * F4);
                20.0 * (num / den).log10() + 0.062
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_points() {
        assert!(Weighting::A.db(1000.0).abs() < 0.01);
        assert!(Weighting::C.db(1000.0).abs() < 0.01);
        assert!((Weighting::A.db(100.0) - -19.1).abs() < 0.1);
        assert!((Weighting::A.db(10_000.0) - -2.5).abs() < 0.1);
        assert!((Weighting::C.db(31.5) - -3.0).abs() < 0.1);
        assert_eq!(Weighting::Z.db(50.0), 0.0);
    }
}
