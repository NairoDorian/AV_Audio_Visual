//! Two equal tones near 60 Hz at several separations: dip contrast (peak power / midpoint power,
//! dB) per layer count, all at FFT size 32,768. `cargo run --release -p av-dsp --example
//! layer_resolution`. Feeds docs/super-fft.md section 1.6.

use av_dsp::{Analyzer, PlanCache, SpectrumConfig};

const SR: u32 = 48_000;

fn contrast(layers: usize, sep_bins: usize) -> f64 {
    let mut cfg = SpectrumConfig::layered(layers, 32_768);
    cfg.fft_size = 32_768;
    cfg.auto_gain = false;
    let mut a = Analyzer::new(cfg.clone(), PlanCache::default().get(32_768)).unwrap();
    let df = SR as f64 / 32_768.0;
    let b1 = 41usize;
    let b2 = b1 + sep_bins;
    let mid = (b1 + b2) / 2;
    let (f1, f2) = (b1 as f64 * df, b2 as f64 * df);
    let hop = cfg.hop_samples;
    let mut buf = vec![0.0f32; hop];
    let (mut peaks, mut dip) = (0.0f64, 0.0f64);
    let mut n = 0usize;
    for h in 0..300 {
        for x in buf.iter_mut() {
            let t = n as f64 / SR as f64;
            *x = (0.5 * (std::f64::consts::TAU * f1 * t).sin()
                + 0.5 * (std::f64::consts::TAU * f2 * t).sin()) as f32;
            n += 1;
        }
        a.process(&buf, hop as f32 / SR as f32);
        if h > 70 {
            let p = a.bin_power();
            peaks += (p[b1] + p[b2]) as f64 / 2.0;
            dip += p[mid] as f64;
        }
    }
    10.0 * (peaks / dip).log10()
}

fn main() {
    let df = SR as f64 / 32_768.0;
    print!("separation ");
    for l in 1..=6 {
        print!("   L={l}  ");
    }
    println!();
    for sep in [4usize, 6, 8, 12, 20, 40] {
        print!("{:>7.1} Hz ", sep as f64 * df);
        for l in 1..=6 {
            print!("{:>7.2}  ", contrast(l, sep));
        }
        println!();
    }
}
