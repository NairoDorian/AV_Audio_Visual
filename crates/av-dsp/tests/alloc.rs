//! Allocation gate (pre-plan section 6): after 20 warm-up hops, 500 steady-state hops perform
//! zero heap allocations. Conceptual source: the FFT plugin study's zero-allocation gate.
//!
//! A counting global allocator is installed for this test binary only. The count is per thread
//! so the test harness's own threads cannot disturb it.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use av_dsp::{Analyzer, PlanCache, SpectrumConfig};

struct Counting;

thread_local! {
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
}

// SAFETY: forwards every call to `System` unchanged; only counts allocations on this thread.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCS.try_with(|c| c.set(c.get() + 1));
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` came from `System.alloc` with this layout.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let _ = ALLOCS.try_with(|c| c.set(c.get() + 1));
        // SAFETY: same contract as the caller's.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn allocations() -> usize {
    ALLOCS.with(Cell::get)
}

#[test]
fn steady_state_hops_never_allocate() {
    for emit_bins in [false, true] {
        let cfg = SpectrumConfig {
            emit_bins,
            ..SpectrumConfig::default()
        };
        let mut plans = PlanCache::default();
        let mut a = Analyzer::new(cfg.clone(), plans.get(cfg.fft_size)).unwrap();
        let hop: Vec<f32> = (0..cfg.hop_samples)
            .map(|n| (n as f32 * 0.37).sin() * 0.5)
            .collect();
        let dt = cfg.hop_samples as f32 / cfg.sample_rate as f32;
        for _ in 0..20 {
            a.process(&hop, dt);
        }
        let before = allocations();
        let mut checksum = 0.0f32;
        for _ in 0..500 {
            checksum += a.process(&hop, dt).bars[0];
        }
        let after = allocations();
        assert!(checksum.is_finite());
        assert_eq!(
            after - before,
            0,
            "allocations in 500 steady-state hops (emit_bins={emit_bins})"
        );
    }
}
