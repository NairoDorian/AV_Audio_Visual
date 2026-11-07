//! FFT layer timing per hop, p50/p99: sequential vs every thread count, and Hop-Scaled refresh.
//! `cargo run --release -p av-engine --example layer_timing`.

use std::time::Instant;

use av_dsp::{Analyzer, PlanCache, SpectrumConfig};
use av_engine::parallel::ParallelLayers;
use av_protocol::config::LayerRefresh;

fn time(mut a: Analyzer, hop: usize) -> (f64, f64) {
    let mut phase = 0.0f32;
    let mut x = vec![0.0f32; hop];
    let mut times = Vec::with_capacity(3000);
    for i in 0..3100 {
        for s in x.iter_mut() {
            phase += 0.0917;
            *s = phase.sin() * 0.4;
        }
        let t = Instant::now();
        std::hint::black_box(a.process(&x, hop as f32 / 48_000.0).bars[0]);
        if i >= 100 {
            times.push(t.elapsed().as_secs_f64() * 1e6);
        }
    }
    times.sort_by(f64::total_cmp);
    let q = |p: f64| times[((times.len() - 1) as f64 * p) as usize];
    (q(0.5), q(0.99))
}

fn main() {
    let mut plans = PlanCache::default();
    println!("config                          threads  refresh      p50 us    p99 us");
    for (layers, fft) in [(4usize, 32_768usize), (6, 32_768), (6, 65_536)] {
        for refresh in [LayerRefresh::EveryFrame, LayerRefresh::HopScaled] {
            if layers < 4 && refresh == LayerRefresh::HopScaled {
                continue;
            }
            let mut s = SpectrumConfig::layered(layers, fft).settings;
            s.layer_refresh = refresh;
            let cfg = SpectrumConfig::new(s, 48_000);
            let plan = plans.get(cfg.fft_size);
            for threads in 1..=layers {
                let a = if threads == 1 {
                    Analyzer::new(cfg.clone(), plan.clone()).unwrap()
                } else {
                    Analyzer::with_runner(cfg.clone(), plan.clone(), move |u| {
                        Box::new(ParallelLayers::with_threads(u, threads))
                    })
                    .unwrap()
                };
                let (p50, p99) = time(a, cfg.hop_samples);
                println!(
                    "L={layers} N={fft:<6}                 {threads}     {:<10} {p50:>8.1} {p99:>9.1}",
                    format!("{refresh:?}")
                );
            }
        }
    }
}
