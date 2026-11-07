# Super FFT: the analysis engine's final goal

Status: plan (2026-11-07). Decision record: [ADR-020](adr/020-layered-fft.md). This document
extends `pre-plan.md` section 5.3 (analyzer contract) and README section 4.5 (DSP). It covers
three things that together define the product's analysis and its signature look:

1. **Layered FFT**: several FFTs per hop with doubling windows, scaled to one level and averaged,
   so the spectrum is fast and precise at once.
2. **Full integration of the TouchDesigner FFT CHOP** (the custom `Fftcustom` CHOP studied in
   `references/studies/fft-plugin.md`) and its optimizations.
3. **The concentric (circular) spectrum view** from ZER0's overlay scope, as a scene style.

---

## 1. Layered FFT ("Super FFT")

### 1.1 The idea

A short window reacts fast but cannot separate low notes; a long window resolves 40 Hz from 45 Hz
but smears every transient. Instead of choosing, run **L layers per hop**, each with the same
settings except the number of audio samples it windows:

| Layer | Window samples | Scale before averaging |
| --: | --: | --: |
| 1 | 1,024 | 1 |
| 2 | 2,048 | 1/2 |
| 3 | 4,096 | 1/4 |
| 4 | 8,192 | 1/8 |
| 5 | 16,384 | 1/16 |
| 6 | 32,768 | 1/32 |

`L` is 1 to 6 (2-6 for the layered preset); the largest window is 32,768. Every layer:

- takes the **newest** `W_k = 1024 * 2^(k-1)` samples (all layers end at the same sample, so the
  short layer keeps the attack sharp),
- applies the same window shape (Hann by default, Kaiser from the CHOP),
- is **zero-padded to one common FFT size `N`** (`N >= W_max`, times the zero-pad factor 1, 2 or
  4), so every layer's bins land on the same frequencies and can be averaged bin by bin,
- produces a magnitude spectrum `|X_k|`, divided by `2^(k-1)`.

The layered spectrum is the average: `M = (1/L) * sum_k |X_k| / 2^(k-1)`.

### 1.2 Why the scale is exactly right

A sine of amplitude `A` gives a peak FFT magnitude of `A * sum(w_k) / 2`, and the window sum is
proportional to its length: doubling the window doubles the magnitude. Dividing layer `k` by
`2^(k-1)` is therefore window-length normalization, and every layer reports the same level for
the same tone. In the calibrated pipeline this is applied as `2 / sum(w_k)` per layer (the same
thing up to one global constant), so a full-scale sine still reads 0 dB in layered mode.

Bands need the effective noise bandwidth of the averaged response. It is computed once per
configuration, exactly: the average of the layers' normalized window magnitude responses,
squared and summed over every bin (`ENBW_eff`). With one layer this reduces to the classic
`N * sum(w^2) / sum(w)^2` (1.5 bins for Hann without padding).

### 1.3 What each part contributes

- Low bars get the long layers' resolution (32,768 at 48 kHz = 1.46 Hz bins before padding).
- Transients still move every bar at once through the 1,024 layer (21 ms window); the long
  layers fill in the sustained energy behind them.
- Zero-padding interpolates the spectrum between bins (smoother bars, more precise peaks); it
  does not add resolution on its own, which is why the long layers exist.

### 1.4 Settings

| Setting | Values | Default |
| :-- | :-- | :-- |
| `preset` | `layered` plus the classic single-window presets | `layered` |
| `layers` | 1-6 (6 reaches 32,768) | 4 (1,024-8,192) |
| `zeroPad` | 1, 2, 4 (multiplies `N` beyond the largest window) | 2 |
| base window | 1,024 (fixed by the design) | 1,024 |
| hop | 512 at 48 kHz (93.75 analyses/s) | 512 |
| window shape | Hann, Blackman-Harris, Kaiser(beta) | Hann |

### 1.5 Optimizations (measured before they become defaults)

1. **Cost budget.** `L` real FFTs of size `N` per hop. Target: p99 < 2 ms at 6 layers with
   `zeroPad = 1` (N = 32,768) on the dev machine; record every preset in `docs/performance.md`.
