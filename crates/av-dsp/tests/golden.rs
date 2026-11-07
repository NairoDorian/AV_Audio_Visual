//! Numeric acceptance gates (pre-plan section 6) for the analyzer.

use av_dsp::{Analyzer, LevelRef, PlanCache, SpectrumConfig};
use av_protocol::config::BandLayout;

const SR: u32 = 48_000;

fn analyzer(cfg: SpectrumConfig) -> Analyzer {
    let mut plans = PlanCache::default();
    let plan = plans.get(cfg.fft_size);
    Analyzer::new(cfg, plan).expect("valid config")
}

/// Instant ballistics, no auto-gain: bars equal the per-hop measurement.
fn raw(cfg: SpectrumConfig) -> SpectrumConfig {
    SpectrumConfig {
        attack_ms: 0.0,
        release_ms: 0.0,
        auto_gain: false,
        db_floor: -120.0,
        db_ceiling: 12.0,
        ..cfg
    }
}

/// Runs `seconds` of `signal(n)` through the analyzer and returns the last frame's dB values.
fn run(a: &mut Analyzer, seconds: f32, mut signal: impl FnMut(usize) -> f32) -> (Vec<f32>, bool) {
    let hop = a.config().hop_samples;
    let dt = hop as f32 / SR as f32;
    let hops = (seconds * SR as f32 / hop as f32).ceil() as usize;
    let mut buf = vec![0.0; hop];
    let mut n = 0;
    let mut last = (Vec::new(), false);
    for _ in 0..hops {
        for x in buf.iter_mut() {
            *x = signal(n);
            n += 1;
        }
        let f = a.process(&buf, dt);
        assert!(
            f.bars.iter().chain(f.bar_db).all(|v| v.is_finite()),
            "non-finite output"
        );
        last = (f.bar_db.to_vec(), f.silent);
    }
    last
}

fn sine(freq: f64, amp: f64) -> impl FnMut(usize) -> f32 {
    move |n| (amp * (std::f64::consts::TAU * freq * n as f64 / SR as f64).sin()) as f32
}

/// Index of the bar whose edges contain `f`.
fn bar_of(a: &Analyzer, f: f32) -> usize {
    let c = a.band_centers();
    (0..c.len())
        .min_by(|&i, &j| {
            (c[i].ln() - f.ln())
                .abs()
                .total_cmp(&(c[j].ln() - f.ln()).abs())
        })
        .unwrap()
}

#[test]
fn sine_peak_lands_in_the_right_bar() {
    let mut a = analyzer(raw(SpectrumConfig::default()));
    let (db, silent) = run(&mut a, 0.5, sine(1000.0, 0.5));
    assert!(!silent);
    let loudest = (0..db.len())
        .max_by(|&i, &j| db[i].total_cmp(&db[j]))
        .unwrap();
    assert!(
        (loudest as isize - bar_of(&a, 1000.0) as isize).abs() <= 1,
        "loudest bar {loudest}"
    );
}

#[test]
fn full_scale_sine_reads_0_db_with_band_reference() {
    // 8 wide log bars so the whole Hann main lobe falls inside one bar; bin-centred tone.
    let cfg = SpectrumConfig {
        bar_count: 8,
        level_ref: LevelRef::Band,
        ..raw(SpectrumConfig::single(2048, 2048, 512))
    };
    let mut a = analyzer(cfg);
    let f = 43.0 * SR as f64 / 2048.0; // bin 43 = 1007.8 Hz
    let (db, _) = run(&mut a, 0.5, sine(f, 1.0));
    let b = bar_of(&a, f as f32);
    assert!(db[b].abs() < 0.5, "full-scale sine read {} dB", db[b]);
}

#[test]
fn digital_silence_is_flagged_at_the_floor_without_nan() {
    let mut a = analyzer(SpectrumConfig::default());
    let hop = a.config().hop_samples;
    let zeros = vec![0.0; hop];
    for _ in 0..(10 * SR as usize / hop) {
        let f = a.process(&zeros, hop as f32 / SR as f32);
        assert!(f.silent);
        assert!(f.bars.iter().all(|&b| b == 0.0 && b.is_finite()));
    }
}

