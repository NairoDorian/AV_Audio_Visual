// Concentric geometry (docs/super-fft.md section 3): pooling keeps peaks, the floor gates before
// the gain, the cross-sum is a mirror-symmetric palindrome repeated twice, the seam sits at
// 3 o'clock.
import { describe, expect, test } from "bun:test";
import { poolRing, ringAngle, ringPoints, ringSignal } from "../src/lib/render/concentric";

describe("concentric ring", () => {
  test("pooling keeps the peak of each bucket", () => {
    const out = poolRing(
      [0.1, 0.9, 0.2, 0.3, 0.5, 0.4],
      { bins: 3, floor: 0, gain: 1 },
      new Float32Array(3),
    );
    [0.9, 0.3, 0.5].forEach((v, i) => expect(out[i]).toBeCloseTo(v, 6));
  });

  test("the floor gates before the gain and the gain clamps at 1", () => {
    const out = poolRing([0.1, 0.5, 1.0], { bins: 3, floor: 0.2, gain: 2 }, new Float32Array(3));
    expect(out[0]).toBe(0);
    expect(out[1]).toBeCloseTo(((0.5 - 0.2) / 0.8) * 2, 6);
    expect(out[2]).toBe(1);
  });

  test("more display bins than bars repeats bars rather than inventing values", () => {
    const out = poolRing([0.2, 0.8], { bins: 4, floor: 0, gain: 1 }, new Float32Array(4));
    expect(Array.from(out).map((v) => Math.round(v * 10) / 10)).toEqual([0.2, 0.2, 0.8, 0.8]);
  });

  test("the cross-sum is mirror-symmetric and repeats twice", () => {
    const s = Float32Array.from([0.1, 0.4, 0.9, 0.3, 0.0]);
    const points = ringPoints(s.length);
    expect(points).toBe(10);
    for (let k = 0; k < points; k++) {
      expect(ringSignal(k, s)).toBeCloseTo(ringSignal(points - 1 - k, s), 6);
      expect(ringSignal(k, s)).toBeCloseTo(ringSignal((k + s.length) % points, s), 6);
    }
    expect(ringSignal(0, s)).toBeCloseTo((0.1 + 0.0) / 2, 6);
  });

  test("the seam straddles 3 o'clock", () => {
    const points = 8;
    expect(ringAngle(0, points)).toBeCloseTo(Math.PI / 8, 9);
    expect(ringAngle(points - 1, points)).toBeCloseTo(2 * Math.PI - Math.PI / 8, 9);
  });
});
