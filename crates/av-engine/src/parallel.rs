//! Parallel FFT layers over a fixed number of threads (`docs/super-fft.md` 1.5; FFT CHOP v2.13
//! `LayerPool`).
//!
//! The layers are split over `threads` groups by a static assignment (layer `k` -> group
//! `k % threads`; every layer shares the FFT size, so their costs are equal and round-robin is the
//! balanced, longest-first-equivalent split). Group 0 runs on the analysis thread; every other group
//! lives on its own `av-layer-<g>` thread for the analyzer's lifetime. Per hop the analysis thread
//! copies each worker's due layers' newest samples into that worker's preallocated inputs,
//! publishes the hop's [`LayerPlan`], flips the worker to WORK and unparks it, computes group 0,
//! waits for every worker, then sums the included magnitudes **in layer order**, so the result is
//! bit-identical to [`av_dsp::SequentialLayers`] at any thread count.
//!
//! Hand-off without allocation: a per-worker atomic state (IDLE -> WORK -> DONE), the jobs in an
//! uncontended `Mutex<Vec<Slot>>` (the state machine guarantees one side at a time; a futex mutex
//! never allocates), and `thread::park`/`unpark` after a short spin. `std::sync::mpsc` was measured
//! to allocate occasionally the first time a blocking `recv` parks, so it is not used.
//!
//! Cost: `threads - 1` parked threads; per hop, one wake-up and one hand-back per worker plus a
//! copy of each computed layer's window (`examples/layer_timing.rs`, docs/performance.md).

use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::thread::{JoinHandle, Thread};

use av_dsp::layers::{LayerPlan, accumulate, newest};
use av_dsp::{LayerRunner, LayerUnit};

const IDLE: u8 = 0;
const WORK: u8 = 1;
const DONE: u8 = 2;
const STOP: u8 = 3;
/// Spins before parking: an FFT takes 100-300 us, a park/unpark round trip tens of us.
const SPIN: u32 = 2_000;
/// Most layers any analyzer has (`MAX_LAYERS`).
const MAX_UNITS: usize = 6;

/// One layer of a worker's group and its input buffer.
struct Slot {
    layer: usize,
    unit: LayerUnit,
    input: Vec<f32>,
}

struct Shared {
    state: AtomicU8,
    /// The hop's `LayerPlan::compute` bits, written before WORK.
    compute: AtomicU32,
    slots: Mutex<Vec<Slot>>,
    /// The analysis thread, registered on the first `run` (during warm-up).
    caller: OnceLock<Thread>,
}

struct Worker {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

pub struct ParallelLayers {
    /// Group 0, computed on the calling thread.
    local: Vec<(usize, LayerUnit)>,
    workers: Vec<Worker>,
    /// For every layer: `(group, index in the group)`; group 0 is `local`.
    place: Vec<(usize, usize)>,
}

impl std::fmt::Debug for ParallelLayers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParallelLayers")
            .field("layers", &self.place.len())
            .field("threads", &(1 + self.workers.len()))
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

fn lock(slots: &Mutex<Vec<Slot>>) -> MutexGuard<'_, Vec<Slot>> {
    slots.lock().unwrap_or_else(PoisonError::into_inner)
}

impl ParallelLayers {
    /// One thread per layer (the analysis thread runs layer 0).
    pub fn new(units: Vec<LayerUnit>) -> Self {
        let n = units.len();
        Self::with_threads(units, n)
    }

