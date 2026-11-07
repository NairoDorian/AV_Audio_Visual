//! Golden behaviour of the CHOP-parity analyzer (pre-plan section 6 numeric gates): levels,
//! references, loudness modes, ballistics, axis, silence, and every menu combination.

use av_dsp::{Analyzer, PlanCache, SpectrumConfig};
use av_protocol::config::{
    AnalysisConfig, BallisticsMode, DbReference, KaiserBetaMode, LoudnessMode, MagnitudeNorm,
    OutputBinsMode, Scale, WarpAggregation, WarpInterp, Weighting, WindowLengthMode, WindowShape,
};

const SR: u32 = 48_000;

fn analyzer(cfg: SpectrumConfig) -> Analyzer {
    let mut plans = PlanCache::default();
    Analyzer::new(cfg.clone(), plans.get(cfg.fft_size)).expect("valid config")
}

/// Measurement settings: no EQ, no ballistics, dBFS reference, full-scale magnitudes, then
/// `edit`.
fn cfg(edit: impl FnOnce(&mut AnalysisConfig)) -> SpectrumConfig {
    let mut s = AnalysisConfig::default();
    s.eq.enabled = false;
    s.ballistics = false;
    s.db_reference = DbReference::Dbfs;
    s.magnitude_norm = MagnitudeNorm::FullScale;
    edit(&mut s);
    SpectrumConfig::new(s, SR)
}

/// Feeds `seconds` of `signal`; returns the last display frame and its silent flag.
fn run(a: &mut Analyzer, seconds: f32, mut signal: impl FnMut(usize) -> f32) -> (Vec<f32>, bool) {
    let hop = a.config().hop_samples;
    let dt = hop as f32 / SR as f32;
    let mut buf = vec![0.0f32; hop];
    let mut n = 0;
    let mut last = (Vec::new(), false);
    for _ in 0..(seconds * SR as f32 / hop as f32).ceil() as usize {
        for x in buf.iter_mut() {
            *x = signal(n);
            n += 1;
        }
        let f = a.process(&buf, dt);
        assert!(
            f.bars
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
        );
        last = (f.bars.to_vec(), f.silent);
    }
    last
}

fn sine(freq: f64, amp: f64) -> impl FnMut(usize) -> f32 {
    move |n| (amp * (std::f64::consts::TAU * freq * n as f64 / SR as f64).sin()) as f32
}

fn nearest(a: &Analyzer, hz: f64) -> usize {
    let t = a.target_hz();
    (0..t.len())
        .min_by(|&i, &j| (t[i] - hz).abs().total_cmp(&(t[j] - hz).abs()))
        .unwrap()
}

fn peak(v: &[f32]) -> (usize, f32) {
    v.iter()
        .copied()
        .enumerate()
        .fold((0, f32::MIN), |b, (i, x)| if x > b.1 { (i, x) } else { b })
}

#[test]
fn a_sine_peaks_at_its_frequency_on_every_scale() {
    for scale in [
        Scale::Log,
        Scale::Mel,
        Scale::Erb,
        Scale::Bark,
        Scale::Chroma,
        Scale::Linear,
        Scale::Melog,
    ] {
        let mut a = analyzer(cfg(|s| s.scale = scale));
        let (bars, _) = run(&mut a, 0.6, sine(1000.0, 0.5));
        let (i, _) = peak(&bars);
        let hz = a.target_hz()[i];
        let step = (a.target_hz()[i + 1] - a.target_hz()[i - 1]).abs();
        assert!(
            (hz - 1000.0).abs() <= step.max(3.0),
            "{scale:?}: peak at {hz:.1} Hz"
        );
    }
}

/// Raw RFFT bins, linear loudness, dBFS: a full-scale bin-centred sine reads exactly full
/// height, in both magnitude normalizations (their dBFS references differ by `window / 2`).
#[test]
fn a_full_scale_sine_reads_full_height_in_dbfs() {
    for norm in [MagnitudeNorm::FullScale, MagnitudeNorm::CoherentGain] {
        for loudness in [LoudnessMode::Off, LoudnessMode::Db] {
            let mut a = analyzer(cfg(|s| {
                s.raw_bins = true;
                s.layers = 1;
                s.magnitude_norm = norm;
                s.loudness = loudness;
            }));
            assert!(a.warp().is_identity());
            let f = 64.0 * SR as f64 / a.config().fft_size as f64;
            let (bars, _) = run(&mut a, 0.3, sine(f, 1.0));
            let (i, v) = peak(&bars);
            assert_eq!(i, 64);
            assert!((v - 1.0).abs() < 0.01, "{norm:?} {loudness:?}: {v}");
        }
    }
}

