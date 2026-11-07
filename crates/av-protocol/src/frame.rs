//! The frame header and flags (README section 4.4, pre-plan section 5.4). Frozen for wire v1.
//!
//! | Word | Field          | Notes                                                   |
//! | ---: | :------------- | :------------------------------------------------------ |
//! | 0    | `sequence`     | Monotonic; wraps are allowed.                           |
//! | 1    | `timestamp_lo` | Low 32 bits of monotonic microseconds.                  |
//! | 2    | `timestamp_hi` | High 32 bits.                                           |
//! | 3    | `sample_rate`  | Analysis rate.                                          |
//! | 4    | `fft_size`     | Analysis FFT size.                                      |
//! | 5    | `bar_count`    | Bars in the payload.                                    |
//! | 6    | `flags`        | Bits 0-5 flags, 6-23 reserved (zero), 24-31 wire version |
//! | 7    | `generation`   | DSP/config generation that produced the frame.          |
//!
//! `timestamp_us` is the device-clock time of the **newest analysed sample**, mapped to the
//! process monotonic clock; never the encode time.

/// Words in the header.
pub const HEADER_WORDS: usize = 8;
/// Bytes in the header.
pub const HEADER_BYTES: usize = HEADER_WORDS * 4;
/// The wire version this crate writes. v2 added the waveform block (flag bit 6).
pub const WIRE_VERSION: u8 = 2;
/// The oldest version this crate still reads (v1 frames are v2 frames without a waveform).
pub const MIN_WIRE_VERSION: u8 = 1;
/// Longest waveform block (the tapered raw-audio scope, docs/super-fft.md section 4).
pub const MAX_WAVEFORM: usize = 4096;
/// Maximum bars in one frame (slot capacity, pre-plan 5.4).
pub const MAX_BARS: usize = crate::config::MAX_OUTPUT_BINS as usize;
/// Largest FFT the slot ring is sized for; bins = `MAX_FFT_SIZE / 2 + 1` = 4,097.
pub const MAX_FFT_SIZE: usize = 8192;
/// Maximum bins in one frame.
pub const MAX_BINS: usize = MAX_FFT_SIZE / 2 + 1;
/// Fixed size of the feature block. Reserved in v1: `FEATURES_PRESENT` is never set by the engine.
pub const FEATURE_COUNT: usize = 8;

/// Number of spectrum bins for an FFT size.
pub const fn bins_for_fft(fft_size: u32) -> usize {
    fft_size as usize / 2 + 1
}

/// Frame flags, bits 0-5 of word 6. The wire version lives in bits 24-31 and is not part of this
/// type; `FrameHeader::to_words` adds it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FrameFlags(u32);

impl FrameFlags {
    /// Frame is silent; bars are at the floor.
    pub const SILENT: Self = Self(1 << 0);
    /// Input clipped during this hop.
    pub const CLIPPED: Self = Self(1 << 1);
    /// Stereo features are valid.
    pub const STEREO: Self = Self(1 << 2);
    /// Spectrum bins follow the bars.
    pub const BINS_PRESENT: Self = Self(1 << 3);
    /// The fixed-size feature block follows the bins.
    pub const FEATURES_PRESENT: Self = Self(1 << 4);
    /// Reply carries no payload: the client already has this sequence.
    pub const HEADER_ONLY: Self = Self(1 << 5);
    /// (v2) A waveform block follows the features: one u32 count, then that many f32 samples.
    pub const WAVEFORM_PRESENT: Self = Self(1 << 6);

    /// Every defined flag bit.
    pub const DEFINED_MASK: u32 = 0x7F;
    /// Bits 7-23: must be zero (bit 6 too in a v1 frame).
    pub const RESERVED_MASK: u32 = 0x00FF_FF80;
    /// Bits 24-31: the wire version.
    pub const VERSION_SHIFT: u32 = 24;

    pub const fn empty() -> Self {
        Self(0)
    }

    /// Builds flags from raw bits; bits outside `DEFINED_MASK` are dropped.
    pub const fn from_bits_truncate(bits: u32) -> Self {
        Self(bits & Self::DEFINED_MASK)
    }

    pub const fn bits(self) -> u32 {
        self.0
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    pub fn set(&mut self, other: Self, on: bool) {
        *self = if on {
            self.union(other)
        } else {
            self.without(other)
        };
    }
}

impl std::ops::BitOr for FrameFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        self.union(rhs)
    }
}

