// Wire format v1 decoder (and a test-grade encoder) for the Studio preview.
//
// Mirrors crates/av-protocol/src/{frame,wire}.rs; byte-for-byte parity is pinned by
// test/wire.parity.test.ts against fixtures generated from Rust. Layout: 8 little-endian u32
// header words, then `barCount` f32 bars, then `fftSize / 2 + 1` f32 bins if BINS_PRESENT, then
// 8 f32 features if FEATURES_PRESENT. No padding.
//
// `decodeFrame` is zero-copy: the payload arrays are Float32Array views over the response
// buffer. Consume them in the same rAF tick; the next response replaces the buffer.

export const HEADER_WORDS = 8;
export const HEADER_BYTES = HEADER_WORDS * 4;
export const WIRE_VERSION = 1;
export const MAX_BARS = 128;
export const MAX_FFT_SIZE = 8192;
export const FEATURE_COUNT = 8;

export const Flags = {
  SILENT: 1 << 0,
  CLIPPED: 1 << 1,
  STEREO: 1 << 2,
  BINS_PRESENT: 1 << 3,
  FEATURES_PRESENT: 1 << 4,
  HEADER_ONLY: 1 << 5,
} as const;

const DEFINED_MASK = 0x3f;
const RESERVED_MASK = 0x00ffffc0;
const VERSION_SHIFT = 24;

export interface FrameHeader {
  sequence: number;
  timestampLo: number;
  timestampHi: number;
  sampleRate: number;
  fftSize: number;
  barCount: number;
  /** Flag bits 0-5; never includes the version byte. */
  flags: number;
  generation: number;
}

export interface DecodedFrame {
  header: FrameHeader;
  headerOnly: boolean;
  bars: Float32Array;
  bins: Float32Array | null;
  features: Float32Array | null;
}

export type WireErrorKind =
  | "Truncated"
  | "TrailingBytes"
  | "UnknownVersion"
  | "ReservedBitsSet"
  | "TooManyBars"
  | "InvalidFftSize";

export class WireError extends Error {
  constructor(
    readonly kind: WireErrorKind,
    detail: string,
  ) {
    super(`${kind}: ${detail}`);
    this.name = "WireError";
  }
}

/** Monotonic microseconds of the newest analysed sample. Exact below 2^53 (285 years). */
export function timestampUs(h: FrameHeader): number {
  return h.timestampHi * 0x1_0000_0000 + h.timestampLo;
}

export const hasFlag = (h: FrameHeader, flag: number): boolean => (h.flags & flag) === flag;

function binCount(h: FrameHeader): number {
  return hasFlag(h, Flags.BINS_PRESENT) ? h.fftSize / 2 + 1 : 0;
}

function featureCount(h: FrameHeader): number {
  return hasFlag(h, Flags.FEATURES_PRESENT) ? FEATURE_COUNT : 0;
}

export function encodedLength(h: FrameHeader): number {
  if (hasFlag(h, Flags.HEADER_ONLY)) return HEADER_BYTES;
  return HEADER_BYTES + (h.barCount + binCount(h) + featureCount(h)) * 4;
}

// Float32Array views use host byte order; every supported target is little-endian. A big-endian
// host would need DataView copies instead of views, so refuse loudly rather than misread.
const LITTLE_ENDIAN = new Uint8Array(new Uint32Array([1]).buffer)[0] === 1;

