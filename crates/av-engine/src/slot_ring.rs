//! The frame slot ring (ADR-003, blueprint 5.2): 64 preallocated slots, one writer, any number
//! of readers, no locks and no allocation after construction.
//!
//! Each slot is a seqlock: the writer makes the slot's `version` odd, writes, then makes it even
//! with `Release`; a reader loads `version` with `Acquire`, copies, fences, re-loads, and retries
//! if the version moved or was odd. The payload is stored as `AtomicU32` f32 bit patterns accessed
//! with `Relaxed`, so a racing read is a stale value the version check rejects, never undefined
//! behaviour. On x86-64 and AArch64 these are plain loads and stores.
//!
//! `latest` counts published frames; the frame sequence on the wire is that count truncated to
//! u32, so readers learn the newest sequence without touching a slot. The history (~680 ms at
//! 93.75 Hz) serves A/V offset and interpolation (ADR-016).

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering, fence};

use av_protocol::frame::{FrameFlags, FrameHeader, MAX_BARS, MAX_BINS};

pub const SLOTS: usize = 64;
/// Read attempts before a reader gives up and keeps its previous frame.
const MAX_READ_ATTEMPTS: usize = 64;

struct Slot {
    version: AtomicU32,
    sequence: AtomicU32,
    timestamp_us: AtomicU64,
    sample_rate: AtomicU32,
    fft_size: AtomicU32,
    bar_count: AtomicU32,
    flags: AtomicU32,
    generation: AtomicU32,
    bin_count: AtomicU32,
    bars: Box<[AtomicU32]>,
    bins: Box<[AtomicU32]>,
}

impl Slot {
    fn new() -> Self {
        let zeros = |n: usize| (0..n).map(|_| AtomicU32::new(0)).collect::<Box<[_]>>();
        Self {
            version: AtomicU32::new(0),
            sequence: AtomicU32::new(0),
            timestamp_us: AtomicU64::new(0),
            sample_rate: AtomicU32::new(0),
            fft_size: AtomicU32::new(0),
            bar_count: AtomicU32::new(0),
            flags: AtomicU32::new(0),
            generation: AtomicU32::new(0),
            bin_count: AtomicU32::new(0),
            bars: zeros(MAX_BARS),
            bins: zeros(MAX_BINS),
        }
    }

    /// Reads the header fields; the caller validates them with the version check.
    fn header_relaxed(&self) -> FrameHeader {
        FrameHeader {
            sequence: self.sequence.load(Ordering::Relaxed),
            timestamp_us: self.timestamp_us.load(Ordering::Relaxed),
            sample_rate: self.sample_rate.load(Ordering::Relaxed),
            fft_size: self.fft_size.load(Ordering::Relaxed),
            bar_count: self.bar_count.load(Ordering::Relaxed),
            flags: FrameFlags::from_bits_truncate(self.flags.load(Ordering::Relaxed)),
            generation: self.generation.load(Ordering::Relaxed),
        }
    }
}

struct Ring {
    slots: Box<[Slot]>,
    /// Frames published so far; 0 = none yet.
    published: AtomicU64,
}

/// An owned, preallocated copy of one frame. Reuse it across reads.
#[derive(Debug, Clone)]
pub struct FrameCopy {
    pub header: FrameHeader,
    bars: Box<[f32]>,
    bins: Box<[f32]>,
    bin_count: usize,
}

impl Default for FrameCopy {
    fn default() -> Self {
        Self {
            header: FrameHeader::default(),
            bars: vec![0.0; MAX_BARS].into_boxed_slice(),
            bins: vec![0.0; MAX_BINS].into_boxed_slice(),
            bin_count: 0,
        }
    }
}

impl FrameCopy {
    pub fn bars(&self) -> &[f32] {
        &self.bars[..self.header.bar_count as usize]
    }
    /// Bins, if the frame carried them and the read asked for them.
    pub fn bins(&self) -> Option<&[f32]> {
        (self.bin_count > 0).then(|| &self.bins[..self.bin_count])
    }
}

/// The single writer. Not `Clone`: one analysis worker owns it.
pub struct SlotWriter {
    ring: Arc<Ring>,
}

impl std::fmt::Debug for SlotWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SlotWriter").finish_non_exhaustive()
    }
}

/// A reader handle; cheap to clone, one per consumer (renderer, preview command).
#[derive(Clone)]
pub struct FrameReader {
    ring: Arc<Ring>,
}

impl std::fmt::Debug for FrameReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameReader")
            .field("latest", &self.latest_sequence())
            .finish()
    }
}

/// Allocates the ring once (~1.1 MB) and returns its writer and a reader.
pub fn slot_ring() -> (SlotWriter, FrameReader) {
    slot_ring_from(0)
}

/// As [`slot_ring`] but with the publish counter preset (tests use it to cross the u32 wrap).
pub fn slot_ring_from(published: u64) -> (SlotWriter, FrameReader) {
    let ring = Arc::new(Ring {
        slots: (0..SLOTS).map(|_| Slot::new()).collect(),
        published: AtomicU64::new(published),
    });
    (SlotWriter { ring: ring.clone() }, FrameReader { ring })
}

