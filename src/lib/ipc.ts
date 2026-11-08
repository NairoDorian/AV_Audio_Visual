// The only module that calls `invoke()`. Once ADR-014's typed bindings land (Track A step A3)
// these hand-written shapes are replaced by the generated `bindings.ts`.
import { invoke } from "@tauri-apps/api/core";
import { FrameBus } from "./frame/bus";

export interface AppInfo {
  name: string;
  version: string;
  identifier: string;
  os: string;
  arch: string;
  debug: boolean;
}

export interface EqSettings {
  enabled: boolean;
  highShelf: boolean;
  highGainDb: number;
  highCutoffHz: number;
  lowShelf: boolean;
  lowGainDb: number;
  lowCutoffHz: number;
  q: number;
  amount: number;
}

/** `AnalysisConfig` schema v2 (crates/av-protocol/src/config.rs): every FFT CHOP parameter. */
export interface AnalysisConfig {
  channelMode: "monoMix" | "left" | "right";
  rawBins: boolean;
  scale: "log" | "mel" | "erb" | "bark" | "chroma" | "linear" | "melog";
  displayMaxHz: number;
  outputBinsMode: "fixed" | "auto";
  outputBins: number;
  warpBlend: number;
  warpInterp: "linear" | "cubic";
  warpAggregation: "off" | "peak" | "rms";
  logFloorHz: number;
  windowLengthMode: "samples" | "milliseconds";
  windowSamples: number;
  windowMs: number;
  layers: number;
  hopSamples: number;
  zeroPadding: boolean;
  /** One FFT size for every layer (zero-padding), a power of two. */
  fftSize: number;
  eq: EqSettings;
  window: "kaiser" | "hann" | "hamming" | "blackman" | "blackmanHarris" | "rectangular";
  kaiserBetaMode: "manual" | "auto";
  kaiserBeta: number;
  weighting: "off" | "a" | "c" | "itu468";
  magnitudeNorm: "coherentGain" | "fullScale";
  loudness: "db" | "off";
  dbReference: "framePeak" | "dbfs" | "agc";
  dbRange: number;
  agcReleaseMs: number;
  ballistics: boolean;
  ballisticsMode: "coefficient" | "milliseconds";
  attack: number;
  release: number;
  attackMs: number;
  releaseMs: number;
  spectralFeatures: boolean;
  /** Raw audio scope samples per frame, half-sine tapered (0 = off). */
  waveformSamples: number;
  layerRefresh: "everyFrame" | "hopScaled";
  /** Threads the layers run on, the analysis thread included (0 = auto). */
  layerThreads: number;
}

export interface PresetInfo {
  id: string;
  name: string;
  builtin: boolean;
}

export interface AppConfig {
  schemaVersion: number;
  analysis: AnalysisConfig;
  scene: string;
  [key: string]: unknown;
}

export interface LoadReport {
  clamped: string[];
  healed: string[];
  normalized: string[];
}

export interface Applied {
  config: AppConfig;
  report: LoadReport;
}

export type VisualStyle = "bars" | "mirroredBars" | "line" | "area" | "concentric";

export interface Ring {
  bins: number;
  floor: number;
  gain: number;
  radius: number;
  outerLine: boolean;
  innerLine: boolean;
  outerBars: boolean;
  innerBars: boolean;
  spanBars: boolean;
  baseCircle: boolean;
  barScale: number;
  barSpacing: number;
}

export interface ScopeStyle {
  enabled: boolean;
  gain: number;
  size: number;
  horizontalMirror: boolean;
  verticalMirror: boolean;
  copies: number;
  lineWidth: number;
}

/** A custom colour per component; `null` uses the palette. */
export interface SceneColors {
  bars: string | null;
  outerLine: string | null;
  innerLine: string | null;
  outerBars: string | null;
  innerBars: string | null;
  spanBars: string | null;
  baseCircle: string | null;
  scope: string | null;
}

/** `Scene` v2 (crates/av-protocol/src/scene.rs). */
export interface Scene {
  formatVersion: number;
  id: string;
  name: string;
  style: VisualStyle;
  /** Display bars of the bar styles, peak-pooled from the output bins. */
  barCount: number;
  barWidth: number;
  cornerRadius: number;
  opacity: number;
  palette: string[];
  ring: Ring;
  scope: ScopeStyle;
  colors: SceneColors;
}

export type SourceRequest =
  | { kind: "system" }
  | { kind: "microphone" }
  | { kind: "demo" }
  | { kind: "file"; path: string };

export interface SourceInfo {
  kind: "system" | "microphone" | "demo" | "file";
  label: string;
  sampleRate: number;
  channels: number;
}

export interface AudioStatus {
  source: SourceInfo | null;
  running: boolean;
  fftSize: number;
  hopSamples: number;
  outputBins: number;
  layers: number;
  windowSamples: number;
  /** Threads the layers run on (1 = the analysis thread only). */
  threads: number;
  peakHz: number;
  features: SpectralFeatures | null;
  generation: number;
  frames: number;
  dspUsAvg: number;
  dspUsMax: number;
  overruns: number;
  droppedSamples: number;
  silent: boolean;
  starvedHops: number;
  /** Peak raw input over the last status window, dBFS. */
  inputPeakDbfs: number;
  clock: string;
  capture: "running" | "finished" | "failed" | "stopped";
}

export interface SpectralFeatures {
  centroidHz: number;
  rolloffHz: number;
  flatness: number;
  flux: number;
  rmsDb: number;
  bassDb: number;
  midDb: number;
  highDb: number;
}

/** True when the Studio runs inside the Tauri webview, false in a plain browser tab. */
export function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export const getAppInfo = () => invoke<AppInfo>("get_app_info");
export const getConfig = () => invoke<AppConfig>("get_config");
export const getDefaults = () => invoke<AppConfig>("get_defaults");
export const patchConfig = (patch: Record<string, unknown>) =>
  invoke<Applied>("patch_config", { patch });
export const startSource = (request: SourceRequest) =>
  invoke<SourceInfo>("start_source", { request });
export const stopSource = () => invoke<void>("stop_source");
export const getAudioStatus = () => invoke<AudioStatus>("get_audio_status");
export const listScenes = () => invoke<Scene[]>("list_scenes");
export const getActiveScene = () => invoke<Scene>("get_active_scene");
export const listPresets = () => invoke<PresetInfo[]>("list_presets");
export const savePreset = (name: string) => invoke<PresetInfo>("save_preset", { name });
export const applyPreset = (id: string) => invoke<Applied>("apply_preset", { id });
export const deletePreset = (id: string) => invoke<void>("delete_preset", { id });
/** The FFT CHOP's Reset: clears filter, ballistics, AGC and feature state. */
export const resetAnalysis = () => invoke<void>("reset_analysis");

/** The Studio's single frame bus (one poller per window). */
export const frameBus = new FrameBus({
  fetchFrame: (lastSequence, wantBins) =>
    invoke<ArrayBuffer>("get_frame", { lastSequence, wantBins }),
  requestFrame: (cb) => requestAnimationFrame(cb),
  cancelFrame: (id) => cancelAnimationFrame(id),
  isHidden: () => document.visibilityState === "hidden",
  now: () => performance.now(),
});
