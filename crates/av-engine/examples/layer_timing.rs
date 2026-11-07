//! Sequential vs parallel FFT layers, per hop, p50/p99:
//! `cargo run --release -p av-engine --example layer_timing`.

use std::time::Instant;

use av_dsp::{Analyzer, PlanCache, SpectrumConfig};
use av_engine::parallel::ParallelLayers;

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
    println!("config              seq p50   seq p99   par p50   par p99   speedup(p50)");
    for fft in [16_384usize, 32_768, 65_536] {
        for layers in [2usize, 3, 4, 5, 6] {
            let cfg = SpectrumConfig::layered(layers, fft);
            if cfg.validate().is_err() {
                continue;
            }
            let plan = plans.get(fft);
            let seq = Analyzer::new(cfg.clone(), plan.clone()).unwrap();
            let par =
                Analyzer::with_runner(cfg.clone(), plan, |u| Box::new(ParallelLayers::new(u)))
                    .unwrap();
            let (s50, s99) = time(seq, cfg.hop_samples);
            let (p50, p99) = time(par, cfg.hop_samples);
            println!(
                "L={layers} N={fft:<6}   {s50:>8.1}  {s99:>8.1}  {p50:>8.1}  {p99:>8.1}   {:>5.2}x",
                s50 / p50
            );
        }
    }
}