#[test]
fn db_display_spans_the_db_range() {
    // -40 dBFS on an 80 dB range is half height.
    let mut a = analyzer(cfg(|s| {
        s.raw_bins = true;
        s.layers = 1;
    }));
    let f = 64.0 * SR as f64 / a.config().fft_size as f64;
    let (bars, _) = run(&mut a, 0.3, sine(f, 0.01));
    assert!((bars[64] - 0.5).abs() < 0.01, "{}", bars[64]);
    // Linear loudness: amplitude 0.25 reads 0.25.
    let mut a = analyzer(cfg(|s| {
        s.raw_bins = true;
        s.layers = 1;
        s.loudness = LoudnessMode::Off;
    }));
    let (bars, _) = run(&mut a, 0.3, sine(f, 0.25));
    assert!((bars[64] - 0.25).abs() < 0.005, "{}", bars[64]);
}

#[test]
fn frame_peak_and_agc_lift_quiet_material_dbfs_does_not() {
    let level = |r: DbReference| {
        let mut a = analyzer(cfg(|s| s.db_reference = r));
        let (bars, _) = run(&mut a, 2.0, sine(1000.0, 0.001));
        peak(&bars).1
    };
    let dbfs = level(DbReference::Dbfs);
    assert!((0.15..0.26).contains(&dbfs), "dBFS {dbfs}");
    assert!((level(DbReference::FramePeak) - 1.0).abs() < 1e-4);
    assert!(level(DbReference::Agc) > 0.98);
}

#[test]
fn a_weighting_takes_19_db_off_100_hz() {
    let at = |w: Weighting| {
        let mut a = analyzer(cfg(|s| s.weighting = w));
        let (bars, _) = run(&mut a, 0.8, sine(100.0, 0.5));
        bars[nearest(&a, 100.0)] * 80.0
    };
    let drop = at(Weighting::Off) - at(Weighting::A);
    assert!((drop - 19.1).abs() < 0.5, "A at 100 Hz: -{drop:.2} dB");
}

#[test]
fn digital_silence_is_flagged_and_reads_zero() {
    let mut a = analyzer(cfg(|_| {}));
    let (bars, silent) = run(&mut a, 0.2, |_| 0.0);
    assert!(silent);
    assert!(bars.iter().all(|&v| v == 0.0));
}

#[test]
fn silence_after_sound_decays_instead_of_freezing() {
    let mut a = analyzer(cfg(|s| {
        s.ballistics = true;
        s.ballistics_mode = BallisticsMode::Milliseconds;
        s.release_ms = 150.0;
    }));
    let (loud, _) = run(&mut a, 0.5, sine(1000.0, 0.5));
    let i = peak(&loud).0;
    let (soon, _) = run(&mut a, 0.05, |_| 0.0);
    let (later, silent) = run(&mut a, 1.5, |_| 0.0);
    assert!(silent);
    assert!(
        soon[i] < loud[i] && soon[i] > later[i],
        "{} {} {}",
        loud[i],
        soon[i],
        later[i]
    );
    assert!(later[i] < 0.01);
}

#[test]
fn coefficient_ballistics_smooth_per_hop() {
    let mut a = analyzer(cfg(|s| {
        s.ballistics = true;
        s.ballistics_mode = BallisticsMode::Coefficient;
        s.attack = 0.9;
        s.release = 0.0;
    }));
    run(&mut a, 0.1, |_| 0.0);
    let hop = a.config().hop_samples;
    let mut tone = sine(1000.0, 1.0);
    let burst: Vec<f32> = (0..hop).map(&mut tone).collect();
    let mut target = analyzer(cfg(|_| {}));
    run(&mut target, 0.1, |_| 0.0);
    let i = nearest(&a, 1000.0);
    let full = target.process(&burst, 0.01).bars[i];
    let smoothed = a.process(&burst, 0.01).bars[i];
    assert!((smoothed - 0.1 * full).abs() < 1e-3, "{smoothed} vs {full}");
}

#[test]
fn dc_offset_stays_out_of_the_display() {
    let mut a = analyzer(cfg(|_| {}));
    let (bars, _) = run(&mut a, 1.0, |_| 0.5);
    assert!(peak(&bars).1 < 0.05, "{}", peak(&bars).1);
}

#[test]
fn a_tone_near_nyquist_appears_at_the_top() {
    let mut a = analyzer(cfg(|_| {}));
    let (bars, _) = run(&mut a, 0.5, sine(22_000.0, 0.5));
    let hz = a.target_hz()[peak(&bars).0];
    assert!(hz > 20_000.0, "{hz}");
}