    /// Spreads the units over `threads` threads (clamped to 1..=layers), the calling thread
    /// included. A worker whose thread cannot be spawned is computed inline instead, so this
    /// never fails.
    pub fn with_threads(units: Vec<LayerUnit>, threads: usize) -> Self {
        assert!(!units.is_empty(), "at least one layer");
        assert!(units.len() <= MAX_UNITS, "at most {MAX_UNITS} layers");
        let threads = threads.clamp(1, units.len());
        let mut groups: Vec<Vec<Slot>> = (0..threads).map(|_| Vec::new()).collect();
        let mut place = Vec::with_capacity(units.len());
        for (k, unit) in units.into_iter().enumerate() {
            let g = k % threads;
            place.push((g, groups[g].len()));
            let input = vec![0.0; unit.len()];
            groups[g].push(Slot {
                layer: k,
                unit,
                input,
            });
        }
        let mut groups = groups.into_iter();
        let local = groups
            .next()
            .expect("group 0")
            .into_iter()
            .map(|s| (s.layer, s.unit))
            .collect();
        let workers = groups
            .enumerate()
            .map(|(i, slots)| {
                let shared = Arc::new(Shared {
                    state: AtomicU8::new(IDLE),
                    compute: AtomicU32::new(0),
                    slots: Mutex::new(slots),
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
        Self {
            local,
            workers,
            place,
        }
    }
}

fn compute_slots(slots: &mut [Slot], compute: u32) {
    for s in slots.iter_mut() {
        if compute & (1 << s.layer) != 0 {
            s.unit.compute(&s.input, &[]);
        }
    }
}

fn worker_loop(shared: &Shared) {
    loop {
        match wait_while(&shared.state, IDLE) {
            STOP => return,
            WORK => {
                let compute = shared.compute.load(Ordering::Acquire);
                compute_slots(&mut lock(&shared.slots), compute);
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
    fn run(&mut self, history: &[f32], write_pos: usize, plan: LayerPlan, sum: &mut [f32]) {
        let compute = plan.compute & plan.include;
        // Dispatch every worker with due layers first, so they run while group 0 computes here.
        let mut dispatched = [false; MAX_UNITS];
        for (g, w) in self.workers.iter().enumerate() {
            let mut slots = lock(&w.shared.slots);
            if slots.iter().all(|s| compute & (1 << s.layer) == 0) {
                continue;
            }
            for s in slots.iter_mut().filter(|s| compute & (1 << s.layer) != 0) {
                let (older, newer) = newest(history, write_pos, s.input.len());
                s.input[..older.len()].copy_from_slice(older);
                s.input[older.len()..].copy_from_slice(newer);
            }
            if w.thread.is_none() {
                // No thread for this group (only after a failed spawn): compute it here.
                compute_slots(&mut slots, compute);
                continue;
            }
            drop(slots);
            w.shared.caller.get_or_init(std::thread::current);
            w.shared.compute.store(compute, Ordering::Release);
            w.shared.state.store(WORK, Ordering::Release);
            if let Some(t) = &w.thread {
                t.thread().unpark();
            }
            dispatched[g] = true;
        }
        for (k, unit) in self.local.iter_mut() {
            if compute & (1 << *k) != 0 {
                let (older, newer) = newest(history, write_pos, unit.len());
                unit.compute(older, newer);
            }
        }
        for (g, w) in self.workers.iter().enumerate() {
            if dispatched[g] {
                wait_while(&w.shared.state, WORK);
            }
        }

        // Sum in layer order. Every worker is DONE or idle, so its slots are free to read.
        let guards: [Option<MutexGuard<'_, Vec<Slot>>>; MAX_UNITS] =
            std::array::from_fn(|g| self.workers.get(g).map(|w| lock(&w.shared.slots)));
        let mut first = true;
        for (k, &(g, i)) in self.place.iter().enumerate() {
            if !plan.includes(k) {
                continue;
            }
            let mag = if g == 0 {
                self.local[i].1.magnitude()
            } else {
                guards[g - 1].as_ref().expect("worker exists")[i]
                    .unit
                    .magnitude()
            };
            accumulate(sum, mag, first);
            first = false;
        }
        drop(guards);
        if first {
            sum.fill(0.0);
        }

        for (g, w) in self.workers.iter().enumerate() {
            if dispatched[g] {
                w.shared.state.store(IDLE, Ordering::Release);
                if let Some(t) = &w.thread {
                    t.thread().unpark();
                }
            }
        }
    }

    fn layer_count(&self) -> usize {
        self.place.len()
    }

    fn first_magnitude(&self) -> &[f32] {
        // Layer 0 is always in group 0.
        self.local[0].1.magnitude()
    }

    fn threads(&self) -> usize {
        1 + self.workers.iter().filter(|w| w.thread.is_some()).count()
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
    use av_protocol::config::LayerRefresh;

    fn tone_hop(n0: usize, len: usize) -> Vec<f32> {
        (n0..n0 + len)
            .map(|n| {
                let t = n as f32 / 48_000.0;
                0.4 * (std::f32::consts::TAU * 440.0 * t).sin()
                    + 0.2 * (std::f32::consts::TAU * 3_100.0 * t).sin()
            })
            .collect()
    }

    fn bits(v: &[f32]) -> Vec<u32> {
        v.iter().map(|f| f.to_bits()).collect()
    }

    /// The parallel runner is a scheduling change only: at every thread count, with Hop-Scaled
    /// refresh and with layers falling silent, the display and the summed magnitude match the
    /// sequential analyzer bit for bit (FFT CHOP v2.13: "identical at any thread count").
    #[test]
    fn every_thread_count_is_bit_identical_to_sequential() {
        for refresh in [LayerRefresh::EveryFrame, LayerRefresh::HopScaled] {
            let mut s = SpectrumConfig::layered(6, 32_768).settings;
            s.layer_refresh = refresh;
            let cfg = SpectrumConfig::new(s, 48_000);
            let mut plans = PlanCache::default();
            let fft = plans.get(cfg.fft_size);
            let mut seq = Analyzer::new(cfg.clone(), fft.clone()).unwrap();
            let mut pars: Vec<Analyzer> = (1..=6)
                .map(|t| {
                    Analyzer::with_runner(cfg.clone(), fft.clone(), move |u| {
                        Box::new(ParallelLayers::with_threads(u, t))
                    })
                    .unwrap()
                })
                .collect();
            let hop = cfg.hop_samples;
            // Sound, then silence long enough for the short layers to drop out one by one.
            for h in 0..140 {
                let x = if h < 80 {
                    tone_hop(h * hop, hop)
                } else {
                    vec![0.0; hop]
                };
                let a = bits(seq.process(&x, hop as f32 / 48_000.0).bars);
                for (t, par) in pars.iter_mut().enumerate() {
                    let b = bits(par.process(&x, hop as f32 / 48_000.0).bars);
                    assert_eq!(a, b, "{refresh:?} threads={} hop {h}", t + 1);
                    assert_eq!(bits(seq.linear_magnitude()), bits(par.linear_magnitude()));
                }
            }
        }
    }

    #[test]
    fn threads_are_clamped_and_dropping_stops_them() {
        let cfg = SpectrumConfig::layered(3, 8192);
        let fft = PlanCache::default().get(cfg.fft_size);
        let runner = ParallelLayers::with_threads(cfg.build_units(&fft, cfg.bin_count()), 99);
        assert_eq!(runner.layer_count(), 3);
        assert_eq!(runner.threads(), 3);
        drop(runner); // joins; a hang here fails the test by timeout
        let one = ParallelLayers::with_threads(cfg.build_units(&fft, cfg.bin_count()), 1);
        assert_eq!(one.threads(), 1);
    }

    /// The analyzer is built on one thread and runs on another (as in the engine).
    #[test]
    fn works_when_moved_to_another_thread() {
        let cfg = SpectrumConfig::layered(4, 16_384);
        let fft = PlanCache::default().get(cfg.fft_size);
        let mut a = Analyzer::with_runner(cfg.clone(), fft, |u| {
            Box::new(ParallelLayers::with_threads(u, 2))
        })
        .unwrap();
        let hop = cfg.hop_samples;
        let out = std::thread::spawn(move || {
            let x = tone_hop(0, hop);
            for _ in 0..50 {
                a.process(&x, hop as f32 / 48_000.0);
            }
            a.process(&x, hop as f32 / 48_000.0).bars.to_vec()
        })
        .join()
        .unwrap();
        assert!(out.iter().all(|v| v.is_finite()));
    }
}
