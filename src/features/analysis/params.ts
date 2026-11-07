// Every analysis parameter, grouped by FFT CHOP page, as data. The panel renders this table; the
// parity test checks every numeric row against the Rust limit table (test/fixtures/settings), so
// a range typed here can never disagree with what Rust accepts.
//
// `min`/`max` are the hard limits (typed values reach them); `sliderMin`/`sliderMax` the
// comfortable slider span, as the CHOP's sliders are narrower than its clamps. `when` greys a
// row out when it has no effect, as the CHOP disables inactive parameters.
import type { AnalysisConfig } from "../../lib/ipc";

export type PageId = "spectrum" | "window" | "eq" | "loudness" | "performance";

interface Base {
  /** Path under `analysis` ("eq.q" for nested). */
  key: string;
  label: string;
  help?: string;
  when?: (a: AnalysisConfig) => boolean;
}

export interface NumberParam extends Base {
  kind: "number";
  min: number;
  max: number;
  sliderMin?: number;
  sliderMax?: number;
  step: number;
  /** Logarithmic slider (frequencies, sizes, times). */
  log?: boolean;
  unit?: string;
  integer?: boolean;
}

export interface SelectParam extends Base {
  kind: "select";
  options: ReadonlyArray<{ value: string | number; label: string }>;
  numeric?: boolean;
}

export interface ToggleParam extends Base {
  kind: "toggle";
}

export type Param = NumberParam | SelectParam | ToggleParam;

const notRaw = (a: AnalysisConfig) => !a.rawBins;
const isKaiser = (a: AnalysisConfig) => a.window === "kaiser";