impl SlotWriter {
    /// Publishes one frame and returns its sequence. `header.sequence` is ignored and assigned;
    /// `BINS_PRESENT` follows `bins`. Never blocks or allocates. Slices longer than the slot
    /// capacity are truncated.
    pub fn publish(&mut self, header: &FrameHeader, bars: &[f32], bins: Option<&[f32]>) -> u32 {
        let count = self.ring.published.load(Ordering::Relaxed) + 1;
        let sequence = count as u32;
        let slot = &self.ring.slots[(count % SLOTS as u64) as usize];

        let v = slot.version.load(Ordering::Relaxed);
        slot.version.store(v.wrapping_add(1), Ordering::Relaxed);
        fence(Ordering::Release);

        let bars = &bars[..bars.len().min(MAX_BARS)];
        let bins = bins.map(|b| &b[..b.len().min(MAX_BINS)]);
        let mut flags = header.flags;
        flags.set(FrameFlags::BINS_PRESENT, bins.is_some());
        flags.set(FrameFlags::HEADER_ONLY, false);
        slot.sequence.store(sequence, Ordering::Relaxed);
        slot.timestamp_us
            .store(header.timestamp_us, Ordering::Relaxed);
        slot.sample_rate
            .store(header.sample_rate, Ordering::Relaxed);
        slot.fft_size.store(header.fft_size, Ordering::Relaxed);
        slot.bar_count.store(bars.len() as u32, Ordering::Relaxed);
        slot.flags.store(flags.bits(), Ordering::Relaxed);
        slot.generation.store(header.generation, Ordering::Relaxed);
        for (dst, &v) in slot.bars.iter().zip(bars) {
            dst.store(v.to_bits(), Ordering::Relaxed);
        }
        let bin_count = bins.map_or(0, |b| {
            for (dst, &v) in slot.bins.iter().zip(b) {
                dst.store(v.to_bits(), Ordering::Relaxed);
            }
            b.len()
        });
        slot.bin_count.store(bin_count as u32, Ordering::Relaxed);

        slot.version.store(v.wrapping_add(2), Ordering::Release);
        self.ring.published.store(count, Ordering::Release);
        sequence
    }

    pub fn reader(&self) -> FrameReader {
        FrameReader {
            ring: self.ring.clone(),
        }
    }
}

/// Why a read produced nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadError {
    /// Nothing published yet.
    Empty,
    /// The writer kept overwriting the slot; keep the previous frame.
    Contended,
    /// No frame in the history is old enough for the requested time.
    TooOld,
}

impl FrameReader {
    /// Sequence of the newest frame, without touching a slot.
    pub fn latest_sequence(&self) -> Option<u32> {
        match self.ring.published.load(Ordering::Acquire) {
            0 => None,
            n => Some(n as u32),
        }
    }

    /// Copies the newest frame. `want_bins = false` skips the (up to 16 KB) bin copy.
    pub fn read_latest(&self, out: &mut FrameCopy, want_bins: bool) -> Result<(), ReadError> {
        let count = self.ring.published.load(Ordering::Acquire);
        if count == 0 {
            return Err(ReadError::Empty);
        }
        self.read_slot((count % SLOTS as u64) as usize, out, want_bins)
    }

    /// Copies the newest frame whose timestamp is at or before `target_us` (A/V offset,
    /// ADR-016). Searches back through the history.
    pub fn read_at_or_before(
        &self,
        target_us: u64,
        out: &mut FrameCopy,
        want_bins: bool,
    ) -> Result<(), ReadError> {
        let count = self.ring.published.load(Ordering::Acquire);
        if count == 0 {
            return Err(ReadError::Empty);
        }
        let depth = count.min(SLOTS as u64 - 1);
        for back in 0..depth {
            let idx = ((count - back) % SLOTS as u64) as usize;
            let Some(ts) = self.timestamp_of(idx) else {
                continue;
            };
            if ts <= target_us {
                return self.read_slot(idx, out, want_bins);
            }
        }
        Err(ReadError::TooOld)
    }

    fn timestamp_of(&self, idx: usize) -> Option<u64> {
        let slot = &self.ring.slots[idx];
        for _ in 0..MAX_READ_ATTEMPTS {
            let v1 = slot.version.load(Ordering::Acquire);
            if v1 & 1 == 1 {
                std::hint::spin_loop();
                continue;
            }
            let ts = slot.timestamp_us.load(Ordering::Relaxed);
            fence(Ordering::Acquire);
            if slot.version.load(Ordering::Relaxed) == v1 {
                return Some(ts);
            }
        }
        None
    }

