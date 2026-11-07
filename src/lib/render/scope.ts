// Raw audio scope traces (docs/super-fft.md section 4). The engine sends the newest raw samples
// already half-sine tapered (wire v2 waveform block); this module only places them.
//
// Inside the ring: one trace left to right through the centre; the horizontal mirror adds a
// time-reversed copy, the vertical mirror an amplitude-inverted copy of every trace, and
// `copies` repeats everything rotated by i * 180 / copies degrees. Traces shown =
// 1 x (2 if horizontal) x (2 if vertical) x copies. Pure geometry plus one painter; nothing
// allocates per frame.

export interface ScopeOptions {
  horizontalMirror: boolean;
  verticalMirror: boolean;
  copies: number;
}

/** Sign pairs `[x, y]` of every trace variant before rotation. */
export function scopeVariants(o: ScopeOptions): ReadonlyArray<readonly [number, number]> {
  const base: Array<readonly [number, number]> = [[1, 1]];
  if (o.horizontalMirror) base.push([-1, 1]);
  if (o.verticalMirror) {
    const n = base.length;
    for (let i = 0; i < n; i++) base.push([base[i]![0], -1]);
  }
  return base;
}

/** Rotation of copy `i` of `copies`, radians. */
export function scopeAngle(i: number, copies: number): number {
  return (i * Math.PI) / Math.max(1, copies);
}

/** Number of traces drawn. */
export function traceCount(o: ScopeOptions): number {
  return scopeVariants(o).length * Math.max(1, Math.round(o.copies));
}

const VARIANT_CACHE = new Map<number, ReadonlyArray<readonly [number, number]>>();

function variantsCached(o: ScopeOptions) {
  const key = (o.horizontalMirror ? 1 : 0) | (o.verticalMirror ? 2 : 0);
  let v = VARIANT_CACHE.get(key);
  if (!v) {
    v = scopeVariants(o);
    VARIANT_CACHE.set(key, v);
  }
  return v;
}

/**
 * Draws the traces centred on (cx, cy): each spans `2 * halfLength` pixels and a full-scale
 * sample reaches `amplitude` pixels. With `clipRadius` the traces are clipped to that circle.
 */
export function drawScope(
  ctx: CanvasRenderingContext2D,
  wave: Float32Array,
  cx: number,
  cy: number,
  halfLength: number,
  amplitude: number,
  o: ScopeOptions & { stroke: string | CanvasGradient; lineWidth: number; clipRadius?: number },
): void {
  const n = wave.length;
  if (n < 2) return;
  ctx.save();
  if (o.clipRadius !== undefined) {
    ctx.beginPath();
    ctx.arc(cx, cy, o.clipRadius, 0, Math.PI * 2);
    ctx.clip();
  }
  ctx.strokeStyle = o.stroke;
  ctx.lineWidth = o.lineWidth;
  ctx.lineJoin = "round";
  const copies = Math.max(1, Math.round(o.copies));
  const step = (2 * halfLength) / (n - 1);
  for (let i = 0; i < copies; i++) {
    const a = scopeAngle(i, copies);
    const co = Math.cos(a);
    const si = Math.sin(a);
    for (const [sx, sy] of variantsCached(o)) {
      ctx.beginPath();
      for (let j = 0; j < n; j++) {
        const x = sx * (-halfLength + j * step);
        const y = -sy * wave[j]! * amplitude;
        const px = cx + x * co - y * si;
        const py = cy + x * si + y * co;
        if (j === 0) ctx.moveTo(px, py);
        else ctx.lineTo(px, py);
      }
      ctx.stroke();
    }
  }
  ctx.restore();
}
