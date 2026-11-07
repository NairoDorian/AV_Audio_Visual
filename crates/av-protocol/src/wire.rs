//! Binary wire format v1: the 8-word header, then `bar_count` f32 bars, then
//! `fft_size / 2 + 1` f32 bins if `BINS_PRESENT`, then `FEATURE_COUNT` f32 features if
//! `FEATURES_PRESENT`. Little-endian, IEEE-754, no padding. A header-only reply is the 32-byte
//! header with `HEADER_ONLY` set.
//!
//! Neither direction allocates: `encode` writes into a caller-owned buffer (reused by the
//! preview command), `decode` borrows the payload bytes. The native renderer never uses this
//! module; it reads the slot ring in place (pre-plan section 5.4).

use crate::frame::{FrameFlags, FrameHeader, HEADER_BYTES, HEADER_WORDS, WireError};

/// The parts of a frame to encode. Slice lengths must match the header.
#[derive(Debug, Clone, Copy)]
pub struct FrameParts<'a> {
    pub header: FrameHeader,
    pub bars: &'a [f32],
    pub bins: Option<&'a [f32]>,
    pub features: Option<&'a [f32]>,
}

/// Writes a full frame into `out` and returns the number of bytes written.
///
/// `BINS_PRESENT` and `FEATURES_PRESENT` are derived from the slices, not trusted from the
/// header, so the flags can never disagree with the payload. `HEADER_ONLY` must not be set; use
/// [`encode_header_only`].
pub fn encode(parts: &FrameParts<'_>, out: &mut [u8]) -> Result<usize, WireError> {
    let mut header = parts.header;
    if header.flags.contains(FrameFlags::HEADER_ONLY) {
        return Err(WireError::PayloadMismatch(
            "HEADER_ONLY set on a full frame",
        ));
    }
    header
        .flags
        .set(FrameFlags::BINS_PRESENT, parts.bins.is_some());
    header
        .flags
        .set(FrameFlags::FEATURES_PRESENT, parts.features.is_some());
    header.validate()?;
    if parts.bars.len() != header.bar_count as usize {
        return Err(WireError::PayloadMismatch("bars length != bar_count"));
    }
    if let Some(bins) = parts.bins
        && bins.len() != header.bin_count()
    {
        return Err(WireError::PayloadMismatch(
            "bins length != fft_size / 2 + 1",
        ));
    }
    if let Some(features) = parts.features
        && features.len() != header.feature_count()
    {
        return Err(WireError::PayloadMismatch(
            "features length != FEATURE_COUNT",
        ));
    }

    let needed = header.encoded_len();
    if out.len() < needed {
        return Err(WireError::BufferTooSmall {
            needed,
            got: out.len(),
        });
    }
    write_header(&header, out);
    let mut at = HEADER_BYTES;
    let mut put = |values: &[f32]| {
        for v in values {
            out[at..at + 4].copy_from_slice(&v.to_le_bytes());
            at += 4;
        }
    };
    put(parts.bars);
    if let Some(bins) = parts.bins {
        put(bins);
    }
    if let Some(features) = parts.features {
        put(features);
    }
    debug_assert_eq!(at, needed);
    Ok(needed)
}

/// Writes the 32-byte header-only reply for `header` (the client already has this sequence).
pub fn encode_header_only(header: &FrameHeader, out: &mut [u8]) -> Result<usize, WireError> {
    if out.len() < HEADER_BYTES {
        return Err(WireError::BufferTooSmall {
            needed: HEADER_BYTES,
            got: out.len(),
        });
    }
    let mut h = *header;
    h.flags = h.flags | FrameFlags::HEADER_ONLY;
    write_header(&h, out);
    Ok(HEADER_BYTES)
}

fn write_header(header: &FrameHeader, out: &mut [u8]) {
    for (i, word) in header.to_words().iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
}

/// Reads just the header.
pub fn decode_header(bytes: &[u8]) -> Result<FrameHeader, WireError> {
    if bytes.len() < HEADER_BYTES {
        return Err(WireError::Truncated {
            needed: HEADER_BYTES,
            got: bytes.len(),
        });
    }
    let mut words = [0u32; HEADER_WORDS];
    for (i, w) in words.iter_mut().enumerate() {
        *w = u32::from_le_bytes([
            bytes[i * 4],
            bytes[i * 4 + 1],
            bytes[i * 4 + 2],
            bytes[i * 4 + 3],
        ]);
    }
    FrameHeader::from_words(words)
}

/// A decoded frame borrowing its payload.
#[derive(Debug, Clone, Copy)]
pub struct Decoded<'a> {
    pub header: FrameHeader,
    bars: &'a [u8],
    bins: &'a [u8],
    features: &'a [u8],
}

impl<'a> Decoded<'a> {
    pub fn is_header_only(&self) -> bool {
        self.header.flags.contains(FrameFlags::HEADER_ONLY)
    }
    pub fn bars(&self) -> F32Le<'a> {
        F32Le(self.bars)
    }
    pub fn bins(&self) -> Option<F32Le<'a>> {
        self.header
            .flags
            .contains(FrameFlags::BINS_PRESENT)
            .then_some(F32Le(self.bins))
            .filter(|_| !self.is_header_only())
    }
    pub fn features(&self) -> Option<F32Le<'a>> {
        self.header
            .flags
            .contains(FrameFlags::FEATURES_PRESENT)
            .then_some(F32Le(self.features))
            .filter(|_| !self.is_header_only())
    }
}

