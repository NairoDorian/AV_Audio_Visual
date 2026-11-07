//! Parallel FFT layers: one core per layer (`docs/super-fft.md` 1.5).
//!
//! Layer 0 runs on the analysis thread; every other layer lives on its own `av-layer-<k>` thread
//! for the analyzer's lifetime. Per hop the analysis thread copies each worker's newest samples
//! into that worker's preallocated job, flips the worker's state to WORK and unparks it, computes
//! layer 0, then waits for each worker **in layer order** and sums its magnitudes, so the result
//! is bit-identical to [`av_dsp::SequentialLayers`].
//!
//! Hand-off without allocation: a per-worker atomic state (IDLE -> WORK -> DONE), the job in an
//! uncontended `Mutex<Option<Box<Job>>>` (the state machine guarantees one side at a time; a
//! futex mutex never allocates), and `thread::park`/`unpark` after a short spin. `std::sync::mpsc`
//! was measured to allocate occasionally the first time a blocking `recv` parks, so it is not used.
//!
//! Cost: one parked thread per extra layer; per hop, one wake-up and one hand-back per worker
//! plus a copy of each layer's window (`examples/layer_timing.rs`, docs/performance.md).

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::thread::{JoinHandle, Thread};

use av_dsp::layers::newest;
use av_dsp::{LayerRunner, LayerUnit};

const IDLE: u8 = 0;
const WORK: u8 = 1;
const DONE: u8 = 2;
const STOP: u8 = 3;
/// Spins before parking: an FFT takes 100-300 us, a park/unpark round trip tens of us.
const SPIN: u32 = 2_000;

struct Job {
    unit: LayerUnit,
    input: Vec<f32>,
}

struct Shared {
    state: AtomicU8,
    job: Mutex<Option<Box<Job>>>,
    /// The analysis thread, registered on the first `run` (during warm-up).
    caller: OnceLock<Thread>,
}

struct Worker {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

pub struct ParallelLayers {
    first: LayerUnit,
    workers: Vec<Worker>,
}

impl std::fmt::Debug for ParallelLayers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParallelLayers")
            .field("workers", &self.workers.len())
            .finish()
    }
}

fn wait_while(state: &AtomicU8, current: u8) -> u8 {
    for _ in 0..SPIN {
        let s = state.load(Ordering::Acquire);
        if s != current {
            return s;
        }
        std::hint::spin_loop();
    }
    loop {
        let s = state.load(Ordering::Acquire);
        if s != current {
            return s;
        }
        // Spurious wake-ups and unparks meant for another wait are fine: the state decides.
        std::thread::park();
    }
}

fn lock(job: &Mutex<Option<Box<Job>>>) -> std::sync::MutexGuard<'_, Option<Box<Job>>> {
    job.lock().unwrap_or_else(PoisonError::into_inner)
}

impl ParallelLayers {
    /// Takes ownership of the units; units `1..` run on their own threads. A layer whose thread
    /// cannot be spawned is computed inline instead, so this never fails.
    pub fn new(mut units: Vec<LayerUnit>) -> Self {
        assert!(!units.is_empty(), "at least one layer");
        let rest = units.split_off(1);
        let first = units.pop().expect("one unit");
        let workers = rest
            .into_iter()
            .enumerate()
            .map(|(i, unit)| {
                let input = vec![0.0; unit.len()];
                let shared = Arc::new(Shared {
                    state: AtomicU8::new(IDLE),
                    job: Mutex::new(Some(Box::new(Job { unit, input }))),
                    caller: OnceLock::new(),
                });
                let theirs = shared.clone();
                let thread = std::thread::Builder::new()
                    .name(format!("av-layer-{}", i + 1))
                    .spawn(move || worker_loop(&theirs))
                    .ok();
                Worker { shared, thread }
            })
            .collect();
        Self { first, workers }
    }
}

fn worker_loop(shared: &Shared) {
    loop {
        match wait_while(&shared.state, IDLE) {
            STOP => return,
            WORK => {
                if let Some(job) = lock(&shared.job).as_mut() {
                    let Job { unit, input } = &mut **job;
                    unit.compute(input, &[]);
                }
                shared.state.store(DONE, Ordering::Release);
                if let Some(caller) = shared.caller.get() {
                    caller.unpark();
                }
                // Wait until the caller has collected (state leaves DONE).
                if wait_while(&shared.state, DONE) == STOP {
                    return;
                }
            }
            _ => {}
        }
    }
}

