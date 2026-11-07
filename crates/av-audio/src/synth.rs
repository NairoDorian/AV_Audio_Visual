//! A deterministic demo track: something to look at before live capture exists, and a
//! repeatable signal for manual checks. 8 seconds at 120 BPM, stereo, looped by the file source:
//! a pitch-dropping kick on every beat, noise hats on eighths, a log sweep 150 Hz -> 6 kHz, a
//! two-note bass line and a quiet pink-ish bed. Generated once; nothing here runs per frame.

use crate::capture::CaptureFormat;
use crate::file::DecodedAudio;

pub const DEMO_SECONDS: f32 = 8.0;

pub fn demo_track(sample_rate: u32) -> DecodedAudio {
    let sr = sample_rate as f32;
    let frames = (sr * DEMO_SECONDS) as usize;
    let beat = sr * 0.5; // 120 BPM
    let mut samples = Vec::with_capacity(frames * 2);
    let mut rng = 0x2545_F491_4F6C_DD1Du64;
    let mut pink = [0.0f32; 3];
    let mut kick_phase = 0.0f32;
    let mut bass_phase = 0.0f32;
    let mut sweep_phase = 0.0f32;
    let tau = std::f32::consts::TAU;

    for n in 0..frames {
        let t = n as f32 / sr;
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        let white = (rng >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0;

        // Kick: 120 -> 45 Hz pitch drop, 180 ms decay.
        let since_beat = (n as f32 % beat) / sr;
        let kick_freq = 45.0 + 75.0 * (-since_beat * 30.0).exp();
        kick_phase = (kick_phase + kick_freq / sr).fract();
        let kick = (tau * kick_phase).sin() * (-since_beat * 12.0).exp() * 0.9;

        // Hats on eighths: 40 ms of high-passed noise.
        let since_eighth = (n as f32 % (beat / 2.0)) / sr;
        let hat = (white - pink[2]) * (-since_eighth * 90.0).exp() * 0.25;

        // Bass: alternates 55 / 82.4 Hz every two beats.
        let bass_freq = if ((t / 1.0) as u32).is_multiple_of(2) {
            55.0
        } else {
            82.41
        };
        bass_phase = (bass_phase + bass_freq / sr).fract();
        let bass = (tau * bass_phase).sin() * 0.25;

        // Lead: log sweep across the whole loop.
        let sweep_freq = 150.0 * (6000.0f32 / 150.0).powf(t / DEMO_SECONDS);
        sweep_phase = (sweep_phase + sweep_freq / sr).fract();
        let lead = (tau * sweep_phase).sin() * 0.15;

        // Bed: crude pink-ish noise (three one-pole stages).
        pink[0] = 0.997 * pink[0] + 0.029 * white;
        pink[1] = 0.985 * pink[1] + 0.032 * white;
        pink[2] = 0.950 * pink[2] + 0.048 * white;
        let bed = (pink[0] + pink[1] + pink[2]) * 0.2;

        let mono = kick + bass + bed;
        let pan = 0.5 + 0.4 * (tau * 0.125 * t).sin();
        samples.push(((mono + hat * 0.7 + lead * (1.0 - pan)) * 0.8).clamp(-1.0, 1.0));
        samples.push(((mono + hat * 1.3 + lead * pan) * 0.8).clamp(-1.0, 1.0));
    }
    DecodedAudio {
        format: CaptureFormat {
            sample_rate,
            channels: 2,
        },
        samples,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_demo_is_deterministic_stereo_and_in_range() {
        let a = demo_track(48_000);
        let b = demo_track(48_000);
        assert_eq!(a, b);
        assert_eq!(a.format.channels, 2);
        assert_eq!(a.frames(), 384_000);
        assert!(a.samples.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        let rms = (a.samples.iter().map(|s| s * s).sum::<f32>() / a.samples.len() as f32).sqrt();
        assert!(rms > 0.05, "demo is too quiet: {rms}");
    }
}