#[test]
fn silence_after_sound_decays_to_zero_instead_of_freezing() {
    let mut a = analyzer(SpectrumConfig::default());
    run(&mut a, 0.5, sine(440.0, 0.5));
    let hop = a.config().hop_samples;
    let zeros = vec![0.0; hop];
    let mut max = 1.0;
    for _ in 0..(3 * SR as usize / hop) {
        let f = a.process(&zeros, hop as f32 / SR as f32);
        max = f.bars.iter().copied().fold(0.0, f32::max);
    }
    assert!(max < 1e-3, "bars still at {max} after 3 s of silence");
}

#[test]
fn dc_offset_stays_out_of_the_bars() {
    let mut a = analyzer(raw(SpectrumConfig::default()));
    let (db, _) = run(&mut a, 2.0, |_| 0.5);
    let worst = db.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    assert!(worst < -60.0, "DC leaked: {worst} dB");
}

#[test]
fn near_nyquist_tone_appears_at_the_top_and_nowhere_low() {
    let cfg = SpectrumConfig {
        max_hz: 24_000.0,
        ..raw(SpectrumConfig::default())
    };
    let mut a = analyzer(cfg);
    let (db, _) = run(&mut a, 0.5, sine(23_000.0, 0.5));
    let top = db.len() - 1;
    let top_db = db[top - 1].max(db[top]);
    assert!(top_db > -20.0, "top bars read {top_db} dB");
    let low = bar_of(&a, 1000.0);
    assert!(
        db[..low].iter().all(|&d| d < top_db - 60.0),
        "aliasing into low bars"
    );
}

