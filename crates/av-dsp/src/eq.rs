//! Pre-analysis shelf EQ (FFT CHOP "EQ" page). A high shelf and a low shelf (RBJ cookbook
//! biquads, transposed direct form II) with a wet/dry `amount`, run on **new** samples at
//! ingest, statefully and in time order: re-filtering the whole window each hop would restart
//! the IIR from a stale state.
//!
//! Provenance: the shelf design and the `x + amount * (eq(x) - x)` blend follow ZER0
//! `src-tauri/src/live_fft/dsp.rs:195-308` (MIT, same author), itself a port of the FFT CHOP.
//! Product default (2026-11-07): high shelf +24 dB at 1 kHz, Q 0.707, no low shelf.

/// EQ settings. A section whose gain is under 0.01 dB is bypassed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EqConfig {
    pub enabled: bool,
    pub high_shelf: bool,
    pub high_gain_db: f32,
    pub high_cutoff_hz: f32,
    pub low_shelf: bool,
    pub low_gain_db: f32,
    pub low_cutoff_hz: f32,
    pub q: f32,
    /// 0 = dry, 1 = fully filtered.
    pub amount: f32,
}

impl Default for EqConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            high_shelf: true,
            high_gain_db: 24.0,
            high_cutoff_hz: 1000.0,
            low_shelf: false,
            low_gain_db: 0.0,
            low_cutoff_hz: 200.0,
            q: 0.707,
            amount: 1.0,
        }
    }
}

impl EqConfig {
    /// No EQ at all.
    pub const OFF: Self = Self {
        enabled: false,
        high_shelf: false,
        high_gain_db: 0.0,
        high_cutoff_hz: 1000.0,
        low_shelf: false,
        low_gain_db: 0.0,
        low_cutoff_hz: 200.0,
        q: 0.707,
        amount: 0.0,
    };
}

#[derive(Debug, Clone, Copy, Default)]
struct Section {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
    active: bool,
}

impl Section {
    fn shelf(sample_rate: f64, high: bool, cutoff_hz: f64, gain_db: f64, q: f64) -> Self {
        if gain_db.abs() < 0.01 {
            return Self::default();
        }
        let cutoff = cutoff_hz.clamp(1.0, sample_rate * 0.49);
        let w0 = std::f64::consts::TAU * cutoff / sample_rate;
        let a = 10f64.powf(gain_db / 40.0);
        let alpha = w0.sin() / (2.0 * q.max(0.01));
        let (cos_w0, sqrt_a) = (w0.cos(), a.sqrt());
        let s = if high { 1.0 } else { -1.0 };
        let b0 = a * ((a + 1.0) + s * (a - 1.0) * cos_w0 + 2.0 * sqrt_a * alpha);
        let b1 = -s * 2.0 * a * ((a - 1.0) + s * (a + 1.0) * cos_w0);
        let b2 = a * ((a + 1.0) + s * (a - 1.0) * cos_w0 - 2.0 * sqrt_a * alpha);
        let a0 = (a + 1.0) - s * (a - 1.0) * cos_w0 + 2.0 * sqrt_a * alpha;
        let a1 = s * 2.0 * ((a - 1.0) - s * (a + 1.0) * cos_w0);
        let a2 = (a + 1.0) - s * (a - 1.0) * cos_w0 - 2.0 * sqrt_a * alpha;
        // FFT CHOP v2.13 (audit F10): a design that is not finite switches the shelf off.
        if ![b0, b1, b2, a0, a1, a2].iter().all(|v| v.is_finite()) || a0 == 0.0 {
            return Self::default();
        }
        Self {
            b0: (b0 / a0) as f32,
            b1: (b1 / a0) as f32,
            b2: (b2 / a0) as f32,
            a1: (a1 / a0) as f32,
            a2: (a2 / a0) as f32,
            z1: 0.0,
            z2: 0.0,
            active: true,
        }
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        if !self.active {
            return x;
        }
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        // Flush denormals in the state (long silences).
        if self.z1.abs() < 1e-25 {
            self.z1 = 0.0;
        }
        if self.z2.abs() < 1e-25 {
            self.z2 = 0.0;
        }
        y
    }
}

/// The shelf pair, built once per configuration.
#[derive(Debug, Clone)]
pub struct ShelfEq {
    high: Section,
    low: Section,
    amount: f32,
    active: bool,
}

impl ShelfEq {
    pub fn new(cfg: &EqConfig, sample_rate: u32) -> Self {
        let sr = sample_rate as f64;
        let q = cfg.q as f64;
        let high = if cfg.enabled && cfg.high_shelf {
            Section::shelf(
                sr,
                true,
                cfg.high_cutoff_hz as f64,
                cfg.high_gain_db as f64,
                q,
            )
        } else {
            Section::default()
        };
        let low = if cfg.enabled && cfg.low_shelf {
            Section::shelf(
                sr,
                false,
                cfg.low_cutoff_hz as f64,
                cfg.low_gain_db as f64,
                q,
            )
        } else {
            Section::default()
        };
        let active = (high.active || low.active) && cfg.amount > 0.0;
        Self {
            high,
            low,
            amount: cfg.amount,
            active,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    /// One sample, `x + amount * (eq(x) - x)`.
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        if !self.active {
            return x;
        }
        let y = self.low.process(self.high.process(x));
        x + self.amount * (y - x)
    }

    pub fn reset(&mut self) {
        for s in [&mut self.high, &mut self.low] {
            s.z1 = 0.0;
            s.z2 = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Steady-state gain of the EQ at `f` Hz, in dB.
    fn gain_at(eq: &EqConfig, f: f64) -> f64 {
        let sr = 48_000u32;
        let mut e = ShelfEq::new(eq, sr);
        let (mut peak, n) = (0.0f64, sr as usize);
        for i in 0..n {
            let x = (std::f64::consts::TAU * f * i as f64 / sr as f64).sin() as f32;
            let y = e.process(x) as f64;
            if i > n / 2 {
                peak = peak.max(y.abs());
            }
        }
        20.0 * peak.log10()
    }

    #[test]
    fn the_default_high_shelf_boosts_highs_by_24_db_and_leaves_lows() {
        let eq = EqConfig::default();
        assert!(gain_at(&eq, 50.0).abs() < 0.5, "lows untouched");
        assert!(
            (gain_at(&eq, 1000.0) - 12.0).abs() < 1.0,
            "half the boost at the corner"
        );
        assert!(
            (gain_at(&eq, 12_000.0) - 24.0).abs() < 0.5,
            "full boost well above"
        );
    }

    #[test]
    fn a_low_shelf_cuts_or_boosts_lows_only() {
        let eq = EqConfig {
            high_shelf: false,
            low_shelf: true,
            low_gain_db: -12.0,
            ..EqConfig::default()
        };
        let low = gain_at(&eq, 30.0);
        assert!((low + 12.0).abs() < 0.5, "low shelf at 30 Hz: {low:.2} dB");
        // 6 kHz: 8 samples per cycle, so the sampled peak is the true peak.
        assert!(gain_at(&eq, 6000.0).abs() < 0.5);
    }

    #[test]
    fn amount_blends_and_off_is_bypass() {
        let half = EqConfig {
            amount: 0.5,
            ..EqConfig::default()
        };
        let g = gain_at(&half, 12_000.0);
        // 1 + 0.5 * (15.85 - 1) = 8.42x = 18.5 dB
        assert!((g - 18.5).abs() < 0.5, "{g}");
        assert!(!ShelfEq::new(&EqConfig::OFF, 48_000).is_active());
        assert!(gain_at(&EqConfig::OFF, 12_000.0).abs() < 1e-6);
    }
}