export const PAGES: ReadonlyArray<{ id: PageId; label: string; params: ReadonlyArray<Param> }> = [
  {
    id: "spectrum",
    label: "Spectrum",
    params: [
      {
        kind: "select",
        key: "channelMode",
        label: "Channels",
        options: [
          { value: "monoMix", label: "Mono mix (average of all channels)" },
          { value: "left", label: "First channel (left)" },
          { value: "right", label: "Second channel (right)" },
        ],
      },
      {
        kind: "toggle",
        key: "rawBins",
        label: "Raw RFFT bins (N/2+1, no warp or interpolation)",
      },
      {
        kind: "select",
        key: "scale",
        label: "Scale",
        when: notRaw,
        options: [
          { value: "log", label: "Logarithmic" },
          { value: "mel", label: "Mel" },
          { value: "erb", label: "ERB" },
          { value: "bark", label: "Bark" },
          { value: "chroma", label: "Chroma / pitch" },
          { value: "linear", label: "Linear" },
          { value: "melog", label: "Mel + Log blend" },
        ],
      },
      {
        kind: "number",
        key: "displayMaxHz",
        label: "Display max",
        unit: "Hz",
        min: 100,
        max: 192_000,
        sliderMin: 1000,
        sliderMax: 24_000,
        step: 1,
        log: true,
        when: notRaw,
        help: "Top of the axis, clamped to Nyquist.",
      },
      {
        kind: "select",
        key: "outputBinsMode",
        label: "Output bins mode",
        when: notRaw,
        options: [
          { value: "fixed", label: "Fixed (Output bins)" },
          { value: "auto", label: "Auto (N/2+1 of the FFT)" },
        ],
      },
      {
        kind: "number",
        key: "outputBins",
        label: "Output bins",
        min: 8,
        max: 65_537,
        sliderMin: 16,
        sliderMax: 16_384,
        step: 1,
        log: true,
        integer: true,
        when: (a) => notRaw(a) && a.outputBinsMode === "fixed",
      },
      {
        kind: "number",
        key: "warpBlend",
        label: "Warp blend",
        min: 0,
        max: 1,
        step: 0.001,
        when: notRaw,
        help: "0 = linear axis, 1 = the scale's own axis.",
      },
      {
        kind: "select",
        key: "warpInterp",
        label: "Warp interpolation",
        when: notRaw,
        options: [
          { value: "linear", label: "Linear (2 taps)" },
          { value: "cubic", label: "Cubic Catmull-Rom (4 taps)" },
        ],
      },
      {
        kind: "select",
        key: "warpAggregation",
        label: "Warp aggregation",
        when: notRaw,
        options: [
          { value: "off", label: "Off (interpolate, may skip peaks)" },
          { value: "peak", label: "Peak (never drops a peak)" },
          { value: "rms", label: "RMS (power mean)" },
        ],
      },
      {
        kind: "number",
        key: "logFloorHz",
        label: "Log floor",
        unit: "Hz",
        min: 1,
        max: 5000,
        sliderMin: 1,
        sliderMax: 500,
        step: 0.1,
        log: true,
        when: (a) => notRaw(a) && (a.scale === "log" || a.scale === "melog"),
      },
      {
        kind: "select",
        key: "windowLengthMode",
        label: "Window length mode",
        options: [
          { value: "samples", label: "Samples" },
          { value: "milliseconds", label: "Milliseconds" },
        ],
      },
      {
        kind: "number",
        key: "windowSamples",
        label: "Window sampling",
        unit: "samples",
        min: 16,
        max: 65_536,
        sliderMin: 64,
        sliderMax: 32_768,
        step: 1,
        log: true,
        integer: true,
        when: (a) => a.windowLengthMode === "samples",
        help: "Window of the first (shortest) layer.",
      },
      {
        kind: "number",
        key: "windowMs",
        label: "Window length",
        unit: "ms",
        min: 0.5,
        max: 5000,
        sliderMin: 1,
        sliderMax: 1000,
        step: 0.01,
        log: true,
        when: (a) => a.windowLengthMode === "milliseconds",
      },
      {
        kind: "number",
        key: "layers",
        label: "Super FFT layers",
        min: 1,
        max: 6,
        step: 1,
        integer: true,
        help: "Each layer doubles the window; all share the FFT size and are averaged.",
      },
      {
        kind: "number",
        key: "hopSamples",
        label: "Hop",
        unit: "samples",
        min: 32,
        max: 16_384,
        sliderMin: 64,
        sliderMax: 4096,
        step: 1,
        log: true,
        integer: true,
        help: "Samples between analyses: the frame rate is rate / hop.",
      },
      {
        kind: "number",
        key: "waveformSamples",
        label: "Raw audio scope",
        unit: "samples",
        min: 0,
        max: 4096,
        step: 1,
        integer: true,
        help: "Newest samples sent with every frame, tapered 0 -> 1 -> 0 (800 = one 60 fps frame at 48 kHz; 0 = off).",
      },
      { kind: "toggle", key: "zeroPadding", label: "Zero-padding" },
      {
        kind: "select",
        key: "fftSize",
        label: "Zero-pad length (FFT size)",
        numeric: true,
        when: (a) => a.zeroPadding,
        options: [1024, 2048, 4096, 8192, 16_384, 32_768, 65_536, 131_072].map((n) => ({
          value: n,
          label: `${n.toLocaleString("en-US")} (${n >= 1024 ? `${n / 1024}K` : n} bins)`,
        })),
      },
    ],
  },
  {
    id: "eq",
    label: "EQ",
    params: [
      { kind: "toggle", key: "eq.enabled", label: "EQ enable" },
      { kind: "toggle", key: "eq.highShelf", label: "High shelf", when: (a) => a.eq.enabled },
      {
        kind: "number",
        key: "eq.highGainDb",
        label: "High boost",
        unit: "dB",
        min: -24,
        max: 36,
        sliderMin: -24,
        sliderMax: 24,
        step: 0.1,
        when: (a) => a.eq.enabled && a.eq.highShelf,
      },
      {
        kind: "number",
        key: "eq.highCutoffHz",
        label: "High cutoff",
        unit: "Hz",
        min: 20,
        max: 20_000,
        step: 1,
        log: true,
        when: (a) => a.eq.enabled && a.eq.highShelf,
      },
      { kind: "toggle", key: "eq.lowShelf", label: "Low shelf", when: (a) => a.eq.enabled },
      {
        kind: "number",
        key: "eq.lowGainDb",
        label: "Low boost",
        unit: "dB",
        min: -24,
        max: 36,
        sliderMin: -24,
        sliderMax: 24,
        step: 0.1,
        when: (a) => a.eq.enabled && a.eq.lowShelf,
      },
      {
        kind: "number",
        key: "eq.lowCutoffHz",
        label: "Low cutoff",
        unit: "Hz",
        min: 20,
        max: 20_000,
        sliderMin: 20,
        sliderMax: 5000,
        step: 1,
        log: true,
        when: (a) => a.eq.enabled && a.eq.lowShelf,
      },
      {
        kind: "number",
        key: "eq.q",
        label: "EQ Q factor",
        min: 0.1,
        max: 10,
        sliderMin: 0.1,
        sliderMax: 4,
        step: 0.001,
        when: (a) => a.eq.enabled,
      },
      {
        kind: "number",
        key: "eq.amount",
        label: "EQ blend amount",
        min: 0,
        max: 5,
        step: 0.01,
        when: (a) => a.eq.enabled,
        help: "0 bypasses, 1 is fully filtered, above 1 over-applies.",
      },
    ],
  },
  {
    id: "window",
    label: "Window & Weighting",
    params: [
      {
        kind: "select",
        key: "window",
        label: "Window type",
        options: [
          { value: "kaiser", label: "Kaiser (beta control)" },
          { value: "hann", label: "Hann" },
          { value: "hamming", label: "Hamming" },
          { value: "blackman", label: "Blackman" },
          { value: "blackmanHarris", label: "Blackman-Harris (92 dB)" },
          { value: "rectangular", label: "Rectangular (flat)" },
        ],
      },
      {
        kind: "select",
        key: "kaiserBetaMode",
        label: "Kaiser beta mode",
        when: isKaiser,
        options: [
          { value: "manual", label: "Manual (Kaiser beta)" },
          { value: "auto", label: "Auto (from dB range floor)" },
        ],
      },
      {
        kind: "number",
        key: "kaiserBeta",
        label: "Kaiser beta",
        min: 0,
        max: 100,
        sliderMin: 1,
        sliderMax: 55,
        step: 0.01,
        when: (a) => isKaiser(a) && a.kaiserBetaMode === "manual",
      },
      {
        kind: "select",
        key: "weighting",
        label: "Loudness weighting",
        options: [
          { value: "off", label: "Off (flat)" },
          { value: "a", label: "A-weighting (IEC 61672)" },
          { value: "c", label: "C-weighting (high SPL)" },
          { value: "itu468", label: "ITU-R 468 (noise)" },
        ],
      },
      {
        kind: "select",
        key: "magnitudeNorm",
        label: "Magnitude normalization",
        options: [
          { value: "coherentGain", label: "Coherent gain (mean(window) = 1)" },
          { value: "fullScale", label: "Full scale (sine amplitude 1 -> 1.0)" },
        ],
      },
    ],
  },
  {
    id: "loudness",
    label: "Loudness & Ballistics",
    params: [
      {
        kind: "select",
        key: "loudness",
        label: "Loudness mode",
        options: [
          { value: "db", label: "dB (range mapped to full height)" },
          { value: "off", label: "Off (linear magnitude)" },
        ],
      },
      {
        kind: "select",
        key: "dbReference",
        label: "dB reference",
        options: [
          { value: "framePeak", label: "Frame peak (loudest bin = top)" },
          { value: "dbfs", label: "0 dBFS (absolute)" },
          { value: "agc", label: "Slow AGC (peak follower)" },
        ],
      },
      {
        kind: "number",
        key: "dbRange",
        label: "dB range floor",
        unit: "dB",
        min: 10,
        max: 160,
        step: 0.1,
        when: (a) => a.loudness === "db",
      },
      {
        kind: "number",
        key: "agcReleaseMs",
        label: "AGC release",
        unit: "ms",
        min: 10,
        max: 60_000,
        sliderMin: 100,
        sliderMax: 10_000,
        step: 1,
        log: true,
        when: (a) => a.dbReference === "agc",
      },
      { kind: "toggle", key: "ballistics", label: "Ballistics enable" },
      {
        kind: "select",
        key: "ballisticsMode",
        label: "Ballistics mode",
        when: (a) => a.ballistics,
        options: [
          { value: "milliseconds", label: "Milliseconds (hop-rate independent)" },
          { value: "coefficient", label: "Per-hop coefficient" },
        ],
      },
      {
        kind: "number",
        key: "attack",
        label: "Attack speed",
        min: 0,
        max: 0.99,
        step: 0.001,
        when: (a) => a.ballistics && a.ballisticsMode === "coefficient",
      },
      {
        kind: "number",
        key: "release",
        label: "Release speed",
        min: 0,
        max: 0.99,
        step: 0.001,
        when: (a) => a.ballistics && a.ballisticsMode === "coefficient",
      },
      {
        kind: "number",
        key: "attackMs",
        label: "Attack",
        unit: "ms",
        min: 0,
        max: 2000,
        step: 0.1,
        when: (a) => a.ballistics && a.ballisticsMode === "milliseconds",
      },
      {
        kind: "number",
        key: "releaseMs",
        label: "Release",
        unit: "ms",
        min: 0,
        max: 5000,
        step: 0.1,
        when: (a) => a.ballistics && a.ballisticsMode === "milliseconds",
      },
    ],
  },
  {
    id: "performance",
    label: "Performance",
    params: [
      {
        kind: "select",
        key: "parallelLayers",
        label: "Parallel layers (one core each)",
        options: [
          { value: "auto", label: "Auto (from 3 layers)" },
          { value: "always", label: "Always" },
          { value: "never", label: "Never" },
        ],
      },
      {
        kind: "toggle",
        key: "spectralFeatures",
        label: "Spectral features (centroid, rolloff, flatness, flux, RMS, bands)",
      },
    ],
  },
];

