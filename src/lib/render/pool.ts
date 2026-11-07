// Peak pooling of the analysis output bins onto a display count. The analysis can emit up to
// 65,537 bins; each scene draws its own count, and a pooled bucket shows its loudest bin so a
// narrow partial never disappears when the display is coarser than the spectrum.

/** Fills `out` with the per-bucket maximum of `src`; with more display slots than bins each
 * slot repeats its nearest bin. Allocation-free. */
export function poolPeak(src: ArrayLike<number>, out: Float32Array): Float32Array {
  const n = out.length;
  const len = src.length;
  if (len === 0) return out.fill(0);
  const bucket = len / n;
  for (let b = 0; b < n; b++) {
    const i0 = Math.min(len - 1, Math.floor(b * bucket));
    const i1 = Math.min(len, Math.max(i0 + 1, Math.floor((b + 1) * bucket)));
    let m = 0;
    for (let i = i0; i < i1; i++) if (src[i]! > m) m = src[i]!;
    out[b] = m;
  }
  return out;
}
