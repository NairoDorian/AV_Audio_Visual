//! Layered FFT gates (ADR-020, `docs/super-fft.md` section 1.6).

use av_dsp::{Analyzer, PlanCache, SpectrumConfig};
use av_protocol::config::{MagnitudeNorm, WindowShape};

const SR: u32 = 48_000;

fn analyzer(cfg: SpectrumConfig) -> Analyzer {
    let mut plans = PlanCache::default();
    Analyzer::new(cfg.clone(), plans.get(cfg.fft_size)).expect("valid config")
}

/// Full-scale magnitudes and the whole spectrum computed (raw bins), so `linear_magnitude`
/// holds every FFT bin.
fn measured(layers: usize, fft: usize, window: WindowShape) -> SpectrumConfig {
    let mut c = SpectrumConfig::layered(layers, fft);
    c.settings.magnitude_norm = MagnitudeNorm::FullScale;
    c.settings.raw_bins = true;
    c.settings.window = window;
    SpectrumConfig::new(c.settings, SR)
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

fn db(m: f32) -> f64 {
    20.0 * (m as f64 + 1e-30).log10()
}

/// Every layer count and common FFT size keeps a full-scale bin-centred sine at 0 dB: the
/// 1/2^(k-1) scale (window-length normalization) puts all layers on one level.
#[test]
fn a_full_scale_sine_reads_0_db_for_every_layer_count_and_fft_size() {
    for layers in 1..=6 {
        for fft in [32_768usize, 65_536, 131_072] {
            let cfg = measured(layers, fft, WindowShape::Kaiser);
            let n = cfg.fft_size;
            let mut a = analyzer(cfg);
            // A bin centre of the base layer is a bin centre of every layer and of N.
            let f = 21.0 * SR as f64 / 1024.0; // 984.4 Hz
            let mut last = Vec::new();
            feed(&mut a, 0.8, tone(f, 1.0), |a| {
                last = a.linear_magnitude().to_vec()
            });
            let peak_bin = (f * n as f64 / SR as f64).round() as usize;
            let peak = db(last[peak_bin]);
            assert!(
                peak.abs() < 0.5,
                "L={layers} N={fft}: peak bin {peak:.2} dB"
            );
        }
    }
}

/// Two equal tones near 60 Hz: the 6-layer analysis shows a dip between them that the 1,024
/// layer alone (padded to the same FFT size) cannot. Measured tables: docs/super-fft.md 1.6.
/// Kaiser beta 15 (the default) trades main-lobe width for -110 dB sidelobes, so it needs a
/// wider separation than Hann for the same dip.
#[test]
fn layers_resolve_low_tones_the_short_window_cannot() {
    let contrast = |window: WindowShape, layers: usize, sep: usize| {
        let mut a = analyzer(measured(layers, 32_768, window));
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
                let m = a.linear_magnitude();
                let p = |k: usize| (m[k] as f64) * (m[k] as f64);
                peaks += (p(b1) + p(b2)) / 2.0;
                dip += p(mid);
            }
        });
        10.0 * (peaks / dip).log10()
    };
    // Hann: 8 bins of 1.46 Hz = 11.7 Hz apart.
    let short = contrast(WindowShape::Hann, 1, 8);
    let layered = contrast(WindowShape::Hann, 6, 8);
    assert!(
        short.abs() < 1.0,
        "Hann 1,024 resolves 11.7 Hz? {short:.2} dB"
    );
    assert!(
        layered > 3.0,
        "Hann 6 layers at 11.7 Hz: only {layered:.2} dB"
    );
    // Kaiser 15: 12 bins = 17.6 Hz apart.
    let short = contrast(WindowShape::Kaiser, 1, 12);
    let layered = contrast(WindowShape::Kaiser, 6, 12);
    assert!(
        short.abs() < 1.0,
        "Kaiser 1,024 resolves 17.6 Hz? {short:.2} dB"
    );
    assert!(
        layered > 3.0,
        "Kaiser 6 layers at 17.6 Hz: only {layered:.2} dB"
    );
}

/// A tone starting from silence lifts its output bin within one hop at every layer count: the
/// 1,024 layer keeps the layered analysis reactive.
#[test]
fn an_onset_shows_within_one_hop_at_every_layer_count() {
    for layers in 1..=6 {
        let cfg = SpectrumConfig::layered(layers, 32_768);
        let hop = cfg.hop_samples;
        let mut a = analyzer(cfg);
        let zeros = vec![0.0f32; hop];
        for _ in 0..80 {
            a.process(&zeros, hop as f32 / SR as f32);
        }
        let t = a.target_hz().to_vec();
        let idx = t.iter().position(|&c| c > 3000.0).unwrap();
        let burst: Vec<f32> = (0..hop).map(|n| tone(t[idx], 1.0)(n) as f32).collect();
        // dB display over 80 dB: 0.6 is -32 dB.
        let level = a.process(&burst, hop as f32 / SR as f32).bars[idx];
        assert!(
            level > 0.6,
            "L={layers}: onset bin only {level:.2} after one hop"
        );
    }
}

#[test]
fn six_layers_at_the_largest_fft_build_and_run() {
    let cfg = SpectrumConfig::layered(6, 131_072);
    assert_eq!(cfg.fft_size, 131_072);
    let mut a = analyzer(cfg);
    let hop = a.config().hop_samples;
    let f = a.process(&vec![0.1; hop], hop as f32 / SR as f32);
    assert!(f.bars.iter().all(|b| b.is_finite()));
}
