//! DC blocker: one-pole high-pass `y[n] = x[n] - x[n-1] + r * y[n-1]`, run on the sample stream
//! before windowing so a DC offset never leaks into the low bars.

/// Corner frequency. Low enough to leave 20 Hz content alone.
pub const CUTOFF_HZ: f64 = 5.0;

#[derive(Debug, Clone)]
pub struct DcBlocker {
    r: f32,
    x1: f32,
    y1: f32,
}

impl DcBlocker {
    pub fn new(sample_rate: u32) -> Self {
        let r = (-std::f64::consts::TAU * CUTOFF_HZ / sample_rate as f64).exp() as f32;
        Self {
            r,
            x1: 0.0,
            y1: 0.0,
        }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let mut y = x - self.x1 + self.r * self.y1;
        // Flush denormals: a decaying tail must not slow the hot loop (pre-plan 5.3).
        if y.abs() < 1e-20 {
            y = 0.0;
        }
        self.x1 = x;
        self.y1 = y;
        y
    }

    pub fn reset(&mut self) {
        self.x1 = 0.0;
        self.y1 = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_input_is_attenuated_below_minus_60_db() {
        let mut dc = DcBlocker::new(48_000);
        let mut y = 0.0;
        for _ in 0..48_000 {
            y = dc.process(0.5);
        }
        assert!(20.0 * (y.abs() / 0.5).log10() < -60.0, "residual {y}");
    }

    #[test]
    fn a_1_khz_tone_passes_unchanged() {
        let mut dc = DcBlocker::new(48_000);
        let mut peak = 0.0f32;
        for n in 0..48_000 {
            let x = (std::f32::consts::TAU * 1000.0 * n as f32 / 48_000.0).sin();
            let y = dc.process(x);
            if n > 24_000 {
                peak = peak.max(y.abs());
            }
        }
        assert!((peak - 1.0).abs() < 0.01, "peak {peak}");
    }
}
