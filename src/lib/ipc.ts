// The only module that calls `invoke()`. Once ADR-014's typed bindings land
// (Track A step A3) these wrappers delegate to the generated `bindings.ts`.
import { invoke } from "@tauri-apps/api/core";

export interface AppInfo {
  name: string;
  version: string;
  identifier: string;
  os: string;
  arch: string;
  debug: boolean;
}

export function getAppInfo(): Promise<AppInfo> {
  return invoke<AppInfo>("get_app_info");
}

/** True when the Studio runs inside the Tauri webview, false in a plain browser tab. */
export function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
