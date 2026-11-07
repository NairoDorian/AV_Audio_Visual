//! `cargo bench -p av-dsp`: one analyzer hop per configuration (budget: < 2 ms per 512-sample
//! hop at 48 kHz, AGENTS.md section 4).

use av_dsp::{Analyzer, PlanCache, SpectrumConfig};
use av_protocol::config::{AnalysisConfig, OutputBinsMode, WarpInterp};
use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

fn bench(c: &mut Criterion) {
    let mut plans = PlanCache::default();
    let d = AnalysisConfig::default();
    for (name, s) in [
        ("default_l4_n32k_1024bins", d.clone()),
        (
            "auto_bins_cubic_l4_n32k",
            AnalysisConfig {
                output_bins_mode: OutputBinsMode::Auto,
                warp_interp: WarpInterp::Cubic,
                ..d.clone()
            },
        ),
        (
            "single_2048",
            AnalysisConfig {
                layers: 1,
                window_samples: 2048,
                fft_size: 2048,
                ..d.clone()
            },
        ),
        (
            "single_1024_hop256",
            AnalysisConfig {
                layers: 1,
                window_samples: 1024,
                fft_size: 1024,
                hop_samples: 256,
                ..d
            },
        ),
    ] {
        let cfg = SpectrumConfig::new(s, 48_000);
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