/// Little-endian f32 values over borrowed bytes.
#[derive(Debug, Clone, Copy)]
pub struct F32Le<'a>(&'a [u8]);

impl<'a> F32Le<'a> {
    pub fn len(&self) -> usize {
        self.0.len() / 4
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn get(&self, i: usize) -> Option<f32> {
        let b = self.0.get(i * 4..i * 4 + 4)?;
        Some(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    pub fn iter(&self) -> impl Iterator<Item = f32> + 'a {
        self.0
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    /// Copies into `out` (which must have `len()` elements).
    pub fn copy_to(&self, out: &mut [f32]) {
        for (dst, v) in out.iter_mut().zip(self.iter()) {
            *dst = v;
        }
    }
}

/// Decodes a whole frame. The byte length must match the header exactly.
pub fn decode(bytes: &[u8]) -> Result<Decoded<'_>, WireError> {
    let header = decode_header(bytes)?;
    let expected = header.encoded_len();
    if bytes.len() < expected {
        return Err(WireError::Truncated {
            needed: expected,
            got: bytes.len(),
        });
    }
    if bytes.len() > expected {
        return Err(WireError::TrailingBytes {
            expected,
            got: bytes.len(),
        });
    }
    let (bars, bins, features) = if header.flags.contains(FrameFlags::HEADER_ONLY) {
        (&[][..], &[][..], &[][..])
    } else {
        let bars_end = HEADER_BYTES + header.bar_count as usize * 4;
        let bins_end = bars_end + header.bin_count() * 4;
        (
            &bytes[HEADER_BYTES..bars_end],
            &bytes[bars_end..bins_end],
            &bytes[bins_end..expected],
        )
    };
    Ok(Decoded {
        header,
        bars,
        bins,
        features,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{FEATURE_COUNT, WIRE_VERSION};

    fn header(bar_count: u32) -> FrameHeader {
        FrameHeader {
            sequence: u32::MAX,
            timestamp_us: (1u64 << 40) + 5,
            sample_rate: 48_000,
            fft_size: 16,
            bar_count,
            flags: FrameFlags::SILENT,
            generation: 2,
        }
    }

    #[test]
    fn full_frame_round_trip() {
        let bars = [0.0, 0.5, 1.0, -0.25];
        let bins: Vec<f32> = (0..9).map(|i| i as f32 * 0.125).collect();
        let features = [1.5; FEATURE_COUNT];
        let parts = FrameParts {
            header: header(4),
            bars: &bars,
            bins: Some(&bins),
            features: Some(&features),
        };
        let mut buf = [0u8; 256];
        let n = encode(&parts, &mut buf).unwrap();
        assert_eq!(n, HEADER_BYTES + (4 + 9 + 8) * 4);
        let d = decode(&buf[..n]).unwrap();
        assert!(
            d.header
                .flags
                .contains(FrameFlags::BINS_PRESENT | FrameFlags::FEATURES_PRESENT)
        );
        assert_eq!(d.header.sequence, u32::MAX);
        assert_eq!(d.header.timestamp_us, (1u64 << 40) + 5);
        assert_eq!(d.bars().iter().collect::<Vec<_>>(), bars);
        assert_eq!(d.bins().unwrap().iter().collect::<Vec<_>>(), bins);
        assert_eq!(d.features().unwrap().get(7), Some(1.5));
    }

    #[test]
    fn presence_flags_follow_the_slices() {
        let mut h = header(1);
        h.flags = FrameFlags::BINS_PRESENT;
        let parts = FrameParts {
            header: h,
            bars: &[0.5],
            bins: None,
            features: None,
        };
        let mut buf = [0u8; 64];
        let n = encode(&parts, &mut buf).unwrap();
        assert_eq!(n, HEADER_BYTES + 4);
        assert!(decode(&buf[..n]).unwrap().bins().is_none());
    }

    #[test]
    fn header_only_reply() {
        let mut buf = [0u8; HEADER_BYTES];
        let n = encode_header_only(&header(48), &mut buf).unwrap();
        let d = decode(&buf[..n]).unwrap();
        assert!(d.is_header_only());
        assert_eq!(d.header.bar_count, 48);
        assert!(d.bars().is_empty());
        assert_eq!(buf[27], WIRE_VERSION);
    }

    #[test]
    fn length_errors() {
        let mut buf = [0u8; 64];
        let n = encode(
            &FrameParts {
                header: header(2),
                bars: &[1.0, 2.0],
                bins: None,
                features: None,
            },
            &mut buf,
        )
        .unwrap();
        assert!(matches!(
            decode(&buf[..n - 1]),
            Err(WireError::Truncated { .. })
        ));
        assert!(matches!(
            decode(&buf[..n + 4]),
            Err(WireError::TrailingBytes { .. })
        ));
        assert!(matches!(
            decode(&buf[..10]),
            Err(WireError::Truncated { needed: 32, .. })
        ));
    }

    #[test]
    fn encoder_rejects_mismatches() {
        let mut buf = [0u8; 16];
        let parts = FrameParts {
            header: header(2),
            bars: &[1.0],
            bins: None,
            features: None,
        };
        assert!(matches!(
            encode(&parts, &mut [0u8; 64]),
            Err(WireError::PayloadMismatch(_))
        ));
        let parts = FrameParts {
            bars: &[1.0, 2.0],
            ..parts
        };
        assert!(matches!(
            encode(&parts, &mut buf),
            Err(WireError::BufferTooSmall { .. })
        ));
    }
}
