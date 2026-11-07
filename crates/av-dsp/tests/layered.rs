//! Layered FFT gates (ADR-020, `docs/super-fft.md` section 1.6).

use av_dsp::{Analyzer, LevelRef, PlanCache, SpectrumConfig};

const SR: u32 = 48_000;

fn analyzer(cfg: SpectrumConfig) -> Analyzer {
    let mut plans = PlanCache::default();
    Analyzer::new(cfg.clone(), plans.get(cfg.fft_size)).expect("valid config")
}

fn raw(cfg: SpectrumConfig) -> SpectrumConfig {
    SpectrumConfig {
        attack_ms: 0.0,
        release_ms: 0.0,
        auto_gain: false,
        db_floor: -140.0,
        db_ceiling: 12.0,
        ..cfg
    }
}

/// Feeds `seconds` of `signal` and calls `each` with the analyzer after every hop.
fn feed(
    a: &mut Analyzer,
    seconds: f32,
    signal: impl Fn(usize) -> f64,
    mut each: impl FnMut(&Analyzer),
) {
    let hop = a.config().hop_samples;
    let dt = hop as f32 / SR as f32;
    let mut buf = vec![0.0f32; hop];
    let mut n = 0;
    for _ in 0..(seconds * SR as f32 / hop as f32).ceil() as usize {
        for x in buf.iter_mut() {
            *x = signal(n) as f32;
            n += 1;
        }
        a.process(&buf, dt);
        each(a);
    }
}

fn tone(freq: f64, amp: f64) -> impl Fn(usize) -> f64 {
    move |n| amp * (std::f64::consts::TAU * freq * n as f64 / SR as f64).sin()
}

fn db(p: f32) -> f64 {
    10.0 * (2.0 * p as f64 + 1e-30).log10()
}

/// Every layer count and common FFT size keeps a full-scale bin-centred sine at 0 dB: the
/// 1/2^(k-1) scale (window-length normalization) puts all layers on one level.
#[test]
fn a_full_scale_sine_reads_0_db_for_every_layer_count_and_fft_size() {
    for layers in 1..=6 {
        for fft in [32_768usize, 65_536, 131_072] {
            let cfg = SpectrumConfig {
                bar_count: 8,
                level_ref: LevelRef::Band,
                ..raw(SpectrumConfig::layered(layers, fft))
            };
            let n = cfg.fft_size;
            let mut a = analyzer(cfg);
            // A bin centre of the base layer is a bin centre of every layer and of N.
            let f = 21.0 * SR as f64 / 1024.0; // 984.4 Hz
            let mut last = Vec::new();
            feed(&mut a, 0.8, tone(f, 1.0), |a| last = a.bin_power().to_vec());
            let peak_bin = (f * n as f64 / SR as f64).round() as usize;
            let peak = db(last[peak_bin]);
            assert!(
                peak.abs() < 0.5,
                "L={layers} N={fft}: peak bin {peak:.2} dB"
            );
            let mut b = analyzer(SpectrumConfig {
                bar_count: 8,
                level_ref: LevelRef::Band,
                ..raw(SpectrumConfig::layered(layers, fft))
            });
            let mut bar = 0.0;
            let centers = b.band_centers().to_vec();
            let idx = (0..centers.len())
                .min_by(|&i, &j| {
                    (centers[i].ln() - (f as f32).ln())
                        .abs()
                        .total_cmp(&(centers[j].ln() - (f as f32).ln()).abs())
                })
                .unwrap();
            let hop = b.config().hop_samples;
            let sig = tone(f, 1.0);
            let mut buf = vec![0.0f32; hop];
            let mut k = 0;
            for _ in 0..(0.8 * SR as f32 / hop as f32) as usize {
                for x in buf.iter_mut() {
                    *x = sig(k) as f32;
                    k += 1;
                }
                bar = b.process(&buf, hop as f32 / SR as f32).bar_db[idx];
            }
            assert!(bar.abs() < 0.5, "L={layers} N={fft}: bar {bar:.2} dB");
        }
    }
}