impl LayerRunner for ParallelLayers {
    fn run(&mut self, history: &[f32], write_pos: usize, sum: &mut [f32]) {
        // Dispatch every worker first so they run while layer 0 computes here.
        for w in &self.workers {
            if w.thread.is_none() {
                continue;
            }
            w.shared.caller.get_or_init(std::thread::current);
            if let Some(job) = lock(&w.shared.job).as_mut() {
                let (older, newer) = newest(history, write_pos, job.input.len());
                job.input[..older.len()].copy_from_slice(older);
                job.input[older.len()..].copy_from_slice(newer);
            }
            w.shared.state.store(WORK, Ordering::Release);
            if let Some(t) = &w.thread {
                t.thread().unpark();
            }
        }
        let (older, newer) = newest(history, write_pos, self.first.len());
        sum.copy_from_slice(self.first.compute(older, newer));
        for w in &self.workers {
            if w.thread.is_none() {
                // No thread for this layer: compute it here (only after a failed spawn).
                if let Some(job) = lock(&w.shared.job).as_mut() {
                    let (older, newer) = newest(history, write_pos, job.unit.len());
                    for (s, &m) in sum.iter_mut().zip(job.unit.compute(older, newer)) {
                        *s += m;
                    }
                }
                continue;
            }
            wait_while(&w.shared.state, WORK);
            if let Some(job) = lock(&w.shared.job).as_ref() {
                for (s, &m) in sum.iter_mut().zip(job.unit.magnitude()) {
                    *s += m;
                }
            }
            w.shared.state.store(IDLE, Ordering::Release);
            if let Some(t) = &w.thread {
                t.thread().unpark();
            }
        }
    }

    fn layer_count(&self) -> usize {
        1 + self.workers.len()
    }
}

impl Drop for ParallelLayers {
    fn drop(&mut self) {
        for w in &self.workers {
            w.shared.state.store(STOP, Ordering::Release);
            if let Some(t) = &w.thread {
                t.thread().unpark();
            }
        }
        for w in &mut self.workers {
            if let Some(t) = w.thread.take() {
                let _ = t.join();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use av_dsp::{Analyzer, PlanCache, SpectrumConfig};

    fn tone_hop(n0: usize, len: usize) -> Vec<f32> {
        (n0..n0 + len)
            .map(|n| {
                let t = n as f32 / 48_000.0;
                0.4 * (std::f32::consts::TAU * 440.0 * t).sin()
                    + 0.2 * (std::f32::consts::TAU * 3_100.0 * t).sin()
            })
            .collect()
    }

    /// The parallel runner is a scheduling change only: bars, dB and bins match the sequential
    /// analyzer bit for bit.
    #[test]
    fn parallel_layers_are_bit_identical_to_sequential() {
        for layers in [2usize, 4, 6] {
            let cfg = SpectrumConfig::layered(layers, 32_768);
            let mut plans = PlanCache::default();
            let fft = plans.get(cfg.fft_size);
            let mut seq = Analyzer::new(cfg.clone(), fft.clone()).unwrap();
            let mut par = Analyzer::with_runner(cfg.clone(), fft, |units| {
                Box::new(ParallelLayers::new(units))
            })
            .unwrap();
            let hop = cfg.hop_samples;
            for h in 0..120 {
                let x = tone_hop(h * hop, hop);
                let a = seq.process(&x, hop as f32 / 48_000.0).bar_db.to_vec();
                let b = par.process(&x, hop as f32 / 48_000.0).bar_db.to_vec();
                let bits = |v: &[f32]| v.iter().map(|f| f.to_bits()).collect::<Vec<_>>();
                assert_eq!(bits(&a), bits(&b), "L={layers} hop {h}");
            }
            assert_eq!(seq.bin_power(), par.bin_power());
        }
    }

    #[test]
    fn dropping_the_runner_stops_its_threads() {
        let cfg = SpectrumConfig::layered(3, 8192);
        let fft = PlanCache::default().get(cfg.fft_size);
        let runner = ParallelLayers::new(cfg.build_units(&fft));
        assert_eq!(runner.layer_count(), 3);
        drop(runner); // joins; a hang here fails the test by timeout
    }

    /// The analyzer is built on one thread and runs on another (as in the engine).
    #[test]
    fn works_when_moved_to_another_thread() {
        let cfg = SpectrumConfig::layered(4, 16_384);
        let fft = PlanCache::default().get(cfg.fft_size);
        let mut a =
            Analyzer::with_runner(cfg.clone(), fft, |u| Box::new(ParallelLayers::new(u))).unwrap();
        let hop = cfg.hop_samples;
        let out = std::thread::spawn(move || {
            let x = tone_hop(0, hop);
            for _ in 0..50 {
                a.process(&x, hop as f32 / 48_000.0);
            }
            a.process(&x, hop as f32 / 48_000.0).bar_db.to_vec()
        })
        .join()
        .unwrap();
        assert!(out.iter().all(|v| v.is_finite()));
    }
}