/// Decoded header. `flags` never carries the version bits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FrameHeader {
    pub sequence: u32,
    pub timestamp_us: u64,
    pub sample_rate: u32,
    pub fft_size: u32,
    pub bar_count: u32,
    pub flags: FrameFlags,
    pub generation: u32,
}

/// Why a header or frame was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireError {
    /// Fewer bytes than the header or the payload the header announces.
    Truncated { needed: usize, got: usize },
    /// More bytes than the header announces.
    TrailingBytes { expected: usize, got: usize },
    /// A wire version this build cannot read.
    UnknownVersion(u8),
    /// Bits 6-23 of the flags word are not zero.
    ReservedBitsSet(u32),
    /// `bar_count` above `MAX_BARS`.
    TooManyBars(u32),
    /// A waveform block longer than `MAX_WAVEFORM`.
    WaveformTooLong(u32),
    /// Bins announced with an odd, zero or oversized `fft_size`.
    InvalidFftSize(u32),
    /// Slices handed to the encoder do not match the header.
    PayloadMismatch(&'static str),
    /// The output buffer cannot hold the encoded frame.
    BufferTooSmall { needed: usize, got: usize },
}

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Truncated { needed, got } => {
                write!(f, "frame truncated: need {needed} bytes, got {got}")
            }
            Self::TrailingBytes { expected, got } => {
                write!(
                    f,
                    "frame has trailing bytes: expected {expected}, got {got}"
                )
            }
            Self::UnknownVersion(v) => write!(
                f,
                "unknown wire version {v} (this build reads {WIRE_VERSION})"
            ),
            Self::ReservedBitsSet(bits) => write!(f, "reserved flag bits set: {bits:#010x}"),
            Self::TooManyBars(n) => write!(f, "bar_count {n} exceeds {MAX_BARS}"),
            Self::WaveformTooLong(n) => {
                write!(f, "waveform of {n} samples exceeds {MAX_WAVEFORM}")
            }
            Self::InvalidFftSize(n) => write!(f, "invalid fft_size {n} for a bins payload"),
            Self::PayloadMismatch(what) => write!(f, "payload does not match header: {what}"),
            Self::BufferTooSmall { needed, got } => {
                write!(f, "output buffer too small: need {needed} bytes, got {got}")
            }
        }
    }
}

impl std::error::Error for WireError {}

impl FrameHeader {
    /// The eight little-endian words, with the wire version in bits 24-31 of word 6.
    pub const fn to_words(&self) -> [u32; HEADER_WORDS] {
        [
            self.sequence,
            self.timestamp_us as u32,
            (self.timestamp_us >> 32) as u32,
            self.sample_rate,
            self.fft_size,
            self.bar_count,
            (self.flags.bits() & FrameFlags::DEFINED_MASK)
                | ((WIRE_VERSION as u32) << FrameFlags::VERSION_SHIFT),
            self.generation,
        ]
    }

    /// Validates the version and reserved bits and the structural limits.
    pub fn from_words(words: [u32; HEADER_WORDS]) -> Result<Self, WireError> {
        let flags_word = words[6];
        let version = (flags_word >> FrameFlags::VERSION_SHIFT) as u8;
        if !(MIN_WIRE_VERSION..=WIRE_VERSION).contains(&version) {
            return Err(WireError::UnknownVersion(version));
        }
        let reserved = if version == 1 {
            FrameFlags::RESERVED_MASK | FrameFlags::WAVEFORM_PRESENT.bits()
        } else {
            FrameFlags::RESERVED_MASK
        };
        if flags_word & reserved != 0 {
            return Err(WireError::ReservedBitsSet(flags_word & reserved));
        }
        let header = Self {
            sequence: words[0],
            timestamp_us: words[1] as u64 | ((words[2] as u64) << 32),
            sample_rate: words[3],
            fft_size: words[4],
            bar_count: words[5],
            flags: FrameFlags::from_bits_truncate(flags_word),
            generation: words[7],
        };
        header.validate()?;
        Ok(header)
    }