export function decodeHeader(view: DataView): FrameHeader {
  if (view.byteLength < HEADER_BYTES) {
    throw new WireError("Truncated", `need ${HEADER_BYTES} bytes, got ${view.byteLength}`);
  }
  const w = (i: number) => view.getUint32(i * 4, true);
  const flagsWord = w(6);
  const version = flagsWord >>> VERSION_SHIFT;
  if (version !== WIRE_VERSION) {
    throw new WireError("UnknownVersion", `version ${version}, this build reads ${WIRE_VERSION}`);
  }
  if ((flagsWord & RESERVED_MASK) !== 0) {
    throw new WireError("ReservedBitsSet", `bits ${(flagsWord & RESERVED_MASK) >>> 0}`);
  }
  const header: FrameHeader = {
    sequence: w(0),
    timestampLo: w(1),
    timestampHi: w(2),
    sampleRate: w(3),
    fftSize: w(4),
    barCount: w(5),
    flags: flagsWord & DEFINED_MASK,
    generation: w(7),
  };
  if (header.barCount > MAX_BARS) {
    throw new WireError("TooManyBars", `${header.barCount} > ${MAX_BARS}`);
  }
  if (
    hasFlag(header, Flags.BINS_PRESENT) &&
    !hasFlag(header, Flags.HEADER_ONLY) &&
    (header.fftSize === 0 || header.fftSize % 2 !== 0 || header.fftSize > MAX_FFT_SIZE)
  ) {
    throw new WireError("InvalidFftSize", `${header.fftSize}`);
  }
  return header;
}

const EMPTY = new Float32Array(0);

/** Decodes a whole frame. `bytes` must hold exactly one frame. */
export function decodeFrame(bytes: ArrayBuffer | Uint8Array): DecodedFrame {
  if (!LITTLE_ENDIAN) throw new Error("big-endian hosts are not supported by the wire decoder");
  const u8 = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
  const view = new DataView(u8.buffer, u8.byteOffset, u8.byteLength);
  const header = decodeHeader(view);
  const expected = encodedLength(header);
  if (u8.byteLength < expected) {
    throw new WireError("Truncated", `need ${expected} bytes, got ${u8.byteLength}`);
  }
  if (u8.byteLength > expected) {
    throw new WireError("TrailingBytes", `expected ${expected} bytes, got ${u8.byteLength}`);
  }
  const headerOnly = hasFlag(header, Flags.HEADER_ONLY);
  if (headerOnly) return { header, headerOnly, bars: EMPTY, bins: null, features: null };

  // A view needs a 4-byte aligned offset; IPC responses start at offset 0. Copy otherwise.
  const floats = (byteStart: number, count: number): Float32Array => {
    const offset = u8.byteOffset + byteStart;
    return offset % 4 === 0
      ? new Float32Array(u8.buffer, offset, count)
      : new Float32Array(u8.slice(byteStart, byteStart + count * 4).buffer);
  };
  const bins = binCount(header);
  const features = featureCount(header);
  const barsAt = HEADER_BYTES;
  const binsAt = barsAt + header.barCount * 4;
  const featuresAt = binsAt + bins * 4;
  return {
    header,
    headerOnly,
    bars: floats(barsAt, header.barCount),
    bins: bins ? floats(binsAt, bins) : null,
    features: features ? floats(featuresAt, features) : null,
  };
}

/**
 * Encodes a frame. Presence flags follow the arrays, as in Rust. Used by tests and by preview
 * mocks; the app itself only decodes.
 */
export function encodeFrame(frame: {
  header: FrameHeader;
  bars: ArrayLike<number>;
  bins?: ArrayLike<number> | null;
  features?: ArrayLike<number> | null;
}): Uint8Array {
  let flags = frame.header.flags & DEFINED_MASK & ~(Flags.BINS_PRESENT | Flags.FEATURES_PRESENT);
  if (frame.bins) flags |= Flags.BINS_PRESENT;
  if (frame.features) flags |= Flags.FEATURES_PRESENT;
  const header = { ...frame.header, flags };
  const out = new Uint8Array(encodedLength(header));
  const view = new DataView(out.buffer);
  const words = [
    header.sequence,
    header.timestampLo,
    header.timestampHi,
    header.sampleRate,
    header.fftSize,
    header.barCount,
    (flags | (WIRE_VERSION << VERSION_SHIFT)) >>> 0,
    header.generation,
  ];
  words.forEach((v, i) => view.setUint32(i * 4, v >>> 0, true));
  if (hasFlag(header, Flags.HEADER_ONLY)) return out;
  let at = HEADER_BYTES;
  for (const arr of [frame.bars, frame.bins, frame.features]) {
    if (!arr) continue;
    for (let i = 0; i < arr.length; i++, at += 4) view.setFloat32(at, arr[i]!, true);
  }
  return out;
}
