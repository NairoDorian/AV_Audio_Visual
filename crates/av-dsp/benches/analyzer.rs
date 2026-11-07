//! `cargo bench -p av-dsp`: one analyzer hop per preset (budget: < 2 ms per 512-sample hop at
//! 48 kHz, AGENTS.md section 4).

use av_dsp::{Analyzer, PlanCache, SpectrumConfig};
use av_protocol::config::{AnalysisConfig, FftPreset};
use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

fn bench(c: &mut Criterion) {
    let mut plans = PlanCache::default();
    for (name, preset) in [
        ("layered_default_l4_pad2", FftPreset::Layered),
        ("default_2048_48bars", FftPreset::Default),
        ("low_latency_1024", FftPreset::LowLatency),
        ("precision_8192", FftPreset::Precision),
    ] {
        let app = AnalysisConfig {
            preset,
            ..AnalysisConfig::default()
        };
        let cfg = SpectrumConfig::from_app(&app, 48_000);
        let mut a = Analyzer::new(cfg.clone(), plans.get(cfg.fft_size)).unwrap();
        let hop: Vec<f32> = (0..cfg.hop_samples)
            .map(|n| (n as f32 * 0.131).sin() * 0.5)
            .collect();
        let dt = cfg.hop_samples as f32 / 48_000.0;
        c.bench_function(name, |b| {
            b.iter(|| black_box(a.process(black_box(&hop), dt).bars[0]))
        });
    }
}

criterion_group!(benches, bench);
criterion_main!(benches);
