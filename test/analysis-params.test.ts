// The Studio's analysis parameter table against the Rust reference: every numeric row must use
// the exact hard range Rust clamps to, every select must offer only values Rust accepts, and the
// resolver must agree with av-dsp's `SpectrumConfig::new` on the default.
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import {
  PAGES,
  fromSlider,
  patchFor,
  readParam,
  resolve,
  roundTo,
  toSlider,
  type NumberParam,
} from "../src/features/analysis/params";
import type { AnalysisConfig } from "../src/lib/ipc";
import { poolPeak } from "../src/lib/render/pool";
import { ANALYSIS_HELP } from "../src/features/analysis/help";

const DIR = join(import.meta.dirname, "fixtures", "settings");
const read = (name: string) => JSON.parse(readFileSync(join(DIR, name), "utf8"));
const defaults = read("config.defaults.json").analysis as AnalysisConfig;
const limits = new Map(
  (read("config.limits.json") as Array<{ path: string; min: number; max: number }>).map((r) => [
    r.path,
    r,
  ]),
);
const all = PAGES.flatMap((p) => p.params);

describe("analysis parameter table", () => {
  test("every field of the Rust default appears exactly once", () => {
    const keys = all.map((p) => p.key);
    expect(new Set(keys).size).toBe(keys.length);
    const leaves = (o: Record<string, unknown>, prefix = ""): string[] =>
      Object.entries(o).flatMap(([k, v]) =>
        v && typeof v === "object"
          ? leaves(v as Record<string, unknown>, `${prefix}${k}.`)
          : [prefix + k],
      );
    expect(keys.toSorted()).toEqual(
      leaves(defaults as unknown as Record<string, unknown>).toSorted(),
    );
  });

  test("numeric rows use the Rust hard range", () => {
    for (const p of all.filter((q): q is NumberParam => q.kind === "number")) {
      const row = limits.get(`analysis.${p.key}`);
      expect(row, p.key).toBeDefined();
      expect([p.min, p.max], p.key).toEqual([row!.min, row!.max]);
      const lo = p.sliderMin ?? p.min;
      const hi = p.sliderMax ?? p.max;
      expect(lo >= p.min && hi <= p.max && lo < hi, p.key).toBe(true);
    }
  });

  test("select rows offer the default value", () => {
    for (const p of all) {
      if (p.kind !== "select") continue;
      const v = readParam(defaults, p.key);
      expect(
        p.options.some((o) => o.value === v),
        p.key,
      ).toBe(true);
    }
  });

  test("sliders round-trip and clamp", () => {
    const bins = all.find((p) => p.key === "outputBins") as NumberParam;
    expect(fromSlider(bins, toSlider(bins, 1024))).toBeCloseTo(1024, -1);
    expect(fromSlider(bins, 0)).toBe(16);
    expect(roundTo(bins, 1e9)).toBe(65_537);
    expect(roundTo(bins, 12.4)).toBe(12);
  });

  test("nested keys patch nested objects", () => {
    expect(patchFor("eq.q", 2)).toEqual({ eq: { q: 2 } });
    expect(patchFor("layers", 3)).toEqual({ layers: 3 });
    expect(readParam(defaults, "eq.highGainDb")).toBe(24);
  });
});

describe("resolver", () => {
  test("the default is 4 layers of 1,024..8,192 at FFT 32,768", () => {
    const r = resolve(defaults, 48_000);
    expect(r).toMatchObject({
      windowSamples: 1024,
      layers: 4,
      largestWindow: 8192,
      fftSize: 32_768,
      outputBins: 1024,
    });
    expect(r.framesPerSecond).toBeCloseTo(93.75, 2);
  });

  test("milliseconds, layer reduction and no zero-padding match av-dsp", () => {
    const ms = { ...defaults, windowLengthMode: "milliseconds" as const, windowMs: 21.333 };
    expect(resolve(ms, 44_100).windowSamples).toBe(941);
    const big = { ...defaults, windowSamples: 30_000, layers: 6 };
    expect(resolve(big, 48_000).layers).toBe(2);
    const odd = { ...defaults, zeroPadding: false, windowSamples: 3175, layers: 2 };
    expect(resolve(odd, 48_000).fftSize).toBe(6350);
    const raw = { ...defaults, rawBins: true };
    expect(resolve(raw, 48_000).outputBins).toBe(16_385);
  });
});

test("peak pooling keeps narrow partials and repeats when stretching", () => {
  const src = new Float32Array(1000);
  src[777] = 0.9;
  const out = poolPeak(src, new Float32Array(10));
  expect(out[7]).toBeCloseTo(0.9, 6);
  expect(Math.max(...out)).toBeCloseTo(0.9, 6);
  const wide = poolPeak(Float32Array.of(0.1, 0.5), new Float32Array(4));
  expect([...wide]).toEqual([...Float32Array.of(0.1, 0.1, 0.5, 0.5)]);
});

test("every setting has an info bubble and every bubble names a setting", () => {
  const keys = new Set(all.map((p) => p.key));
  for (const k of keys) {
    const h = ANALYSIS_HELP[k];
    expect(h, k).toBeDefined();
    expect(h!.text.length, k).toBeGreaterThan(20);
  }
  for (const k of Object.keys(ANALYSIS_HELP)) expect(keys.has(k), k).toBe(true);
});