    fn read_slot(&self, idx: usize, out: &mut FrameCopy, want_bins: bool) -> Result<(), ReadError> {
        let slot = &self.ring.slots[idx];
        for _ in 0..MAX_READ_ATTEMPTS {
            let v1 = slot.version.load(Ordering::Acquire);
            if v1 & 1 == 1 {
                std::hint::spin_loop();
                continue;
            }
            let header = slot.header_relaxed();
            let bars = (header.bar_count as usize).min(MAX_BARS);
            for (dst, src) in out.bars[..bars].iter_mut().zip(slot.bars.iter()) {
                *dst = f32::from_bits(src.load(Ordering::Relaxed));
            }
            let stored_bins = (slot.bin_count.load(Ordering::Relaxed) as usize).min(MAX_BINS);
            let bins = if want_bins { stored_bins } else { 0 };
            for (dst, src) in out.bins[..bins].iter_mut().zip(slot.bins.iter()) {
                *dst = f32::from_bits(src.load(Ordering::Relaxed));
            }
            fence(Ordering::Acquire);
            if slot.version.load(Ordering::Relaxed) == v1 {
                out.header = header;
                out.header.bar_count = bars as u32;
                if !want_bins {
                    out.header.flags.set(FrameFlags::BINS_PRESENT, false);
                }
                out.bin_count = bins;
                return Ok(());
            }
        }
        Err(ReadError::Contended)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(ts: u64) -> FrameHeader {
        FrameHeader {
            timestamp_us: ts,
            sample_rate: 48_000,
            fft_size: 2048,
            ..FrameHeader::default()
        }
    }

    #[test]
    fn empty_ring_reports_empty() {
        let (_w, r) = slot_ring();
        let mut f = FrameCopy::default();
        assert_eq!(r.latest_sequence(), None);
        assert_eq!(r.read_latest(&mut f, false), Err(ReadError::Empty));
    }

    #[test]
    fn publish_then_read_latest() {
        let (mut w, r) = slot_ring();
        w.publish(&header(10), &[0.1, 0.2], None);
        let seq = w.publish(&header(20), &[0.3, 0.4, 0.5], Some(&[1.0; 9]));
        assert_eq!(seq, 2);
        assert_eq!(r.latest_sequence(), Some(2));
        let mut f = FrameCopy::default();
        r.read_latest(&mut f, true).unwrap();
        assert_eq!(f.header.sequence, 2);
        assert_eq!(f.header.timestamp_us, 20);
        assert_eq!(f.bars(), &[0.3, 0.4, 0.5]);
        assert_eq!(f.bins().unwrap().len(), 9);
        assert!(f.header.flags.contains(FrameFlags::BINS_PRESENT));
        r.read_latest(&mut f, false).unwrap();
        assert!(f.bins().is_none());
        assert!(!f.header.flags.contains(FrameFlags::BINS_PRESENT));
    }

    #[test]
    fn sequences_wrap_through_u32_max() {
        let (mut w, r) = slot_ring_from(u32::MAX as u64 - 1);
        assert_eq!(w.publish(&header(1), &[0.0], None), u32::MAX);
        assert_eq!(w.publish(&header(2), &[0.0], None), 0);
        assert_eq!(r.latest_sequence(), Some(0));
        let mut f = FrameCopy::default();
        r.read_latest(&mut f, false).unwrap();
        assert_eq!(f.header.sequence, 0);
    }

    #[test]
    fn history_lookup_picks_the_newest_frame_not_after_the_target() {
        let (mut w, r) = slot_ring();
        for i in 1..=100u64 {
            w.publish(&header(i * 1000), &[i as f32], None);
        }
        let mut f = FrameCopy::default();
        r.read_at_or_before(95_500, &mut f, false).unwrap();
        assert_eq!(f.header.timestamp_us, 95_000);
        assert_eq!(f.bars(), &[95.0]);
        // Only the last 63 frames are kept.
        assert_eq!(
            r.read_at_or_before(10_000, &mut f, false),
            Err(ReadError::TooOld)
        );
    }

    /// Torn-read detector: every bar of a frame equals its own sequence, so any mix of two
    /// frames fails the check. Readers race a writer publishing as fast as it can.
    #[test]
    fn concurrent_reads_are_never_torn() {
        let (mut w, r) = slot_ring();
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let readers: Vec<_> = (0..3)
            .map(|_| {
                let r = r.clone();
                let stop = stop.clone();
                std::thread::spawn(move || {
                    let mut f = FrameCopy::default();
                    let mut ok = 0u64;
                    while !stop.load(Ordering::Relaxed) {
                        if r.read_latest(&mut f, true).is_ok() {
                            let s = f.header.sequence as f32;
                            assert!(f.bars().iter().all(|&b| b == s), "torn bars");
                            assert!(f.bins().unwrap().iter().all(|&b| b == s), "torn bins");
                            assert_eq!(f.header.timestamp_us, f.header.sequence as u64);
                            ok += 1;
                        }
                    }
                    ok
                })
            })
            .collect();
        let mut bars = [0.0f32; MAX_BARS];
        let mut bins = vec![0.0f32; 1025];
        for count in 1..=50_000u64 {
            let s = count as u32 as f32;
            bars.fill(s);
            bins.fill(s);
            w.publish(&header(count), &bars, Some(&bins));
        }
        stop.store(true, Ordering::Relaxed);
        let reads: u64 = readers.into_iter().map(|t| t.join().unwrap()).sum();
        assert!(reads > 0);
    }
}