/** Reads `key` ("eq.q") from the analysis config. */
export function readParam(a: AnalysisConfig, key: string): unknown {
  return key
    .split(".")
    .reduce<unknown>((node, k) => (node as Record<string, unknown> | undefined)?.[k], a);
}

/** The partial patch `{ eq: { q: v } }` for `key` = "eq.q". */
export function patchFor(key: string, value: unknown): Record<string, unknown> {
  return key.split(".").reduceRight<unknown>((inner, k) => ({ [k]: inner }), value) as Record<
    string,
    unknown
  >;
}

/** Slider position (0..1000) <-> value for a log or linear slider. */
export const SLIDER_STEPS = 1000;

export function toSlider(p: NumberParam, v: number): number {
  const lo = p.sliderMin ?? p.min;
  const hi = p.sliderMax ?? p.max;
  const x = Math.min(Math.max(v, lo), hi);
  const t = p.log && lo > 0 ? Math.log(x / lo) / Math.log(hi / lo) : (x - lo) / (hi - lo);
  return Math.round(t * SLIDER_STEPS);
}

export function fromSlider(p: NumberParam, pos: number): number {
  const lo = p.sliderMin ?? p.min;
  const hi = p.sliderMax ?? p.max;
  const t = pos / SLIDER_STEPS;
  const v = p.log && lo > 0 ? lo * Math.pow(hi / lo, t) : lo + t * (hi - lo);
  return roundTo(p, v);
}

