//! Per-hop latency distribution for the DSP gate (p50/p99, not just a mean):
//! `cargo run --release -p av-dsp --example dsp_timing`.
//!
//! The library itself has no clocks; timing lives here.

use std::time::Instant;

use av_dsp::{Analyzer, PlanCache, SpectrumConfig};

fn measure(name: &str, cfg: SpectrumConfig, plans: &mut PlanCache) {
    let cfg = SpectrumConfig {
        emit_bins: true,
        bar_count: 128,
        ..cfg
    };
    let mut a = Analyzer::new(cfg.clone(), plans.get(cfg.fft_size)).unwrap();
    let mut phase = 0.0f32;
    let mut hop = vec![0.0f32; cfg.hop_samples];
    let dt = cfg.hop_samples as f32 / 48_000.0;
    let hops = 5_000;
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
        "{name:<26} {:>7} {:>9.1} {:>8.1} {:>8.1}",
        cfg.fft_size,
        q(0.5),
        q(0.99),
        times[times.len() - 1]
    );
}

fn main() {
    let mut plans = PlanCache::default();
    println!("preset                         N     p50 us   p99 us   max us");
    measure(
        "classic 1024 / 256",
        SpectrumConfig::single(1024, 1024, 256),
        &mut plans,
    );
    measure(
        "classic 2048 / 512",
        SpectrumConfig::single(2048, 2048, 512),
        &mut plans,
    );
    measure(
        "classic 8192 / 1024",
        SpectrumConfig::single(8192, 8192, 1024),
        &mut plans,
    );
    for layers in 2..=6 {
        for fft in [16_384usize, 32_768, 65_536] {
            let cfg = SpectrumConfig::layered(layers, fft);
            if fft < cfg.largest_window() {
                continue;
            }
            measure(&format!("layered L={layers} N={fft}"), cfg, &mut plans);
        }
    }
}
