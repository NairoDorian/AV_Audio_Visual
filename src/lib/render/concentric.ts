// Concentric spectrum geometry (docs/super-fft.md section 3). Conceptual source: ZER0's circular
// overlay scope (Handy_V2 src/lib/overlayScope.ts, MIT; re-implemented here). Pure functions over
// preallocated arrays so the painter and the tests share one definition and nothing allocates
// per frame.
//
// From display values s[0..n) (0..1 after floor and gain):
//   C[k] = (s[j] + s[n-1-j]) / 2,  j = k mod n,  k in [0, 2n)
// a mirrored cross-sum repeated twice around the ring. Point k sits at angle
// (k + 0.5) / 2n * 2*pi with the seam at 3 o'clock; the outer branch is at radius R(1 + C),
// the inner at R(1 - C).

export interface RingParams {
  bins: number;
  floor: number;
  gain: number;
}

/**
 * Peak-pools `bars` into `out` (length = display bins), then applies floor-then-gain. A bucket's
 * value is its maximum, so fewer bins look chunkier instead of losing peaks.
 */
export function poolRing(bars: ArrayLike<number>, p: RingParams, out: Float32Array): Float32Array {
  const n = out.length;
  const bucket = bars.length / n;
  const floor = Math.min(Math.max(p.floor, 0), 0.99);
  for (let b = 0; b < n; b++) {
    const i0 = Math.floor(b * bucket);
    const i1 = Math.min(bars.length, Math.max(i0 + 1, Math.floor((b + 1) * bucket)));
    let m = 0;
    for (let i = i0; i < i1; i++) if (bars[i]! > m) m = bars[i]!;
    const v = m <= floor ? 0 : (m - floor) / (1 - floor);
    out[b] = Math.min(1, v * p.gain);
  }
  return out;
}

/** Number of points around the ring for `n` display bins. */
export const ringPoints = (n: number): number => 2 * n;

/** Mirrored cross-sum at point k (0 <= k < 2n). */
export function ringSignal(k: number, s: ArrayLike<number>): number {
  const n = s.length;
  const j = k < n ? k : k - n;
  return (s[j]! + s[n - 1 - j]!) / 2;
}

/** Angle of point k of `points`, seam straddling 3 o'clock. */
export function ringAngle(k: number, points: number): number {
  return ((k + 0.5) / points) * Math.PI * 2;
}

/** Draws the ring into a 2D context. `pooled` comes from `poolRing`. */
export function drawRing(
  ctx: CanvasRenderingContext2D,
  pooled: Float32Array,
  cx: number,
  cy: number,
  radius: number,
  style: { bars: boolean; inner: boolean; fill: string | CanvasGradient; base: string },
): void {
  const points = ringPoints(pooled.length);
  ctx.globalAlpha = 0.3;
  ctx.strokeStyle = style.base;
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.arc(cx, cy, radius, 0, Math.PI * 2);
  ctx.stroke();
  ctx.globalAlpha = 1;
  ctx.strokeStyle = style.fill;
  if (style.bars) {
    ctx.lineCap = "round";
    ctx.lineWidth = Math.max(1, (2 * Math.PI * radius) / points - 1.5);
    for (let k = 0; k < points; k++) {
      const c = ringSignal(k, pooled);
      if (c <= 0) continue;
      const th = ringAngle(k, points);
      const co = Math.cos(th);
      const si = Math.sin(th);
      const inner = style.inner ? 1 - c : 1;
      ctx.globalAlpha = 0.35 + 0.65 * c;
      ctx.beginPath();
      ctx.moveTo(cx + co * radius * inner, cy + si * radius * inner);
      ctx.lineTo(cx + co * radius * (1 + c), cy + si * radius * (1 + c));
      ctx.stroke();
    }
    ctx.globalAlpha = 1;
    return;
  }
  const loop = (sign: number) => {
    ctx.beginPath();
    for (let k = 0; k < points; k++) {
      const r = radius * (1 + sign * ringSignal(k, pooled));
      const th = ringAngle(k, points);
      const x = cx + Math.cos(th) * r;
      const y = cy + Math.sin(th) * r;
      if (k === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.closePath();
    ctx.stroke();
  };
  ctx.lineWidth = 1.5;
  ctx.lineJoin = "round";
  if (style.inner) {
    ctx.globalAlpha = 0.55;
    loop(-1);
    ctx.globalAlpha = 1;
  }
  loop(1);
}
