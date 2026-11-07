//! Attack/release ballistics (FFT CHOP "Loudness & Ballistics"). Two ways to say the same
//! envelope: a per-hop coefficient, or a time constant converted with the real hop duration
//! (`exp(-dt / tau)`), so the Milliseconds mode behaves identically at any hop size.
//!
//! `state += (1 - c) * (target - state)` with `c` the attack coefficient when rising and the
//! release coefficient when falling; 0 follows instantly, 0.99 is the CHOP's slowest (1 would
//! freeze the state forever). Ported from ZER0 `live_fft/dsp.rs` section 7 (MIT).

/// Largest per-hop coefficient.
pub const MAX_COEF: f32 = 0.999;
/// Below this an envelope is flushed to zero (a release towards silence ends in denormals).
const FLUSH: f32 = 1e-20;

/// Time constant (ms) to a per-hop coefficient for a hop of `dt_ms`; tau = time to 63 %.
pub fn coef_from_ms(time_ms: f64, dt_ms: f64) -> f32 {
    if time_ms <= 0.0 || dt_ms <= 0.0 {
        return 0.0;
    }
    (-dt_ms / time_ms).exp().clamp(0.0, MAX_COEF as f64) as f32
}

/// Moves `state` toward `values` and writes the new envelope back into `values`.
pub fn apply(attack: f32, release: f32, values: &mut [f32], state: &mut [f32]) {
    let up = 1.0 - attack.clamp(0.0, MAX_COEF);
    let down = 1.0 - release.clamp(0.0, MAX_COEF);
    for (v, s) in values.iter_mut().zip(state.iter_mut()) {
        let diff = *v - *s;
        let mut next = *s + if diff > 0.0 { up } else { down } * diff;
        if next.abs() < FLUSH {
            next = 0.0;
        }
        *s = next;
        *v = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_time_constant_reaches_63_percent_after_tau_at_any_hop() {
        for hop_ms in [1.0, 5.333, 10.667, 21.333] {
            let k = coef_from_ms(100.0, hop_ms);
            let (mut v, mut s) = ([0.0f32], [0.0f32]);
            let steps = (100.0 / hop_ms).round() as usize;
            for _ in 0..steps {
                v[0] = 1.0;
                apply(k, k, &mut v, &mut s);
            }
            let expect = 1.0 - (-(steps as f64 * hop_ms) / 100.0).exp();
            assert!((s[0] as f64 - expect).abs() < 1e-4, "{hop_ms}: {}", s[0]);
        }
    }

    #[test]
    fn zero_coefficients_follow_instantly_and_release_is_asymmetric() {
        let (mut v, mut s) = ([0.8f32, 0.1], [0.0f32, 0.5]);
        apply(0.0, 0.9, &mut v, &mut s);
        assert_eq!(v[0], 0.8);
        assert!((v[1] - 0.46).abs() < 1e-6);
        assert_eq!(coef_from_ms(0.0, 10.0), 0.0);
    }

    #[test]
    fn decays_flush_to_exact_zero() {
        let (mut v, mut s) = ([0.0f32], [1e-21f32]);
        apply(0.5, 0.5, &mut v, &mut s);
        assert_eq!(s[0], 0.0);
    }
}
