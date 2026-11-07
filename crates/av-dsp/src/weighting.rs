//! Equal-loudness weighting (FFT CHOP "Loudness Weighting"): a linear gain per output bin,
//! computed once per configuration at the bin's frequency and multiplied in after the warp.
//!
//! A and C are the IEC 61672-1 analogue responses, ITU-R 468 the standard's polynomial pair,
//! each normalized to unity at 1 kHz. Ported from ZER0 `live_fft/dsp.rs` section 5 (MIT).

use av_protocol::config::Weighting;

/// ITU-R 468 response before the +18.2 dB offset that puts 1 kHz at 0 dB.
fn itu468_response(f: f64) -> f64 {
    let f2 = f * f;
    let f3 = f2 * f;
    let f4 = f2 * f2;
    let f5 = f4 * f;
    let f6 = f3 * f3;
    let h1 =
        -4.737338981378384e-24 * f6 + 2.043828333266122e-15 * f4 - 1.363894795463638e-7 * f2 + 1.0;
    let h2 = 1.306612257412824e-19 * f5 - 2.118150887518656e-11 * f3 + 5.559488023498642e-4 * f;
    (1.246332637532143e-4 * f) / (h1 * h1 + h2 * h2).max(1e-12).sqrt()
}

const K2: f64 = 12194.0 * 12194.0;

fn a_response(f: f64) -> f64 {
    let f2 = f * f;
    (K2 * f2 * f2)
        / ((f2 + 20.6 * 20.6) * ((f2 + 107.7 * 107.7) * (f2 + 737.9 * 737.9)).sqrt() * (f2 + K2))
            .max(1e-12)
}

fn c_response(f: f64) -> f64 {
    let f2 = f * f;
    (K2 * f2) / ((f2 + 20.6 * 20.6) * (f2 + K2)).max(1e-12)
}

/// Linear gain of `weighting` at `hz` (1.0 at 1 kHz; 1.0 everywhere for `Off`).
pub fn gain(weighting: Weighting, hz: f64) -> f64 {
    let f = hz.max(1e-5);
    match weighting {
        Weighting::Off => 1.0,
        Weighting::A => a_response(f) / a_response(1000.0),
        Weighting::C => c_response(f) / c_response(1000.0),
        Weighting::Itu468 => itu468_response(f) / itu468_response(1000.0),
    }
}

/// The per-bin curve for `freqs_hz`.
pub fn curve(weighting: Weighting, freqs_hz: &[f64]) -> Vec<f32> {
    freqs_hz
        .iter()
        .map(|&f| gain(weighting, f) as f32)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db(w: Weighting, f: f64) -> f64 {
        20.0 * gain(w, f).log10()
    }

    /// IEC 61672-1 table values and the ITU-R 468 peak.
    #[test]
    fn curves_match_the_standards() {
        for w in [Weighting::A, Weighting::C, Weighting::Itu468] {
            assert!(db(w, 1000.0).abs() < 1e-9, "{w:?}");
        }
        assert!((db(Weighting::A, 100.0) - -19.1).abs() < 0.1);
        assert!((db(Weighting::A, 10_000.0) - -2.5).abs() < 0.1);
        assert!((db(Weighting::C, 31.5) - -3.0).abs() < 0.1);
        assert!((db(Weighting::Itu468, 6300.0) - 12.2).abs() < 0.1);
        assert!((db(Weighting::Itu468, 100.0) - -19.8).abs() < 0.2);
        assert_eq!(gain(Weighting::Off, 50.0), 1.0);
    }
}