    /// Structural limits shared by the encoder and the decoder.
    pub fn validate(&self) -> Result<(), WireError> {
        if self.bar_count as usize > MAX_BARS {
            return Err(WireError::TooManyBars(self.bar_count));
        }
        if self.flags.contains(FrameFlags::BINS_PRESENT)
            && !self.flags.contains(FrameFlags::HEADER_ONLY)
            && (self.fft_size == 0
                || !self.fft_size.is_multiple_of(2)
                || self.fft_size as usize > MAX_FFT_SIZE)
        {
            return Err(WireError::InvalidFftSize(self.fft_size));
        }
        Ok(())
    }

    /// Bins carried by this frame (0 unless `BINS_PRESENT`).
    pub const fn bin_count(&self) -> usize {
        if self.flags.contains(FrameFlags::BINS_PRESENT) {
            bins_for_fft(self.fft_size)
        } else {
            0
        }
    }

    /// Features carried by this frame (0 unless `FEATURES_PRESENT`).
    pub const fn feature_count(&self) -> usize {
        if self.flags.contains(FrameFlags::FEATURES_PRESENT) {
            FEATURE_COUNT
        } else {
            0
        }
    }

    /// Payload floats this header announces; 0 for a header-only reply.
    pub const fn payload_floats(&self) -> usize {
        if self.flags.contains(FrameFlags::HEADER_ONLY) {
            0
        } else {
            self.bar_count as usize + self.bin_count() + self.feature_count()
        }
    }

    /// Total encoded size in bytes.
    pub const fn encoded_len(&self) -> usize {
        HEADER_BYTES + self.payload_floats() * 4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> FrameHeader {
        FrameHeader {
            sequence: 7,
            timestamp_us: 0x0000_0012_3456_789A,
            sample_rate: 48_000,
            fft_size: 2048,
            bar_count: 48,
            flags: FrameFlags::STEREO | FrameFlags::BINS_PRESENT,
            generation: 3,
        }
    }

    #[test]
    fn words_round_trip() {
        let h = sample();
        assert_eq!(FrameHeader::from_words(h.to_words()), Ok(h));
    }

    #[test]
    fn version_lives_in_the_top_byte() {
        let w = sample().to_words();
        assert_eq!(w[6] >> 24, WIRE_VERSION as u32);
        assert_eq!(w[6] & FrameFlags::RESERVED_MASK, 0);
    }

    #[test]
    fn the_64_bit_timestamp_splits_into_lo_and_hi() {
        let w = sample().to_words();
        assert_eq!(w[1], 0x3456_789A);
        assert_eq!(w[2], 0x12);
    }

    #[test]
    fn unknown_version_and_reserved_bits_are_rejected() {
        let mut w = sample().to_words();
        w[6] = (w[6] & 0x00FF_FFFF) | (3 << 24);
        assert_eq!(
            FrameHeader::from_words(w),
            Err(WireError::UnknownVersion(3))
        );
        let mut w = sample().to_words();
        w[6] |= 1 << 7;
        assert_eq!(
            FrameHeader::from_words(w),
            Err(WireError::ReservedBitsSet(1 << 7))
        );
    }

    #[test]
    fn limits_are_enforced() {
        let mut h = sample();
        h.bar_count = MAX_BARS as u32 + 1;
        assert_eq!(
            FrameHeader::from_words(h.to_words()),
            Err(WireError::TooManyBars(MAX_BARS as u32 + 1))
        );
        let mut h = sample();
        h.fft_size = 1023;
        assert_eq!(
            FrameHeader::from_words(h.to_words()),
            Err(WireError::InvalidFftSize(1023))
        );
    }

    #[test]
    fn payload_sizes() {
        let h = sample();
        assert_eq!(h.bin_count(), 1025);
        assert_eq!(h.encoded_len(), HEADER_BYTES + (48 + 1025) * 4);
        let ho = FrameHeader {
            flags: h.flags | FrameFlags::HEADER_ONLY,
            ..h
        };
        assert_eq!(ho.encoded_len(), HEADER_BYTES);
        assert_eq!(MAX_BINS, 4097);
    }

    #[test]
    fn flag_helpers() {
        let mut f = FrameFlags::empty();
        f.set(FrameFlags::SILENT, true);
        assert!(f.contains(FrameFlags::SILENT));
        f.set(FrameFlags::SILENT, false);
        assert_eq!(f, FrameFlags::empty());
        assert_eq!(FrameFlags::from_bits_truncate(0xFFFF_FFFF).bits(), 0x7F);
    }
}