#[test]
fn clipping_is_flagged() {
    let mut a = analyzer(cfg(|_| {}));
    let hop = a.config().hop_samples;
    assert!(a.process(&vec![1.2; hop], 0.01).clipped);
    assert!(!a.process(&vec![0.2; hop], 0.01).clipped);
}

#[test]
fn spectral_features_follow_the_signal() {
    let mut a = analyzer(cfg(|s| s.spectral_features = true));
    let hop = a.config().hop_samples;
    let mut tone = sine(2000.0, 0.5);
    let mut last = None;
    for _ in 0..40 {
        let buf: Vec<f32> = (0..hop).map(&mut tone).collect();
        last = a.process(&buf, 0.01).features;
    }
    let f = last.expect("features enabled");
    assert!((f.centroid_hz - 2000.0).abs() < 60.0, "{f:?}");
    assert!((f.rms_db - -9.03).abs() < 0.3, "{f:?}");
    assert!(f.flatness < 0.05);
}

#[test]
fn the_axis_settings_resolve_as_documented() {
    // Auto output bins = N/2 + 1; raw bins too, and the grid is the identity.
    let c = cfg(|s| s.output_bins_mode = OutputBinsMode::Auto);
    assert_eq!(c.output_bins(), c.fft_size / 2 + 1);
    let c = cfg(|s| s.output_bins = 100_000_000);
    assert_eq!(
        c.output_bins(),
        100_000_000,
        "the config pipeline clamps, not the resolver"
    );
    assert!(c.validate().is_err());
    // Milliseconds window at the source rate.
    let ms = |rate| {
        let s = AnalysisConfig {
            window_length_mode: WindowLengthMode::Milliseconds,
            window_ms: 21.333,
            ..AnalysisConfig::default()
        };
        SpectrumConfig::new(s, rate).window_samples
    };
    assert_eq!((ms(48_000), ms(44_100)), (1024, 941));
    // Without zero-padding the transform is the longest window.
    let c = cfg(|s| {
        s.zero_padding = false;
        s.window_samples = 3175;
        s.layers = 2;
    });
    assert_eq!(c.fft_size, 6350);
    assert!(c.validate().is_ok());
    // Kaiser auto beta from the dB range.
    let c = cfg(|s| {
        s.kaiser_beta_mode = KaiserBetaMode::Auto;
        s.db_range = 80.0;
    });
    assert!((c.kaiser_beta() - 10.73).abs() < 0.01);
    // Layers shrink to keep the longest window within 65,536.
    let c = cfg(|s| {
        s.window_samples = 30_000;
        s.layers = 6;
    });
    assert_eq!(c.layers, 2);
}

#[test]
fn every_menu_combination_builds_and_stays_in_range() {
    let windows = [
        WindowShape::Kaiser,
        WindowShape::Hann,
        WindowShape::Hamming,
        WindowShape::Blackman,
        WindowShape::BlackmanHarris,
        WindowShape::Rectangular,
    ];
    let mut noise = 0x1234_5678u32;
    for (i, window) in windows.into_iter().enumerate() {
        for interp in [WarpInterp::Linear, WarpInterp::Cubic] {
            for agg in [
                WarpAggregation::Off,
                WarpAggregation::Peak,
                WarpAggregation::Rms,
            ] {
                for weighting in [
                    Weighting::Off,
                    Weighting::A,
                    Weighting::C,
                    Weighting::Itu468,
                ] {
                    let mut a = analyzer(cfg(|s| {
                        s.window = window;
                        s.warp_interp = interp;
                        s.warp_aggregation = agg;
                        s.weighting = weighting;
                        s.layers = 1 + (i as u32 % 3);
                        s.fft_size = 8192;
                        s.output_bins = 300;
                        s.eq.enabled = true;
                        s.ballistics = true;
                        s.db_reference = DbReference::Agc;
                    }));
                    run(&mut a, 0.05, |_| {
                        noise ^= noise << 13;
                        noise ^= noise >> 17;
                        noise ^= noise << 5;
                        (noise as f32 / u32::MAX as f32 - 0.5) * 0.5
                    });
                }
            }
        }
    }
}

#[test]
fn invalid_configs_are_rejected() {
    let mut plans = PlanCache::default();
    let c = SpectrumConfig {
        hop_samples: 0,
        ..SpectrumConfig::default()
    };
    assert!(Analyzer::new(c.clone(), plans.get(c.fft_size)).is_err());
    let c = SpectrumConfig::default().at_rate(4_000);
    assert!(Analyzer::new(c.clone(), plans.get(c.fft_size)).is_err());
    let c = SpectrumConfig::default();
    assert!(Analyzer::new(c, plans.get(1024)).is_err(), "plan mismatch");
}
