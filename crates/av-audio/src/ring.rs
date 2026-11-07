//! The real-time handoff (pre-plan 5.2): a bounded, preallocated `rtrb` ring of interleaved f32.
//!
//! The producer side ([`AudioSink`]) is what a capture callback holds. `push` never blocks,
//! locks, allocates or logs: a chunk that does not fit is dropped whole (an SPSC producer cannot
//! pop old data) and counted. The consumer ([`AudioSource`]) lives on the analysis thread; it
//! sees the gap through the shared counters and resets continuity.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use rtrb::{Consumer, Producer, RingBuffer};

/// Counters shared by both ends. Monotonic; read with `Relaxed` for diagnostics.
#[derive(Debug, Default)]
pub struct RingCounters {
    /// Interleaved samples accepted by the ring since creation.
    pub pushed_samples: AtomicU64,
    /// Interleaved samples dropped because the ring was full.
    pub dropped_samples: AtomicU64,
    /// Number of chunks dropped (overrun events).
    pub overruns: AtomicU64,
}

/// Producer end, owned by exactly one capture thread or callback.
#[derive(Debug)]
pub struct AudioSink {
    producer: Producer<f32>,
    counters: Arc<RingCounters>,
}

/// Consumer end, owned by the analysis worker.
#[derive(Debug)]
pub struct AudioSource {
    consumer: Consumer<f32>,
    counters: Arc<RingCounters>,
    channels: u16,
}

/// Creates a ring holding `seconds` of audio at `sample_rate` x `channels`.
pub fn ring(sample_rate: u32, channels: u16, seconds: f32) -> (AudioSink, AudioSource) {
    let capacity = ((sample_rate as f32 * seconds) as usize).max(1) * channels.max(1) as usize;
    let (producer, consumer) = RingBuffer::new(capacity);
    let counters = Arc::new(RingCounters::default());
    (
        AudioSink {
            producer,
            counters: counters.clone(),
        },
        AudioSource {
            consumer,
            counters,
            channels: channels.max(1),
        },
    )
}

impl AudioSink {
    /// Real-time safe. Pushes the whole chunk or drops it whole and counts the overrun.
    #[inline]
    pub fn push(&mut self, interleaved: &[f32]) -> bool {
        match self.producer.push_entire_slice(interleaved) {
            Ok(()) => {
                self.counters
                    .pushed_samples
                    .fetch_add(interleaved.len() as u64, Ordering::Relaxed);
                true
            }
            Err(_) => {
                self.counters
                    .dropped_samples
                    .fetch_add(interleaved.len() as u64, Ordering::Relaxed);
                self.counters.overruns.fetch_add(1, Ordering::Relaxed);
                false
            }
        }
    }

    pub fn counters(&self) -> &Arc<RingCounters> {
        &self.counters
    }

    /// True once the consumer has been dropped (analysis stopped).
    pub fn is_abandoned(&self) -> bool {
        self.producer.is_abandoned()
    }
}

impl AudioSource {
    /// Copies up to `out.len()` samples (rounded down to whole frames). Returns the count.
    pub fn pop(&mut self, out: &mut [f32]) -> usize {
        let ch = self.channels as usize;
        let want = (out.len() / ch).min(self.consumer.slots() / ch) * ch;
        let (done, _) = self.consumer.pop_partial_slice(&mut out[..want]);
        done.len()
    }

    /// Interleaved samples waiting.
    pub fn available(&self) -> usize {
        self.consumer.slots()
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    pub fn counters(&self) -> &Arc<RingCounters> {
        &self.counters
    }

    /// True once the producer has been dropped (capture stopped).
    pub fn is_abandoned(&self) -> bool {
        self.consumer.is_abandoned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_frames_round_trip() {
        let (mut sink, mut src) = ring(10, 2, 1.0); // 20 samples
        assert!(sink.push(&[1.0, 2.0, 3.0, 4.0]));
        let mut out = [0.0; 3];
        assert_eq!(src.pop(&mut out), 2, "rounded down to one stereo frame");
        assert_eq!(&out[..2], &[1.0, 2.0]);
    }

    #[test]
    fn overflow_drops_the_newest_chunk_whole_and_counts_it() {
        let (mut sink, mut src) = ring(4, 1, 1.0); // 4 samples
        assert!(sink.push(&[1.0, 2.0, 3.0]));
        assert!(!sink.push(&[4.0, 5.0]));
        let c = sink.counters();
        assert_eq!(c.overruns.load(Ordering::Relaxed), 1);
        assert_eq!(c.dropped_samples.load(Ordering::Relaxed), 2);
        assert_eq!(c.pushed_samples.load(Ordering::Relaxed), 3);
        let mut out = [0.0; 8];
        assert_eq!(src.pop(&mut out), 3);
        assert_eq!(&out[..3], &[1.0, 2.0, 3.0]);
    }

    #[test]
    fn dropping_one_end_is_visible_to_the_other() {
        let (sink, src) = ring(4, 1, 1.0);
        drop(sink);
        assert!(src.is_abandoned());
    }
}
