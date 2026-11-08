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

/** The component toggles of `Scene.ring` (docs/super-fft.md 3.2). */
export interface RingComponents {
  outerLine: boolean;
  innerLine: boolean;
  outerBars: boolean;
  innerBars: boolean;
  spanBars: boolean;
  baseCircle: boolean;
  /** Bar length as a fraction of the line distance (below 1 the lines outline the bars). */
  barScale: number;
  /** Gap between bars, in canvas pixels. */
  barSpacing: number;
}

/**
 * Radial spans of the bars at one point, in units of the perimeter radius: outer bars run
 * from 1 out to `1 + c * scale`, inner bars from 1 in to `1 - c * scale`, spanning bars from
 * `1 - c * scale` to `1 + c * scale`. Writes into `out` (pairs) and returns the pair count.
 */
export function barSpans(c: number, s: RingComponents, out: Float64Array): number {
  const d = c * s.barScale;
  let n = 0;
  const push = (a: number, b: number) => {
    out[2 * n] = a;
    out[2 * n + 1] = b;
    n++;
  };
  if (s.spanBars) push(1 - d, 1 + d);
  else {
    if (s.outerBars) push(1, 1 + d);
    if (s.innerBars) push(1 - d, 1);
  }
  return n;
}

const spans = new Float64Array(4);

/** The stroke of every component (a colour or the palette gradient). */
export interface RingStrokes {
  outerLineStroke: string | CanvasGradient;
  innerLineStroke: string | CanvasGradient;
  outerStroke: string | CanvasGradient;
  innerStroke: string | CanvasGradient;
  spanStroke: string | CanvasGradient;
  baseStroke: string | CanvasGradient;
}

/** Draws the ring into a 2D context. `pooled` comes from `poolRing`. */
export function drawRing(
  ctx: CanvasRenderingContext2D,
  pooled: Float32Array,
  cx: number,
  cy: number,
  radius: number,
  style: RingComponents & RingStrokes,
): void {
  const points = ringPoints(pooled.length);
  if (style.baseCircle) {
    ctx.globalAlpha = 0.3;
    ctx.strokeStyle = style.baseStroke;
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.arc(cx, cy, radius, 0, Math.PI * 2);
    ctx.stroke();
  }
  ctx.globalAlpha = 1;
  if (style.outerBars || style.innerBars || style.spanBars) {
    ctx.lineCap = "butt";
    ctx.lineWidth = Math.max(1, (2 * Math.PI * radius) / points - style.barSpacing);
    for (let k = 0; k < points; k++) {
      const c = ringSignal(k, pooled);
      if (c <= 0) continue;
      const th = ringAngle(k, points);
      const co = Math.cos(th);
      const si = Math.sin(th);
      ctx.globalAlpha = 0.35 + 0.65 * c;
      const n = barSpans(c, style, spans);
      for (let i = 0; i < n; i++) {
        const a = radius * spans[2 * i]!;
        const b = radius * spans[2 * i + 1]!;
        // Spanning bars, or outer (starts at the perimeter going out), or inner.
        ctx.strokeStyle = style.spanBars
          ? style.spanStroke
          : a >= 1 * radius - 1e-9 && b > radius
            ? style.outerStroke
            : style.innerStroke;
        ctx.beginPath();
        ctx.moveTo(cx + co * a, cy + si * a);
        ctx.lineTo(cx + co * b, cy + si * b);
        ctx.stroke();
      }
    }
    ctx.globalAlpha = 1;
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
  if (style.innerLine) {
    ctx.globalAlpha = 0.75;
    ctx.strokeStyle = style.innerLineStroke;
    loop(-1);
    ctx.globalAlpha = 1;
  }
  if (style.outerLine) {
    ctx.strokeStyle = style.outerLineStroke;
    loop(1);
  }
}
