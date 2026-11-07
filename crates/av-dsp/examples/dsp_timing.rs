//! Per-hop latency distribution for the DSP gate (p50/p99, not just a mean):
//! `cargo run --release -p av-dsp --example dsp_timing`.
//!
//! The library itself has no clocks; timing lives here.

use std::time::Instant;

use av_dsp::{Analyzer, PlanCache, SpectrumConfig};
use av_protocol::config::{AnalysisConfig, FftPreset};

fn main() {
    let mut plans = PlanCache::default();
    println!("preset           hops     p50 us   p99 us   max us");
    for (name, preset) in [
        ("default 2048", FftPreset::Default),
        ("low-latency 1024", FftPreset::LowLatency),
        ("precision 8192", FftPreset::Precision),
    ] {
        let app = AnalysisConfig {
            preset,
            bar_count: 128,
            ..AnalysisConfig::default()
        };
        let cfg = SpectrumConfig {
            emit_bins: true,
            ..SpectrumConfig::from_app(&app, 48_000)
        };
        let mut a = Analyzer::new(cfg.clone(), plans.get(cfg.fft_size)).unwrap();
        let mut phase = 0.0f32;
        let mut hop = vec![0.0f32; cfg.hop_samples];
        let dt = cfg.hop_samples as f32 / 48_000.0;
        let hops = 20_000;
        let mut times = Vec::with_capacity(hops);
        for i in 0..hops + 100 {
            for x in hop.iter_mut() {
                phase += 0.0917;
                *x = phase.sin() * 0.4;
            }
            let t = Instant::now();
            std::hint::black_box(a.process(&hop, dt).bars[0]);
            if i >= 100 {
                times.push(t.elapsed().as_secs_f64() * 1e6);
            }
        }
        times.sort_by(f64::total_cmp);
        let q = |p: f64| times[((times.len() - 1) as f64 * p) as usize];
        println!(
            "{name:<16} {hops:>6} {:>9.1} {:>8.1} {:>8.1}",
            q(0.5),
            q(0.99),
            times[times.len() - 1]
        );
    }
}