/** Rounds to the parameter's step and clamps to its hard range. */
export function roundTo(p: NumberParam, v: number): number {
  const stepped = p.integer ? Math.round(v) : Math.round(v / p.step) * p.step;
  const clamped = Math.min(Math.max(stepped, p.min), p.max);
  return Number(clamped.toFixed(6));
}

/** What the engine resolves from `a` at `rate` (mirrors `SpectrumConfig::new` in av-dsp). */
export interface Resolved {
  windowSamples: number;
  layers: number;
  largestWindow: number;
  fftSize: number;
  outputBins: number;
  hzPerBin: number;
  framesPerSecond: number;
}

const nextPow2 = (n: number) => 2 ** Math.ceil(Math.log2(Math.max(1, n)));

export function resolve(a: AnalysisConfig, rate: number): Resolved {
  const raw =
    a.windowLengthMode === "milliseconds"
      ? Math.round((a.windowMs * rate) / 1000)
      : a.windowSamples;
  const windowSamples = Math.min(Math.max(raw, 16), 65_536);
  let layers = Math.min(Math.max(a.layers, 1), 6);
  while (layers > 1 && windowSamples * 2 ** (layers - 1) > 65_536) layers--;
  const largestWindow = windowSamples * 2 ** (layers - 1);
  const fftSize = a.zeroPadding
    ? Math.min(Math.max(nextPow2(a.fftSize), nextPow2(largestWindow)), 131_072)
    : largestWindow + (largestWindow % 2);
  const bins = fftSize / 2 + 1;
  const outputBins = a.rawBins || a.outputBinsMode === "auto" ? bins : a.outputBins;
  const hop = Math.min(Math.max(a.hopSamples, 1), fftSize);
  return {
    windowSamples,
    layers,
    largestWindow,
    fftSize,
    outputBins,
    hzPerBin: rate / fftSize,
    framesPerSecond: rate / hop,
  };
}

const n = (v: number) => v.toLocaleString("en-US");

/** One line for the panel: windows, FFT, resolution, frame rate. */
export function summary(a: AnalysisConfig, rate: number): string {
  const r = resolve(a, rate);
  const windows =
    r.layers > 1
      ? `${r.layers} layers, windows ${n(r.windowSamples)} to ${n(r.largestWindow)}`
      : `window ${n(r.windowSamples)}`;
  return `${windows} · FFT ${n(r.fftSize)} (${r.hzPerBin.toFixed(2)} Hz/bin) · ${n(r.outputBins)} output bins · ${r.framesPerSecond.toFixed(1)} frames/s at ${n(rate)} Hz`;
}
