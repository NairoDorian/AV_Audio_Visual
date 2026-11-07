// Raw audio scope placement and ring components (docs/super-fft.md sections 3.2 and 4.3).
import { describe, expect, test } from "bun:test";
import { barSpans, type RingComponents } from "../src/lib/render/concentric";
import { scopeAngle, scopeVariants, traceCount } from "../src/lib/render/scope";

describe("scope traces", () => {
  test("each toggle doubles the traces and copies multiply them", () => {
    const o = { horizontalMirror: false, verticalMirror: false, copies: 1 };
    expect(traceCount(o)).toBe(1);
    expect(traceCount({ ...o, horizontalMirror: true })).toBe(2);
    expect(traceCount({ ...o, horizontalMirror: true, verticalMirror: true })).toBe(4);
    expect(traceCount({ ...o, horizontalMirror: true, verticalMirror: true, copies: 3 })).toBe(12);
  });

  test("the mirrors reverse time and invert amplitude", () => {
    const v = scopeVariants({ horizontalMirror: true, verticalMirror: true, copies: 1 });
    expect(v.map((p) => [...p])).toEqual([
      [1, 1],
      [-1, 1],
      [1, -1],
      [-1, -1],
    ]);
  });

  test("copy i is rotated by i * 180 / copies degrees", () => {
    const deg = (i: number, k: number) => (scopeAngle(i, k) * 180) / Math.PI;
    expect(deg(1, 2)).toBeCloseTo(90, 9);
    expect([0, 1, 2].map((i) => deg(i, 3))).toEqual([0, 60, 120].map((x) => expect.closeTo(x, 9)));
  });
});

describe("ring components", () => {
  const base: RingComponents = {
    outerLine: true,
    innerLine: true,
    outerBars: false,
    innerBars: false,
    spanBars: false,
    baseCircle: true,
    barScale: 0.9,
    barSpacing: 1,
  };
  const spans = (s: Partial<RingComponents>, c: number) => {
    const out = new Float64Array(4);
    const n = barSpans(c, { ...base, ...s }, out);
    return Array.from(out.slice(0, 2 * n)).map((v) => Math.round(v * 1e9) / 1e9);
  };

  test("outer and inner bars start at the perimeter, scaled short of the lines", () => {
    expect(spans({ outerBars: true }, 0.5)).toEqual([1, 1.45]);
    expect(spans({ innerBars: true }, 0.5)).toEqual([0.55, 1]);
    expect(spans({ outerBars: true, innerBars: true }, 0.5)).toEqual([1, 1.45, 0.55, 1]);
  });

  test("spanning bars run from the inner to the outer line and take precedence", () => {
    expect(spans({ spanBars: true, outerBars: true }, 0.5)).toEqual([0.55, 1.45]);
    expect(spans({}, 0.5)).toEqual([]);
  });
});
