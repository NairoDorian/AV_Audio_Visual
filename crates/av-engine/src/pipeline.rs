//! The synchronous core of an analysis worker: interleaved samples in, frames into the slot
//! ring. No threads here, so it is tested deterministically; [`crate::engine`] wraps it in the
//! `av-analysis-<source>` thread.
//!
//! Timestamps use the sample-count clock: `origin + frames_consumed / rate`, the time of the
//! newest analysed sample. Device-clock anchoring (WASAPI `pu64QPCPosition`) replaces the origin
//! when live capture lands; status reports which clock is in use.

use std::time::Instant;

use av_dsp::Analyzer;
use av_protocol::frame::{FrameFlags, FrameHeader};

use crate::slot_ring::SlotWriter;

/// Aggregated per-hop timing; read into the status snapshot, never logged per hop.
#[derive(Debug, Clone, Copy, Default)]
pub struct HopStats {
    pub hops: u64,
    pub total_us: f64,
    pub max_us: f64,
    /// Largest raw input sample over the window.
    pub input_peak: f32,
}

#[derive(Debug)]
pub struct Pipeline {
    analyzer: Analyzer,
    generation: u32,
    channels: usize,
    hop: Vec<f32>,
    filled: usize,
    frames_consumed: u64,
    origin_us: u64,
    stats: HopStats,
}

impl Pipeline {
    /// `analyzer` must already be built for the source rate; `origin_us` anchors frame 0.
    pub fn new(analyzer: Analyzer, generation: u32, channels: u16, origin_us: u64) -> Self {
        let hop = vec![0.0; analyzer.config().hop_samples];
        Self {
            analyzer,
            generation,
            channels: channels.max(1) as usize,
            hop,
            filled: 0,
            frames_consumed: 0,
            origin_us,
            stats: HopStats::default(),
        }
    }

    pub fn generation(&self) -> u32 {
        self.generation
    }

    pub fn analyzer(&self) -> &Analyzer {
        &self.analyzer
    }

    pub fn stats(&self) -> HopStats {
        self.stats
    }

    pub fn reset_stats(&mut self) {
        self.stats = HopStats::default();
    }

    /// Swaps in a new analyzer at a hop boundary (display or DSP setting changed). The capture
    /// stream is untouched; a partially filled hop is dropped. Returns the old analyzer so the
    /// caller decides where it is freed. The hop buffer is reallocated only when the hop size
    /// changes, which is a configuration change, not steady state.
    pub fn swap(&mut self, analyzer: Analyzer, generation: u32) -> Analyzer {
        let hop = analyzer.config().hop_samples;
        if hop != self.hop.len() {
            self.hop = vec![0.0; hop];
        }
        self.filled = 0;
        self.generation = generation;
        std::mem::replace(&mut self.analyzer, analyzer)
    }

    /// Frames were lost upstream (ring overrun): keep the clock honest and restart the window.
    pub fn on_gap(&mut self, lost_frames: u64) {
        self.frames_consumed += lost_frames;
        self.filled = 0;
        self.analyzer.reset_continuity();
    }

    /// Downmixes interleaved samples to mono and analyses every complete hop.
    pub fn feed(&mut self, interleaved: &[f32], writer: &mut SlotWriter) -> usize {
        let ch = self.channels;
        let mut published = 0;
        let scale = 1.0 / ch as f32;
        for frame in interleaved.chunks_exact(ch) {
            self.hop[self.filled] = frame.iter().sum::<f32>() * scale;
            self.filled += 1;
            self.frames_consumed += 1;
            if self.filled == self.hop.len() {
                self.filled = 0;
                self.analyze_hop(writer);
                published += 1;
            }
        }
        published
    }

    /// Publishes one hop of silence (starved source). Silence is a frame, never a freeze.
    pub fn feed_silence(&mut self, writer: &mut SlotWriter) {
        self.hop.fill(0.0);
        self.frames_consumed += self.hop.len() as u64;
        self.filled = 0;
        self.analyze_hop(writer);
    }

    fn analyze_hop(&mut self, writer: &mut SlotWriter) {
        let cfg = self.analyzer.config();
        let rate = cfg.sample_rate;
        let dt = self.hop.len() as f32 / rate as f32;
        let timestamp_us = self.origin_us + self.frames_consumed * 1_000_000 / rate as u64;
        let fft_size = cfg.fft_size as u32;

        let started = Instant::now();
        let frame = self.analyzer.process(&self.hop, dt);
        let input_peak = frame.input_peak;
        let mut flags = FrameFlags::empty();
        flags.set(FrameFlags::SILENT, frame.silent);
        flags.set(FrameFlags::CLIPPED, frame.clipped);
        let header = FrameHeader {
            sequence: 0,
            timestamp_us,
            sample_rate: rate,
            fft_size,
            bar_count: frame.bars.len() as u32,
            flags,
            generation: self.generation,
        };
        writer.publish(&header, frame.bars, frame.bins);
        let us = started.elapsed().as_secs_f64() * 1e6;
        self.stats.hops += 1;
        self.stats.total_us += us;
        self.stats.max_us = self.stats.max_us.max(us);
        self.stats.input_peak = self.stats.input_peak.max(input_peak);
    }
}
