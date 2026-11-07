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
    // The layered default (4 layers, N = 32,768), six layers, a classic window, and every
    // optional stage on (features, weighting, cubic + RMS warp, frame-peak reference).
    let mut full = SpectrumConfig::default().settings;
    full.spectral_features = true;
    full.weighting = av_protocol::config::Weighting::A;
    full.warp_interp = av_protocol::config::WarpInterp::Cubic;
    full.warp_aggregation = av_protocol::config::WarpAggregation::Rms;
    full.db_reference = av_protocol::config::DbReference::FramePeak;
    full.output_bins = 8192;
    let configs = [
        SpectrumConfig::default(),
        SpectrumConfig::layered(6, 32_768),
        SpectrumConfig::single(2048, 2048, 512),
        SpectrumConfig::new(full, 48_000),
    ];
    for (n, cfg) in configs.into_iter().enumerate() {
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
            "allocations in 500 steady-state hops (config {n})"
        );
    }
}
