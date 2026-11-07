//! Wire encoding for the Studio preview and external clients (pre-plan 5.4): the renderer reads
//! the slot ring in place; only this path encodes, into a buffer reused across calls.

use av_protocol::frame::{
    FEATURE_COUNT, FrameFlags, FrameHeader, HEADER_BYTES, MAX_BARS, MAX_BINS, MAX_WAVEFORM,
};
use av_protocol::wire::{self, FrameParts};

use crate::slot_ring::{FrameCopy, FrameReader};

/// Reusable encoder; one per client (the preview command owns one).
#[derive(Debug)]
pub struct PreviewEncoder {
    copy: FrameCopy,
    buf: Vec<u8>,
}

impl Default for PreviewEncoder {
    fn default() -> Self {
        Self {
            copy: FrameCopy::default(),
            buf: vec![
                0u8;
                HEADER_BYTES + (MAX_BARS + MAX_BINS + FEATURE_COUNT + 1 + MAX_WAVEFORM) * 4
            ],
        }
    }
}

impl PreviewEncoder {
    /// Encodes the newest frame, or a 32-byte header-only reply when the client already has
    /// `known` (or nothing has been published yet: then the header is all zero but valid).
    pub fn encode(&mut self, reader: &FrameReader, known: Option<u32>, want_bins: bool) -> &[u8] {
        let latest = reader.latest_sequence();
        if latest.is_none() {
            let n = wire::encode_header_only(&FrameHeader::default(), &mut self.buf)
                .expect("buffer holds a header");
            return &self.buf[..n];
        }
        let unchanged = latest == known;
        if reader
            .read_latest(&mut self.copy, want_bins && !unchanged)
            .is_err()
        {
            // Contended or empty: tell the client to keep what it has.
            let mut h = self.copy.header;
            h.flags = FrameFlags::empty();
            let n = wire::encode_header_only(&h, &mut self.buf).expect("buffer holds a header");
            return &self.buf[..n];
        }
        let n = if unchanged {
            wire::encode_header_only(&self.copy.header, &mut self.buf)
        } else {
            wire::encode(
                &FrameParts {
                    header: self.copy.header,
                    bars: self.copy.bars(),
                    bins: self.copy.bins(),
                    features: None,
                    waveform: self.copy.waveform(),
                },
                &mut self.buf,
            )
        }
        .expect("the buffer is sized for the largest frame");
        &self.buf[..n]
    }
}