2. **Layer decimation** (option): layer `k` recomputes every `2^(k-1)` hops and its last
   spectrum is reused in between. Long layers change slowly; this divides their cost without
   touching the 1,024 layer's reactivity. A/B visually before enabling.
3. **Per-layer FFT size** (option): layer `k` uses `N_k = W_k * zeroPad` and is averaged per
   *bar* (bands integrate in Hz) instead of per bin. Cuts cost to about `2/L` of the common-N
   form; needs a calibration test that bars match the common-N result within 0.5 dB.
4. **Warm everything**: one plan per distinct `N` from the plan cache, one precomputed window per
   layer, one shared history ring of `W_max` samples, scratch reused. No allocation per hop
   (the allocation gate covers layered mode).
5. **No parallel fan-out on the audio path** (CHOP study section 9). If 6 layers ever miss the
   budget, decimation comes first; a second worker is the last resort.

### 1.6 Measured resolution (2026-11-07)

Dip contrast between two equal tones near 60 Hz (peak power over midpoint power, dB), FFT size
32,768 for every row, `cargo run --release -p av-dsp --example layer_resolution`:

| Separation | L=1 | L=2 | L=3 | L=4 | L=5 | L=6 |
| --: | --: | --: | --: | --: | --: | --: |
| 5.9 Hz | -0.02 | -0.05 | -0.12 | -0.09 | 0.41 | 1.75 |
| 8.8 Hz | -0.05 | -0.11 | -0.19 | 0.20 | 1.45 | 2.83 |
| 11.7 Hz | -0.08 | -0.17 | -0.13 | 0.60 | 2.27 | 3.69 |
| 17.6 Hz | -0.18 | -0.30 | 0.28 | 1.92 | 3.67 | 5.14 |
| 29.3 Hz | -0.43 | -0.09 | 1.61 | 3.84 | 5.69 | 7.21 |
| 58.6 Hz | -0.34 | 3.06 | 6.32 | 8.79 | 10.71 | 12.28 |

Every added layer sharpens the low end, as designed. Because the average gives each layer an
equal vote, the shorter layers partly fill the gap between close low tones: a single 32,768
window alone would separate them more sharply, at the cost of reactivity. A candidate for later
A/B (not a change of the design): **frequency-dependent layer weights**, letting long layers
dominate below a few hundred Hz and short layers above.

### 1.7 Gates

- A full-scale sine at a bin centre reads 0 dB +-0.5 for every `L` and `zeroPad`.
- Pink noise stays flat (+-2 dB, 100 Hz-10 kHz) in layered mode.
- Resolution: two equal tones 11.7 Hz apart near 60 Hz show more than 3 dB of dip with 6
  layers and none with the 1,024 layer alone; at 5.9 Hz, 6 layers beat 1 layer by more than
  1.5 dB (`tests/layered.rs`).
- A step onset raises the top bars within one hop at every `L` (reactivity gate).
- Zero allocations per hop; p99 within budget.

---

## 2. Integrating the FFT CHOP

The CHOP is the most complete analyzer in the references. Its source has no licence file, so it
is **re-implemented** from the study, never copied (AGENTS.md invariant 21). Status of each
feature in `av-dsp`:

