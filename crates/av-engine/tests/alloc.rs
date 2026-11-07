//! Allocation gate for the frame path: after warm-up, feeding samples through the pipeline,
//! publishing into the slot ring, reading frames back and encoding a preview never allocate.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use av_dsp::{Analyzer, PlanCache, SpectrumConfig};
use av_engine::pipeline::Pipeline;
use av_engine::preview::PreviewEncoder;
use av_engine::{FrameCopy, slot_ring};

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

#[test]
fn feed_publish_read_and_encode_never_allocate() {
    let cfg = SpectrumConfig {
        emit_bins: true,
        ..SpectrumConfig::default()
    };
    let analyzer = Analyzer::new(cfg.clone(), PlanCache::default().get(cfg.fft_size)).unwrap();
    let (mut writer, reader) = slot_ring();
    let mut pipeline = Pipeline::new(analyzer, 1, 2, 0);
    let mut copy = FrameCopy::default();
    let mut encoder = PreviewEncoder::default();
    let chunk: Vec<f32> = (0..960).map(|n| (n as f32 * 0.05).sin() * 0.3).collect(); // 10 ms stereo

    for _ in 0..50 {
        pipeline.feed(&chunk, &mut writer);
        pipeline.feed_silence(&mut writer);
    }
    let before = ALLOCS.with(Cell::get);
    let mut checksum = 0usize;
    for i in 0..500 {
        pipeline.feed(&chunk, &mut writer);
        if i % 10 == 0 {
            pipeline.feed_silence(&mut writer);
        }
        reader.read_latest(&mut copy, true).unwrap();
        checksum += encoder.encode(&reader, None, true).len();
        checksum += encoder
            .encode(&reader, reader.latest_sequence(), false)
            .len();
    }
    let after = ALLOCS.with(Cell::get);
    assert!(checksum > 0);
    assert_eq!(after - before, 0, "allocations on the frame path");
}
