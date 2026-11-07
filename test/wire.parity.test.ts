// Rust/TypeScript wire parity (Track A step A4, closes P0.5). The fixtures are generated from
// the Rust reference by `bun run fixtures`; never edit them by hand.
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import {
  decodeFrame,
  encodeFrame,
  Flags,
  hasFlag,
  timestampUs,
  WireError,
  type FrameHeader,
} from "../src/lib/frame/wire";

interface FrameCase {
  name: string;
  file: string;
  header: FrameHeader;
  bars: number[];
  bins: number[] | null;
  features: number[] | null;
  waveform: number[] | null;
}
interface ErrorCase {
  name: string;
  file: string;
  error: string;
}
type Case = FrameCase | ErrorCase;

const DIR = join(import.meta.dirname, "fixtures", "wire");
const manifest = JSON.parse(readFileSync(join(DIR, "manifest.json"), "utf8")) as {
  wireVersion: number;
  cases: Case[];
};
const bytesOf = (c: Case) => new Uint8Array(readFileSync(join(DIR, c.file)));
/** f32 bit patterns, so -0.0 and the f32 rounding of each value are compared exactly. */
const bits = (values: ArrayLike<number>) =>
  Array.from(new Uint32Array(Float32Array.from(values).buffer));

describe("wire parity with Rust fixtures", () => {
  test("the manifest is wire v2 and covers the required cases", () => {
    expect(manifest.wireVersion).toBe(2);
    const names = manifest.cases.map((c) => c.name);
    for (const required of [
      "sequence_max",
      "sequence_wrapped",
      "header_only",
      "timestamp_64bit",
      "reject_unknown_version",
      "waveform_present",
      "v1_frame_still_reads",
      "reject_waveform_in_v1",
    ]) {
      expect(names).toContain(required);
    }
  });

  for (const c of manifest.cases) {
    if ("error" in c) {
      test(`${c.name}: rejected with ${c.error}`, () => {
        let caught: unknown;
        try {
          decodeFrame(bytesOf(c));
        } catch (e) {
          caught = e;
        }
        expect(caught).toBeInstanceOf(WireError);
        expect((caught as WireError).kind).toBe(c.error as WireError["kind"]);
      });
      continue;
    }

    test(`${c.name}: decodes as Rust describes`, () => {
      const d = decodeFrame(bytesOf(c));
      expect(d.header).toEqual(c.header);
      expect(bits(d.bars)).toEqual(bits(c.bars));
      expect(d.bins === null ? null : bits(d.bins)).toEqual(c.bins === null ? null : bits(c.bins));
      expect(d.features === null ? null : bits(d.features)).toEqual(
        c.features === null ? null : bits(c.features),
      );
      expect(d.waveform === null ? null : bits(d.waveform)).toEqual(
        c.waveform === null ? null : bits(c.waveform),
      );
      expect(d.headerOnly).toBe(hasFlag(c.header, Flags.HEADER_ONLY));
    });

    // A legacy v1 frame decodes, but this build writes v2.
    if (bytesOf(c)[27] === 1) continue;

    test(`${c.name}: re-encodes byte for byte`, () => {
      const encoded = encodeFrame({
        header: c.header,
        bars: c.bars,
        bins: c.bins,
        features: c.features,
        waveform: c.waveform,
      });
      expect(Array.from(encoded)).toEqual(Array.from(bytesOf(c)));
    });
  }
});

describe("wire semantics", () => {
  test("64-bit timestamps combine lo and hi", () => {
    const c = manifest.cases.find((x) => x.name === "timestamp_64bit") as FrameCase;
    expect(timestampUs(decodeFrame(bytesOf(c)).header)).toBe(0x0001_2345_6789_abcd);
  });

  test("sequence wraps from u32::MAX to 0", () => {
    const max = manifest.cases.find((x) => x.name === "sequence_max") as FrameCase;
    const wrapped = manifest.cases.find((x) => x.name === "sequence_wrapped") as FrameCase;
    const a = decodeFrame(bytesOf(max)).header.sequence;
    const b = decodeFrame(bytesOf(wrapped)).header.sequence;
    expect(a).toBe(0xffffffff);
    expect((a + 1) >>> 0).toBe(b);
  });

  test("payload arrays are zero-copy views over the buffer", () => {
    const c = manifest.cases.find((x) => x.name === "bins_present") as FrameCase;
    const buf = bytesOf(c);
    const d = decodeFrame(buf);
    expect(d.bars.buffer).toBe(buf.buffer);
    expect(d.bins!.buffer).toBe(buf.buffer);
  });

  test("an unaligned slice still decodes (copy fallback)", () => {
    const c = manifest.cases.find((x) => x.name === "bars_basic") as FrameCase;
    const src = bytesOf(c);
    const padded = new Uint8Array(src.length + 1);
    padded.set(src, 1);
    const d = decodeFrame(padded.subarray(1));
    expect(bits(d.bars)).toEqual(bits(c.bars));
  });
});