| CHOP feature | Study reference | `av-dsp` |
| :-- | :-- | :-- |
| Window length separate from FFT size; zero-padding | 3, 5 | Layered analysis (this plan) |
| Hann / Kaiser(beta) / Blackman-Harris windows, coherent-gain normalization | 5 | Hann, Blackman-Harris done; Kaiser with this plan |
| Unnormalized forward FFT, explicit calibration | 5 | Done (`2 / sum(w)` power scale) |
| Warps: Log, Mel, ERB, Bark, Chroma, Melog; identity bypass; linear and cubic interpolation | 5 | Log/Linear/Mel done as band layouts; ERB, Bark, Melog, Chroma (12 pitch classes) next; cubic interpolation for narrow bars next |
| Weighting: A, C, ITU-R 468 | 5 | A/C/Z done; ITU-R 468 next |
| Pre-analysis EQ (low/high shelf, Q, wet/dry) | 4 | Planned (biquad DF2T, recomputed on change) |
| Output modes: linear, dB, dB-normalized; dB reference; floor | 4 | dB-normalized done; linear and dB reference next |
| Aggregation: peak vs average per band | 5 | Average-PSD done; peak aggregation next |
| Ballistics in ms at the real frame interval; reset | 5 | Done (time-based, `dt` clamped) |
| Key-cached rebuilds | 9 | Done: settings build a new analyzer off the hot path, swapped by generation |
| Off-thread plan building | 6 | Done (`PlanCache` on the caller's thread) |
| Allocation gate (20 warm-up + 500 hops) | 7 | Done, extended to layered mode |
| Async worker + triple buffer + dormancy handshake | 8, 9 | Worker + slot ring done; dormancy (park after 500 ms idle) in Phase 1B.6 |
| Worker priority (MMCSS "Pro Audio", no EcoQoS) | 8 | Planned in `av-platform` (Windows), behind a platform module |
| Explicit denormal flushing | 8 | Done in DC blocker and ballistics |
| `FastLog10` LUT, `rsqrt` magnitude | 5 | Only if profiling shows `log10`/`sqrt` in the budget |
| Rate/axis model, info counters | 2, 9 | Status snapshot done; axis model with the warp work |
| Golden tests with published tolerances; bench baseline | 7 | Done; layered goldens and bench with this plan |

Dropped on purpose: FFTW wisdom, the FFTW/oneMKL runtime backend table, TouchDesigner glue and
the generated parameter layout (study section 9).

---

## 3. Concentric spectrum view

Product owner's specification (2026-11-07), building on ZER0's circular scope
(`Handy_V2 src/lib/overlayScope.ts:166-213`, `src/overlay/OverlayScope.tsx:532-657`, MIT;
concept only). Drawn by the Studio preview first, then natively by the wgpu renderer (A9).

### 3.1 The signal

1. **Spectrum in.** The ring reads a high-resolution display spectrum, not just the bars: the
   layered FFT gives thousands of bins, so the ring can carry a very high number of bars
   (hundreds to a few thousand). It is produced by the CHOP-style warp (section 2: log/mel warp
   of all FFT bins to `M` output points with interpolation, `M` up to 4,096) and carried by
   wire v2 (section 5). Until then the ring uses the bars (max 128).
2. **Pool, floor, gain.** Peak-pool to the display count `n`; values under the floor draw
   nothing; the rest are rescaled and multiplied by a fixed gain.
3. **Mirror and add.** The spectrum is added to its own index-reversed copy:
   `P[j] = (s[j] + s[n-1-j]) / 2`. The result is perfectly symmetric (a palindrome).
4. **Half a circle.** `P` is spread over **half the circle**, the top half; the bottom half is
   its mirror image, so the figure is symmetric about both axes. The seams (where the two
   halves meet) are at 3 and 9 o'clock. Equivalent form used in code: `C[k] = P[k mod n]` over
   `2n` points at angles `(k + 0.5) / 2n * 2*pi`.

### 3.2 The components (each a toggle, combinable)

With base radius `R` (the ring's perimeter) and the signal `C` in 0..1:

| Component | Geometry |
| :-- | :-- |
| **Outer line** | Closed line at radius `R (1 + C)`: the FFT going outward, away from the centre. |
| **Inner line** | Closed line at radius `R (1 - C)`: the FFT going inward, towards the centre. |
| **Outer bars** | Radial bars from the perimeter `R` out to the outer line. |
| **Inner bars** | Radial bars from the perimeter `R` in to the inner line. |
| **Spanning bars** | Radial bars from the inner line to the outer line, through the perimeter. |
| **Base circle** | The perimeter itself, faint. |

- **Bar inset (gap).** Bars can be scaled slightly shorter than the lines (`barScale` < 1, for
  example 0.92 of the distance from the perimeter), leaving a small gap so the outer and inner
  lines read as the **outline** of the bars.
- **Bar count.** Up to the display spectrum's resolution; bar width follows the arc length
  (`2 pi R / 2n` minus a spacing), so thousands of bars stay legible as a fine texture.
- Style per component: colour from the palette gradient (radial), line width, bar alpha
  following the value (`0.35 + 0.65 C`, ZER0's rule) or constant.

### 3.3 Settings (`Scene.ring`)

`bins` (display count `n`), `floor`, `gain`, `radius`, and the component toggles
`outerLine`, `innerLine`, `outerBars`, `innerBars`, `spanBars`, `baseCircle`, plus
`barScale` (0.5-1.0) and `barSpacing`. The current `bars`/`inner` booleans migrate to these
(scene format v2, with a migration test).

---

## 4. Raw audio scope

The raw audio signal is the second ingredient of the concentric view, and on its own it is the
natural content for the taskbar strip (a linear scope).

### 4.1 The signal

1. **Window length.** The newest `L = sample_rate / display_fps` mono samples: 800 samples at
   48 kHz and 60 fps (one display frame of audio).
2. **Taper.** Multiply by a half-sine of the same length, `w[i] = sin(pi * i / (L - 1))` (0 -> 1
   -> 0), so the trace fades in and out at both ends instead of jumping.
3. **Gain.** A display gain (fixed, optionally auto) scales the amplitude.

The engine keeps the newest raw mono samples with every frame (wire v2 waveform block, up to
4,096 samples); the consumer takes the newest `L` for its own frame rate and applies the taper,
so a 60 Hz and a 144 Hz surface each get exactly one frame of audio.

### 4.2 Linear scope

Left to right across the surface, centred vertically: the taskbar strip's content, and an
optional layer in the bar views.

### 4.3 Inside the ring

The trace is drawn inside the inner circle (spanning its diameter, clipped to it). Layers
combine; each toggle doubles what is shown:

| Control | Effect |
| :-- | :-- |
| (base) | One trace, horizontal, left to right through the centre. |
| **Horizontal mirror** (toggle) | Adds a copy drawn right to left (time-reversed), superposed. |
| **Vertical mirror** (toggle) | Adds a copy of every trace flipped across the horizontal axis (amplitude inverted). |
| **Rotational copies** (slider, 1-8, default 1) | Duplicates everything `k` times, copy `i` rotated by `i * 180 / k` degrees: `k = 2` adds the vertical traces (bottom to top, and top to bottom when the horizontal mirror is on), `k = 3` gives copies every 60 degrees, and so on around the circle. |

Traces shown = 1 x (2 if horizontal mirror) x (2 if vertical mirror) x `k`.

---

## 5. Wire v2 (needed by sections 3 and 4)

Wire v1 carries bars, optional raw FFT bins (`fft_size / 2 + 1`, max 4,097) and a reserved
feature block. Sections 3 and 4 need two new optional blocks:

- **Display spectrum**: `M` warped points (up to 4,096) for high-count ring bars.
- **Waveform**: the newest `W` raw mono samples (up to 4,096).

Plan: wire version 2 sets flag bit 6 `EXTENDED`; a two-word extension header
(`display_count`, `waveform_count`) follows the 8-word header; payload order is bars, bins,
features, display spectrum, waveform. v2 decoders keep reading v1 frames; the slot ring grows
the two blocks (preallocated); Rust/TS parity fixtures cover every combination. The renderer
reads the slot ring in place as before.

---

## 6. Order of work

1. ADR-020 and this plan. Done.
2. `av-dsp`: layered analyzer, Kaiser window, exact `ENBW_eff`, gates, timings. Done.
3. `AppConfig`: `layers`, `zeroPad`, `layered` preset as the default; Studio controls. Done.
4. Studio preview: `concentric` built-in scene (outer/inner lines or spanning bars). Done.
5. Wire v2 + slot ring blocks: waveform and display spectrum (section 5), with parity fixtures.
6. Concentric components and bar inset (section 3.2), scene format v2 with migration.
7. Raw audio scope: taper, linear scope, ring traces with the three controls (section 4).
8. CHOP features (warps first: they also produce the display spectrum), ITU-R 468, peak
   aggregation, EQ, output modes, each with goldens.
9. Optimizations of section 1.5 behind measured A/B results.
10. wgpu renderer (A9): bars, concentric ring and scope traces natively.
