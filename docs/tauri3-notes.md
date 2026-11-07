# Tauri 3 notes (confirmed against tauri 3.0.0-alpha.6)

Recorded during Track A step A1 (blueprint section 11).

- `tauri::Builder::default()` is `Builder<tauri::DynRuntime>`; `tauri::Wry` is gone. The runtime
  is selected with `.runtime(tauri_runtime_wry::Wry::default())`; without it, building fails with
  `RuntimeNotConfigured`.
- `tauri` default features in alpha.6: `compression`, `common-controls-v6`, `dynamic-acl`.
  Optional: `tray-icon`, `unstable` (webview-less windows, ADR-013), `specta`, `gtk3`/`gtk4`
  (Linux decision for P1.5b), `tracing`, `protocol-asset`, `isolation`.
- `tauri-build` is 3.0.0-alpha.4 while `tauri`/`tauri-runtime-wry` are alpha.6: family members
  carry different numbers.
- `WebviewWindowBuilder::new(manager, label, WebviewUrl::App(..))`, `generate_context!()`,
  `generate_handler!` and `#[tauri::command]` with an `AppHandle` argument work unchanged from v2.
- `run_on_main_thread` is a `Manager` trait method (since alpha.3).
- `tauri.conf.json` `$schema` points at the installed CLI's `config.schema.json`, so editors and
  the updater validate against the CLI that will actually run.