/// Paul Kellet's refined pink filter (public domain, accurate to +-0.05 dB above 9.2 Hz) over a
/// deterministic xorshift white source.
fn pink() -> impl FnMut(usize) -> f32 {
    let mut s = 0x9E37_79B9_7F4A_7C15u64;
    let mut b = [0.0f64; 7];
    move |_| {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        let white = (s >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0;
        b[0] = 0.99886 * b[0] + white * 0.0555179;
        b[1] = 0.99332 * b[1] + white * 0.0750759;
        b[2] = 0.96900 * b[2] + white * 0.1538520;
        b[3] = 0.86650 * b[3] + white * 0.3104856;
        b[4] = 0.55000 * b[4] + white * 0.5329522;
        b[5] = -0.7616 * b[5] - white * 0.0168980;
        let out = b[0] + b[1] + b[2] + b[3] + b[4] + b[5] + b[6] + white * 0.5362;
        b[6] = white * 0.115926;
        (out * 0.05) as f32
    }
}

#[test]
fn pink_noise_is_flat_within_1_5_db_from_100_hz_to_10_khz() {
    // Classic single window, ThirdOctave reference, 48 log bars.
    assert_pink_flat(raw(SpectrumConfig::single(2048, 2048, 512)), 1.5);
}

#[test]
fn pink_noise_is_flat_within_2_db_in_layered_mode() {
    assert_pink_flat(raw(SpectrumConfig::layered(4, 2)), 2.0);
}

/// Returns the worst deviation from the mean level, in dB, after asserting the tolerance.
fn assert_pink_flat(cfg: SpectrumConfig, tolerance_db: f64) -> f64 {
    let mut a = analyzer(cfg);
    let hop = a.config().hop_samples;
    let dt = hop as f32 / SR as f32;
    let mut source = pink();
    let mut buf = vec![0.0; hop];
    let mut sum = vec![0.0f64; a.config().bar_count];
    let mut n = 0usize;
    let hops = 4000;
    for i in 0..hops + 8 {
        for x in buf.iter_mut() {
            *x = source(n);
            n += 1;
        }
        let f = a.process(&buf, dt);
        if i >= 8 {
            for (s, &d) in sum.iter_mut().zip(f.bar_db) {
                *s += 10f64.powf(d as f64 / 10.0);
            }
        }
    }
    let centers = a.band_centers().to_vec();
    let levels: Vec<f64> = sum
        .iter()
        .map(|p| 10.0 * (p / hops as f64).log10())
        .collect();
    let in_range: Vec<(f32, f64)> = centers
        .iter()
        .zip(&levels)
        .filter(|(c, _)| **c >= 100.0 && **c <= 10_000.0)
        .map(|(c, l)| (*c, *l))
        .collect();
    let mean = in_range.iter().map(|(_, l)| l).sum::<f64>() / in_range.len() as f64;
    let mut worst = 0.0f64;
    for (c, l) in &in_range {
        worst = worst.max((l - mean).abs());
        assert!(
            (l - mean).abs() <= tolerance_db,
            "pink noise at {c:.0} Hz is {:.2} dB off",
            l - mean
        );
    }
    worst
}

#[test]
fn impulse_is_broadband_and_finite() {
    let mut a = analyzer(raw(SpectrumConfig::default()));
    let hop = a.config().hop_samples;
    let mut buf = vec![0.0; hop];
    buf[hop / 2] = 1.0;
    let f = a.process(&buf, hop as f32 / SR as f32);
    let db = f.bar_db.to_vec();
    let centers = a.band_centers().to_vec();
    for (c, d) in centers.iter().zip(&db) {
        assert!(d.is_finite());
        if (100.0..=10_000.0).contains(c) {
            assert!(*d > -100.0, "impulse missing at {c} Hz ({d} dB)");
        }
    }
}

#[test]
fn every_layout_and_preset_builds_and_runs() {
    use av_protocol::config::{AnalysisConfig, FftPreset};
    for preset in [
        FftPreset::Layered,
        FftPreset::LowLatency,
        FftPreset::Default,
        FftPreset::Precision,
        FftPreset::LowPower,
    ] {
        for layout in [BandLayout::Log, BandLayout::Linear, BandLayout::Mel] {
            let app = AnalysisConfig {
                preset,
                layout,
                bar_count: 128,
                ..AnalysisConfig::default()
            };
            let mut a = analyzer(SpectrumConfig::from_app(&app, SR));
            let (db, _) = run(&mut a, 0.2, sine(2000.0, 0.25));
            assert_eq!(db.len(), 128, "{preset:?} {layout:?}");
        }
    }
}

#[test]
fn clipping_is_flagged() {
    let mut a = analyzer(SpectrumConfig::default());
    let hop = a.config().hop_samples;
    let f = a.process(&vec![1.0; hop], hop as f32 / SR as f32);
    assert!(f.clipped);
    let f = a.process(&vec![0.5; hop], hop as f32 / SR as f32);
    assert!(!f.clipped);
}

#[test]
fn auto_gain_lifts_quiet_material() {
    let quiet = sine(1000.0, 0.01); // -40 dBFS
    let mut fixed = analyzer(SpectrumConfig {
        auto_gain: false,
        ..SpectrumConfig::default()
    });
    let mut auto = analyzer(SpectrumConfig::default());
    run(&mut fixed, 1.0, quiet);
    run(&mut auto, 1.0, sine(1000.0, 0.01));
    let hop = auto.config().hop_samples;
    let b = bar_of(&auto, 1000.0);
    let tone: Vec<f32> = (0..hop)
        .map(|n| (0.01 * (std::f64::consts::TAU * 1000.0 * n as f64 / SR as f64).sin()) as f32)
        .collect();
    let lf = fixed.process(&tone, hop as f32 / SR as f32).bars[b];
    let la = auto.process(&tone, hop as f32 / SR as f32).bars[b];
    assert!(la > lf + 0.2, "auto-gain {la} vs fixed {lf}");
    assert!(la <= 1.0);
}

#[test]
fn invalid_configs_are_rejected() {
    let mut plans = PlanCache::default();
    let bad = SpectrumConfig::single(2048, 2047, 512);
    assert!(Analyzer::new(bad, plans.get(2047)).is_err());
    let unpadded_too_small = SpectrumConfig::single(2048, 1024, 512);
    assert!(Analyzer::new(unpadded_too_small, plans.get(1024)).is_err());
    assert!(SpectrumConfig::layered(7, 1).validate().is_err());
    assert!(
        SpectrumConfig::layered(6, 8).validate().is_err(),
        "above the FFT size limit"
    );
    let mismatch = SpectrumConfig::default();
    assert!(Analyzer::new(mismatch, plans.get(1024)).is_err());
    let no_bars = SpectrumConfig {
        bar_count: 0,
        ..SpectrumConfig::default()
    };
    assert!(Analyzer::new(no_bars, plans.get(2048)).is_err());
}
