//! Time-based attack/release smoothing and peak hold (README 3.1 rule 4). The same preset
//! behaves identically at any analysis or display rate because every coefficient comes from
//! `exp(-dt / tau)` with a clamped `dt`, never from a per-frame constant.

/// Longest step accepted; a stall longer than this is treated as this long.
pub const MAX_DT: f32 = 0.25;

/// `1 - exp(-dt / tau)`; a zero time constant means "follow instantly".
#[inline]
pub fn coefficient(dt: f32, time_ms: f32) -> f32 {
    if time_ms <= 0.0 {
        return 1.0;
    }
    1.0 - (-dt.clamp(0.0, MAX_DT) * 1000.0 / time_ms).exp()
}

/// Per-bar exponential attack/release.
#[derive(Debug, Clone, Copy)]
pub struct Ballistics {
    pub attack_ms: f32,
    pub release_ms: f32,
}

impl Ballistics {
    /// Moves every `state` value toward its `target`.
    pub fn apply(&self, state: &mut [f32], target: &[f32], dt: f32) {
        let up = coefficient(dt, self.attack_ms);
        let down = coefficient(dt, self.release_ms);
        for (s, &t) in state.iter_mut().zip(target) {
            let k = if t > *s { up } else { down };
            *s += (t - *s) * k;
            if s.abs() < 1e-12 {
                *s = 0.0;
            }
        }
    }
}

/// Peak markers: hold for `hold_ms`, then fall at `fall_per_s` (normalized units per second).
#[derive(Debug, Clone, Copy)]
pub struct PeakHold {
    pub hold_ms: f32,
    pub fall_per_s: f32,
}

impl PeakHold {
    pub fn apply(&self, peaks: &mut [f32], held_s: &mut [f32], values: &[f32], dt: f32) {
        let dt = dt.clamp(0.0, MAX_DT);
        for ((p, h), &v) in peaks.iter_mut().zip(held_s.iter_mut()).zip(values) {
            if v >= *p {
                *p = v;
                *h = 0.0;
            } else {
                *h += dt;
                if *h * 1000.0 > self.hold_ms {
                    *p = (*p - self.fall_per_s * dt).max(v);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pre-plan 6: a step sampled at 30/60/120 fps reaches the same value at the same time.
    #[test]
    fn step_response_is_frame_rate_independent() {
        let b = Ballistics {
            attack_ms: 40.0,
            release_ms: 300.0,
        };
        let attack = |fps: f32| {
            let mut s = [0.0f32];
            for _ in 0..(0.5 * fps).round() as usize {
                b.apply(&mut s, &[1.0], 1.0 / fps);
            }
            s[0]
        };
        let (a, c, d) = (attack(30.0), attack(60.0), attack(120.0));
        assert!((a - c).abs() < 1e-3 && (c - d).abs() < 1e-3, "{a} {c} {d}");
        let release = |fps: f32| {
            let mut s = [1.0f32];
            // 0.5 s is a whole number of frames at 30, 60, 120 and 144 fps.
            for _ in 0..(0.5 * fps).round() as usize {
                b.apply(&mut s, &[0.0], 1.0 / fps);
            }
            s[0]
        };
        assert!((release(30.0) - release(144.0)).abs() < 1e-3);
    }

    #[test]
    fn time_constants_mean_what_they_say() {
        // After one time constant an exponential covers 1 - 1/e of the step.
        let b = Ballistics {
            attack_ms: 100.0,
            release_ms: 100.0,
        };
        let mut s = [0.0f32];
        for _ in 0..100 {
            b.apply(&mut s, &[1.0], 0.001);
        }
        assert!((s[0] - (1.0 - (-1.0f32).exp())).abs() < 1e-3);
    }

    #[test]
    fn zero_attack_is_instant_and_dt_is_clamped() {
        assert_eq!(coefficient(0.01, 0.0), 1.0);
        assert_eq!(coefficient(10.0, 1000.0), coefficient(MAX_DT, 1000.0));
        assert_eq!(coefficient(-1.0, 50.0), 0.0);
    }

    #[test]
    fn peaks_hold_then_fall() {
        let ph = PeakHold {
            hold_ms: 100.0,
            fall_per_s: 1.0,
        };
        let (mut p, mut h) = ([0.0f32], [0.0f32]);
        ph.apply(&mut p, &mut h, &[0.8], 0.01);
        for _ in 0..9 {
            ph.apply(&mut p, &mut h, &[0.0], 0.01);
        }
        assert_eq!(p[0], 0.8, "still holding at 90 ms");
        for _ in 0..21 {
            ph.apply(&mut p, &mut h, &[0.0], 0.01);
        }
        assert!(p[0] < 0.8 && p[0] > 0.55, "{}", p[0]);
    }
}
