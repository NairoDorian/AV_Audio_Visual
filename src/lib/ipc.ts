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

export type FftPreset = "lowLatency" | "default" | "precision" | "lowPower";
export type BandLayout = "log" | "linear" | "mel";

/** The subset of `AppConfig` the Studio edits today (crates/av-protocol/src/config.rs). */
export interface AnalysisConfig {
  preset: FftPreset;
  barCount: number;
  layout: BandLayout;
  minHz: number;
  maxHz: number;
  dbFloor: number;
  dbCeiling: number;
  attackMs: number;
  releaseMs: number;
  autoGain: boolean;
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

export type SourceRequest = { kind: "demo" } | { kind: "file"; path: string };

export interface SourceInfo {
  kind: "demo" | "file";
  label: string;
  sampleRate: number;
  channels: number;
}

export interface AudioStatus {
  source: SourceInfo | null;
  running: boolean;
  fftSize: number;
  hopSamples: number;
  barCount: number;
  generation: number;
  frames: number;
  dspUsAvg: number;
  dspUsMax: number;
  overruns: number;
  droppedSamples: number;
  silent: boolean;
  starvedHops: number;
  clock: string;
}

/** True when the Studio runs inside the Tauri webview, false in a plain browser tab. */
export function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export const getAppInfo = () => invoke<AppInfo>("get_app_info");
export const getConfig = () => invoke<AppConfig>("get_config");
export const patchConfig = (patch: Record<string, unknown>) =>
  invoke<Applied>("patch_config", { patch });
export const startSource = (request: SourceRequest) =>
  invoke<SourceInfo>("start_source", { request });
export const stopSource = () => invoke<void>("stop_source");
export const getAudioStatus = () => invoke<AudioStatus>("get_audio_status");

/** The Studio's single frame bus (one poller per window). */
export const frameBus = new FrameBus({
  fetchFrame: (lastSequence, wantBins) =>
    invoke<ArrayBuffer>("get_frame", { lastSequence, wantBins }),
  requestFrame: (cb) => requestAnimationFrame(cb),
  cancelFrame: (id) => cancelAnimationFrame(id),
  isHidden: () => document.visibilityState === "hidden",
  now: () => performance.now(),
});