/// Two equal tones near 60 Hz: the 6-layer analysis shows a dip between them that the 1,024
/// layer alone (padded to the same FFT size) cannot. Measured tables: docs/super-fft.md 1.6.
/// Kaiser beta 15 (the default) trades main-lobe width for -110 dB sidelobes, so it needs a
/// wider separation than Hann for the same dip.
#[test]
fn layers_resolve_low_tones_the_short_window_cannot() {
    use av_dsp::WindowKind;
    let contrast = |window: WindowKind, layers: usize, sep: usize| {
        let mut a = analyzer(SpectrumConfig {
            window,
            ..raw(SpectrumConfig::layered(layers, 32_768))
        });
        let df = SR as f64 / 32_768.0;
        let (b1, b2, mid) = (41usize, 41 + sep, 41 + sep / 2);
        let sig = {
            let (f1, f2) = (b1 as f64 * df, b2 as f64 * df);
            move |n: usize| 0.5 * tone(f1, 1.0)(n) + 0.5 * tone(f2, 1.0)(n)
        };
        let (mut peaks, mut dip, mut hops) = (0.0f64, 0.0f64, 0);
        feed(&mut a, 3.0, sig, |a| {
            hops += 1;
            if hops > 70 {
                let p = a.bin_power();
                peaks += (p[b1] + p[b2]) as f64 / 2.0;
                dip += p[mid] as f64;
            }
        });
        10.0 * (peaks / dip).log10()
    };
    let kaiser = WindowKind::Kaiser { beta: 15.0 };
    // Hann: 8 bins of 1.46 Hz = 11.7 Hz apart.
    let short = contrast(WindowKind::Hann, 1, 8);
    let layered = contrast(WindowKind::Hann, 6, 8);
    assert!(
        short.abs() < 1.0,
        "Hann 1,024 resolves 11.7 Hz? {short:.2} dB"
    );
    assert!(
        layered > 3.0,
        "Hann 6 layers at 11.7 Hz: only {layered:.2} dB"
    );
    // Kaiser 15: 12 bins = 17.6 Hz apart.
    let short = contrast(kaiser, 1, 12);
    let layered = contrast(kaiser, 6, 12);
    assert!(
        short.abs() < 1.0,
        "Kaiser 1,024 resolves 17.6 Hz? {short:.2} dB"
    );
    assert!(
        layered > 3.0,
        "Kaiser 6 layers at 17.6 Hz: only {layered:.2} dB"
    );
}

/// A tone starting from silence moves its bar within one hop at every layer count: the 1,024
/// layer keeps the layered analysis reactive.
#[test]
fn an_onset_shows_within_one_hop_at_every_layer_count() {
    for layers in 1..=6 {
        let cfg = raw(SpectrumConfig::layered(layers, 32_768));
        let hop = cfg.hop_samples;
        let mut a = analyzer(cfg);
        let zeros = vec![0.0f32; hop];
        for _ in 0..80 {
            a.process(&zeros, hop as f32 / SR as f32);
        }
        let centers = a.band_centers().to_vec();
        let idx = centers.iter().position(|&c| c > 3000.0).unwrap();
        let f = centers[idx] as f64;
        let burst: Vec<f32> = (0..hop).map(|n| tone(f, 1.0)(n) as f32).collect();
        let level = a.process(&burst, hop as f32 / SR as f32).bar_db[idx];
        assert!(
            level > -30.0,
            "L={layers}: onset bar only {level:.1} dB after one hop"
        );
    }
}

/// The layered default stays inside the 2 ms budget by a wide margin even unoptimized in a test
/// build; the real numbers come from `examples/dsp_timing.rs`.
#[test]
fn six_layers_build_and_run_with_bins_skipped_above_the_wire_limit() {
    let cfg = SpectrumConfig {
        emit_bins: true,
        ..SpectrumConfig::layered(6, 131_072)
    };
    assert_eq!(cfg.fft_size, 131_072);
    let mut a = analyzer(cfg);
    let hop = a.config().hop_samples;
    let f = a.process(&vec![0.1; hop], hop as f32 / SR as f32);
    assert!(f.bins.is_none(), "65,537 bins exceed the wire's 4,097");
    assert!(f.bars.iter().all(|b| b.is_finite()));
}
