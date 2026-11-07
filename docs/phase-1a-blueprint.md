# Phase 1A blueprint: the buildable architecture

Status: plan, third pass (2026-11-07): three placements (section 10) and the ported updater (section 8.1) added. No application code yet. This is the document we execute
from when scaffolding starts. It turns the README architecture and the
[pre-plan](pre-plan.md) contracts into a concrete workspace: versions, directories, crate
boundaries, threads, scripts and an ordered task list with gates.

Read order: [`README.md`](../README.md) (what and why) -> [`pre-plan.md`](pre-plan.md) (contracts,
spikes, gates) -> this file (how the repository is laid out and built).

---

## 1. Verified stack snapshot

Probed on **2026-11-07** from registry.npmjs.org and crates.io. This is a snapshot, not a pin
file: `bun run update` keeps moving it forward. Re-probe before scaffolding if more than a week has
passed.

### 1.1 Toolchain

| Tool | Version | Note |
| :-- | :-- | :-- |
| Rust | 1.99.1 stable, edition 2024 | Local `rustc`/`cargo`. `rust-version = "1.99"` in the workspace. |
| Bun | 1.4.4 latest (local 1.4.0) | Upgrade local Bun before 1A.1. `bun-types` canary tracks Bun canary. |
| TypeScript | 7.1.0-dev.20261107.1 (`next`) | TS 7 is the native compiler. `latest` is 7.0.2. |
| Vite | 8.3.6 | `beta` tag (8.3.0-beta.1) is older than `latest`; take max semver. |

### 1.2 Tauri 3 family

| Package | Newest 3.x | Stable line (do not use) |
| :-- | :-- | :-- |
| `tauri` (crate) | 3.0.0-alpha.6 (2026-11-06) | 2.12.5 |
| `tauri-runtime-wry` | 3.0.0-alpha.6 | 2.12.3 |
| `tauri-build` | 3.0.0-alpha.4 | 2.7.4 |
| `tauri-plugin` | 3.0.0-alpha.4 | 2.7.2 |
| `tauri-plugin-single-instance`, `-log`, `-store`, `-global-shortcut`, `-autostart` | 3.0.0-alpha.3 (on crates.io) | 2.x |
| `@tauri-apps/cli` | 3.0.0-alpha.6 (`next`) | 2.12.4 (`latest`) |
| `@tauri-apps/api` | 3.0.0-alpha.3 (`next`) | 2.12.4 |
| `@tauri-apps/plugin-*` (JS) | 3.0.0-alpha.3 (`next`) | 2.x |

Facts that change the updater and the family pin:

1. **Family members do not share one version number.** `tauri` is alpha.6 while `tauri-build` is
   alpha.4 and the plugins are alpha.3. A "family pin" is the set that `tauri`'s own requirements
   resolve to (`tauri-build ^3.0.0-alpha.4`, `tauri-runtime ~3.0.0-alpha.5`,
   `tauri-utils ~3.0.0-alpha.4`), not "every crate at the same number".
2. **The v3 plugins are now published on crates.io.** CursorFX and ZER0 pull them from the
   `plugins-workspace` `v3` git branch; AV_AudioVisual uses the crates.io releases and needs no git
   dependency for plugins.
3. `tauri` 3.0.0-alpha.6 exposes features `unstable`, `tray-icon`, `specta`, `gtk3`, `gtk4`,
   `tracing`, `protocol-asset`, `isolation`, among others. `unstable` is what a webview-less window
   needs (ADR-013); `gtk3`/`gtk4` is a Linux decision for P1.5b.

### 1.3 SolidJS 2 family

| Package | Newest | Trap |
| :-- | :-- | :-- |
| `solid-js` | 2.0.0-rc.21 (`next`) | `latest` is 1.9.18. |
| `@solidjs/web` | 2.0.0-rc.21 (`next`) | `latest` points at **2.0.0-rc.0**. Must equal `solid-js` exactly. |
| `@solidjs/vite-plugin` | 3.0.0-next.57 (`latest`) | `next` points at **3.0.0-next.35**, older than `latest`. |
| `@solidjs/router` | 2.0.0-next.44 (`next`) | Only if routing is needed; v0.1 uses section state, not a router. |
| `vite-plugin-solid` | 2.11.x / 3.0.0-next.27 | **Old package name. Do not use** with Solid 2. |

The rule that follows: **dist-tags are hints, not truth.** The updater takes the highest semver
across all published versions on the allowed channels, and refuses to move backwards.

### 1.4 Rust crates

| Area | Crate | Newest | Use |
| :-- | :-- | :-- | :-- |
| GPU | `wgpu` | 30.0.1 | Renderer. DX12 + Vulkan on Windows, Metal on macOS, Vulkan on Linux. |
| GPU | `raw-window-handle` | 0.6.2 | Host -> renderer handoff. |
| Capture | `cpal` | 0.18.2 | Microphone; candidate for WASAPI loopback (P0.3 compares). |
| Capture | `wasapi` | 0.27.0 (2026-11-06) | Loopback candidate with event mode and device position. |
| Capture | `rtrb` | 0.4.1 | SPSC ring from the real-time path. |
| Capture | `rubato` | 5.0.1 | Resampling in the worker, never in the callback. |
| Capture (macOS) | `objc2`, `objc2-core-audio`, `objc2-app-kit` | 0.6.5 / 0.3.2 / 0.3.2 | Process tap, `NSPanel`, desktop-level window. |
| Capture (macOS) | `screencapturekit` | 11.1.2 | macOS 13 fallback. |
| Capture (Linux) | `pipewire` | 0.10.1 | Monitor of the default sink. |
| DSP | `realfft` / `rustfft` | 3.5.1 / 6.4.1 | Preplanned real FFT. |
| File source | `symphonia` | 0.6.1 | Deterministic test source and fixtures. |
| Wire/data | `bytemuck` | 1.25.2 | POD header/payload views. |
| Mailbox | `triple_buffer` / `arc-swap` | 9.0.0 / 1.9.3 | Status and config snapshots only; frames use the slot ring (section 5.2). |
| Platform | `windows` / `windows-sys` | 0.62.2 / 0.61.2 | Must match `tauri`'s `windows ^0.62`. |
| Platform (Linux) | `smithay-client-toolkit`, `x11rb` | 0.21.1 / 0.14.0 | Layer-shell and X11 hosts (Phase 2+). |
| IPC types | `specta` / `specta-typescript` | 2.0.0-rc.28 / 0.0.15 | Type export. `tauri` 3 depends on `specta ^2.0.0-rc.25` (optional). |
| IPC commands | `tauri-specta` | 2.0.0-rc.26 on crates.io, rc.27 on upstream `main` | **Requires `tauri ^2`.** Used through the rebased `tauri-specta-v3` fork pinned by `rev` (section 6.1). |
| Errors/logs | `thiserror`, `tracing`, `tracing-appender` | 2.0.21 / 0.1.44 / 0.2.5 | Typed errors, session log with rotation. |
| Bench | `criterion` | 0.8.2 | DSP, wire and render benchmarks. |

---

## 2. Repository layout

```text
AV_AudioVisual/
  app.meta.json                 single source of product identity (name, id, version, publisher)
  package.json  bun.lock        Bun workspace root; exact pins, no carets
  Cargo.toml    Cargo.lock      Rust workspace root; [workspace.dependencies] holds every version
  rust-toolchain.toml           channel = "stable", components = rustfmt, clippy
  index.html                    the only Vite entry: the Studio window
  vite.config.ts  tsconfig.json  .oxlintrc.json  .prettierrc
  .githooks/pre-commit          POSIX sh, calls `bun run precommit`
  .github/workflows/ci.yml      windows-latest, ubuntu-latest, macos-latest
  src/                          SolidJS 2 Studio (settings, scene editor, preview, diagnostics)
  src-tauri/                    composition root (package `av-app`): startup order, tray, commands, Studio window
  crates/
    av-protocol/                types and pure functions: config, scene, frame, wire, ids, errors
    av-dsp/                     pure DSP: no threads, no I/O, no clocks
    av-audio/                   capture backends, ring, device model, format conversion
    av-engine/                  source supervisor, analysis worker, frame slot ring, idle machine
    av-render/                  wgpu device, pipelines, targets, offscreen capture; no windowing
    av-platform/                native host windows, monitors, DPI, power/occlusion signals
  spikes/                       throwaway Phase 0.5 binaries; NOT workspace members
  scripts/                      update-deps.ts, pre-commit.ts, app-meta.ts, fixtures.ts, ...
  test/                         TS parity tests and generated fixtures
  docs/                         plans, ADRs (docs/adr/NNN-*.md), performance and testing notes
  evidence/                     git-ignored; spike and release evidence bundles
  references/                   read-only research library
```

`spikes/` is excluded from the workspace so a spike can never be imported by product code. Each
spike has its own `Cargo.toml`, a README with the question and the result, and is deleted after its
gate is recorded in `evidence/` and the ADR.

---

## 3. Crate boundaries

### 3.1 Allowed dependency graph

```text
                av-protocol            (serde, bytemuck, specta types; no internal deps)
               /     |      \
          av-dsp  av-audio  av-render  av-platform
               \     |         |         |
                av-engine      |         |
                     \         |         /
                      src-tauri (composition root)
```

| Crate | May depend on | Must not depend on | Why |
| :-- | :-- | :-- | :-- |
| `av-protocol` | nothing internal | anything with threads, I/O, GPU, OS APIs | It is shared by Rust, fixtures and TS parity. |
| `av-dsp` | `av-protocol` | `av-audio`, `av-engine`, clocks, threads, logging | Pure functions are testable against golden vectors. |
| `av-audio` | `av-protocol` | `av-dsp`, `av-render`, Tauri | Capture is about samples and devices only. |
| `av-engine` | `av-protocol`, `av-dsp`, `av-audio` | `av-render`, `av-platform`, Tauri | Owns analysis threads; publishes frames. |
| `av-render` | `av-protocol` | `av-platform`, `av-audio`, Tauri | Accepts `raw-window-handle` targets; knows nothing about hosts. |
| `av-platform` | `av-protocol` | `av-render`, `av-engine`, Tauri internals | Creates native windows and reports monitors; hands handles out. |
| `src-tauri` | all of the above | being imported by any crate | Wiring, startup order, commands, tray. |

A `cargo deny`-style check or a short script in `pre-commit.ts` reads `cargo metadata` and fails on
an edge outside this table. A crate over about 2,000 lines in one module triggers a split review.

### 3.2 What lives where

| Crate | Owns | Key public types |
| :-- | :-- | :-- |
| `av-protocol` | `AppConfig` (`schema_version`), `Scene` (`format_version`), `ShareCode`, migrate/self-heal/sanitize/normalize, frame header and flags, wire encode/decode, `MonitorId`, `SourceId`, `Generation`, error enums | `AppConfig`, `Scene`, `FrameHeader`, `FrameFlags`, `WireVersion`, `wire::{encode, decode}` |
| `av-dsp` | Windows, FFT plan cache, band-edge builders, weighting, EQ, auto-gain, ballistics, peak hold, DC blocker, loudness (later), features | `SpectrumConfig`, `Analyzer::process(&mut self, hop: &[f32], dt) -> &FrameData`, `BandLayout` |
| `av-audio` | `CaptureBackend` trait, push/pull adapters, format conversion, channel masks, device enumeration and change notifications, file source | `CaptureBackend`, `CaptureFormat`, `CaptureStatus`, `CaptureError`, `AudioSink` (ring producer) |
| `av-engine` | Source supervisor, analysis worker thread, generation swaps at hop boundaries, frame slot ring, status channel, idle state machine | `Engine`, `FrameReader`, `StatusSnapshot`, `IdleState` |
| `av-render` | Instance, adapter, device, pipelines, per-target surfaces and uniforms, pacing policy, settle/park, offscreen capture | `Renderer`, `RenderTarget`, `PresentPolicy`, `CaptureTarget` |
| `av-platform` | Overlay host (Phase 1B), Background and Taskbar hosts (later), monitor enumeration, DPI, power/fullscreen/occlusion signals, main-thread hops | `SurfaceHost`, `HostState`, `Monitor`, `PlatformSignals` |

---

## 4. Runtime model

### 4.1 Threads

| Thread | Name | Created by | Owns | Never does |
| :-- | :-- | :-- | :-- | :-- |
| Tauri main | `main` | OS | Studio window, tray, commands, macOS AppKit windows | FFT, present, blocking I/O in a command |
| Capture | `av-capture-<source>` | `av-audio` backend (or CPAL) | Device client, ring producer | Allocate, lock, log, FFT, resample |
| Analysis | `av-analysis-<source>` | `av-engine` | Ring consumer, `Analyzer`, slot ring writer | Touch GPU or windows |
| Render | `av-render` | `src-tauri` at startup step 10 | wgpu device, surfaces, targets | Query Tauri per frame, wait on the webview |
| Host pump (Windows) | `av-host-win` | `av-platform` | Native host HWNDs and their message loop | Render, query audio |
| Supervisor | `av-supervisor` | `av-engine` | Device-change handling, backoff, deep-idle peak meter | Real-time work |

Windows: native visualizer HWNDs are created and pumped on `av-host-win`; the render thread
presents to them through `raw-window-handle` (DX12/Vulkan allow presenting from another thread).
macOS: `NSWindow`/`NSPanel` creation and geometry hop to the main thread
(`run_on_main_thread`); the render thread presents through a `CAMetalLayer`. Linux: Wayland/X11
host connections live on the host thread; GTK work, if any, hops to the main thread.

### 4.2 Startup

The pre-plan's ten-step order (`pre-plan.md` section 5.1) is implemented in `src-tauri/src/startup.rs`
as one function with numbered steps and a test that asserts the panic hook is installed before any
window exists.

### 4.3 Shutdown and panic

- Order: stop accepting commands -> hide hosts -> stop render (release pending textures, drop
  surfaces) -> destroy host windows -> stop analysis -> stop capture -> flush config -> exit.
- The panic hook sends `HideAll` to the host pump and destroys native host windows directly if the
  pump is dead. `panic = "unwind"` (ADR-001) lets Drop guards run.

---

## 5. Data path specifics

### 5.1 Capture -> analysis

- Ring: `rtrb` of interleaved f32, capacity = 2 s at the capture rate, preallocated.
- On overflow the producer drops the newest chunk (an SPSC producer cannot pop) and increments an
  atomic overrun counter; the consumer detects the gap by sample position and resets ballistics
  continuity. ("Drop-oldest" in older notes means the analyzer skips forward to the newest hop.)
- Every pushed chunk carries a timestamp: the device position clock (WASAPI `pu64QPCPosition`, CPAL
  `InputCallbackInfo` capture instant) mapped to the process monotonic clock. A side ring of
  `(sample_index, monotonic_us)` anchors avoids widening every sample.

### 5.2 Analysis -> consumers: frame slot ring

`Arc<Frame>` per hop is a per-frame allocation and is rejected. Frames are published into a
**preallocated slot ring**:

- 64 slots (~680 ms at 93.75 Hz). Each slot is a fixed-capacity `FrameSlot` sized for the maximum
  payload (128 bars + 4,097 bins + 8 features), allocated once at engine start.
- The writer fills the next slot, then publishes it with a per-slot sequence (seqlock pattern:
  odd while writing, even when done) and an atomic `latest` index. No locks, no allocation.
- A reader copies a slot out (or reads bars in place) and retries if the sequence changed under
  it. The renderer reads ~200 bytes of bars per frame; bins are copied only for spectrogram scenes
  and the preview.
- The history exists for two reasons: **A/V offset** (pick the frame whose timestamp is closest to
  `now - offset`, ADR-016) and **interpolation** between the two frames that bracket the display
  time.

Status (DSP microseconds, overruns, device state, permission state, idle state) is a separate
`arc-swap` snapshot updated at most every 250 ms.

### 5.3 Preview transport (Studio)

- The Studio pulls; Rust never pushes frames as events.
- `get_frame(last_sequence: u32, want_bins: bool) -> tauri::ipc::Response` returns raw wire bytes:
  the 8-word header only when `last_sequence` is current, otherwise header + payload.
- One poller per Studio window, driven by `requestAnimationFrame`, capped at 30 Hz, and paused when
  the preview is not visible (`document.visibilityState`, intersection observer).
- The decode is zero-copy: a `Float32Array` view over the response `ArrayBuffer` at the payload
  offset. The view is consumed in the same rAF tick.
- Cost bound: 48 bars = 224 bytes/frame; with 1,025 bins = ~4.3 KB/frame, ~130 KB/s at 30 Hz.

### 5.4 Wire header v1

The README section 4.4 header stays at eight u32 words. Second-pass changes, both additive:

- **Wire version in the flags word:** bits 24-31 of `flags` carry `wire_version` (v1 = `1`); bits
  6-23 are reserved and must be zero. A decoder rejects an unknown version with an explicit error
  instead of misreading the payload. This removes the need for a header redesign later.
- **Timestamp meaning:** monotonic microseconds of the **newest sample in the analysis window**,
  taken from the device clock, not the time the frame was encoded. `render_time - timestamp` is the
  in-app audio age shown in diagnostics.

---

## 6. Tauri shell

- `tauri.conf.json`: `app.windows = []` (every window is created in code, ZER0 pattern);
  `build.devUrl = http://localhost:1420`; `beforeDevCommand = "bun run dev"`;
  `frontendDist = "../dist"`; identifier and product name mirrored from `app.meta.json`.
- **Webviews:** exactly one, the Studio (`label = "studio"`). Visualizer windows are native and
  webview-less (ADR-013). If P0.2 forces a fallback to Tauri webview windows, their labels start
  with `viz-` and they load a separate, empty `viz.html` entry, never the Studio bundle.
- **Capabilities:** `capabilities/studio.json` grants only the Studio label. No `shell:allow-*`, no
  `fs` scope beyond the app data directory.
- **CSP:** `default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'` (Solid
  inline styles), `connect-src ipc: http://ipc.localhost`. Tighten only.
- **Plugins in v0.1:** `single-instance` (second launch focuses the Studio) and
  `global-shortcut` (show/hide, click-through and layout-edit escape hatches, section 10.3). `log`
  is not used (own `tracing` session log), `store` is not used (own atomic config pipeline).
  `autostart` arrives in Phase 6.
- **Tray:** desired/applied snapshot (ZER0): Show Studio, Overlay on/off, Click-through, Source,
  Pause, Quit. The tray menu is rebuilt from a snapshot, never mutated item by item.
- **Release profile:** `panic = "unwind"`, `lto = "fat"`, `codegen-units = 1`,
  `strip = "symbols"`. Dev profile: `[profile.dev.package."*"] opt-level = 2` so wgpu/realfft are
  usable in `bun run tauri dev` and DSP benchmarks are not measured in debug.

### 6.1 Typed IPC: do we need `tauri-specta`? (ADR-014, decided 2026-11-07)

**What the app actually sends over IPC.** Spectrum frames do not use it (raw `ipc::Response`,
section 5.3). What remains is a small but type-heavy surface: about 20-30 commands (config get and
patch, devices, scenes CRUD, monitors, placements, layout edit, diagnostics, hotkeys) and 3-5
low-rate events (status snapshot <= 4 Hz, device change, host state, permission state). The
payloads are the big, evolving types: `AppConfig`, `Scene`, `Anchor`, `CaptureStatus`, error enums.

**What each option gives.**

| Need | `specta` + `specta-typescript` only | `tauri-specta` |
| :-- | :-- | :-- |
| TS types for `AppConfig`, `Scene`, errors (parity-critical) | Yes | Yes |
| Typed command wrappers (`commands.patchConfig(...)`) | No; hand-written in `ipc.ts` | Yes, generated |
| Command name/argument drift caught at build time | Only with a homemade manifest test | Yes, plus duplicate and rename checks |
| Typed events (`events.statusChanged.listen`) | No | Yes |
| `Result` error unions instead of untyped rejections | Hand-written | Yes (`ErrorHandlingMode`) |
| Type-safe mocks for `bun test` without Tauri | No | Yes (upstream `26880fd`) |
| Coupling to the Tauri release | None | Yes: it depends on `tauri` |

Conclusion: **we need it.** Types alone are not enough; dropping it means re-implementing its
command, event and drift checking for ~25 commands, which is the part of the IPC most likely to
break silently. The cost is one coupling to the Tauri release, which the fork below absorbs.

**Upstream status (checked 2026-11-07).** `specta-rs/tauri-specta` is active (last merge
2026-11-04, version 2.0.0-rc.27, crates.io still at rc.26), but `main` and the only other branch
(`tauri-ipc-introspection`, last touched 2026-05-07) both declare `tauri = "2"`. There is no
Tauri 3 release or branch. **Upstream cannot be used as published.**

**The fork (`NairoDorian/tauri-specta-v3`, local at `C:\Users\Z\Downloads\PROJECTS\tauri-specta-v3`).**

- It is **only a manifest retarget**: two commits (2026-09-15) on top of upstream `ebe38d6` that
  change `Cargo.toml` alone (`tauri` 3.0.0-alpha.0, specta rc.26 from a git `[patch]`). No source
  changes were needed.
- It is about **40 upstream commits behind**, including: `ErrorHandlingMode` default changed to
  `Throw` (breaking), duplicate/colliding command rejection, `#[tauri::command(rename)]` honoured,
  nested `Channel` rejection, typed `emitTo`, type-safe mocks, published specta rc.27 (the git
  `[patch]` is no longer needed).
- **Static compatibility check against Tauri 3.0.0-alpha.6** (cached crate source): every Tauri
  item the library uses exists (`Manager`, `Runtime`, `ipc::Invoke`, `Emitter`, `Listener`,
  `EventId`, `EventTarget`, `ipc::Channel`, `JavaScriptChannelId`, `Webview`, `AppHandle`, `State`,
  `try_state`/`manage`/`state`, `listen`/`once`/`emit`/`emit_to`/`emit_filter`). The one change
  found: Tauri 3 replaced the default `Wry` runtime alias with `tauri::DynRuntime`, and
  upstream's `tauri::Wry` appears only inside `#[cfg(test)]` (`src/lang/js_ts.rs` from line 1960).
  ZER0 already runs the fork as `Builder::<tauri::DynRuntime>` (`ZER0 src-tauri/src/lib.rs:794`).
- A compile test of upstream retargeted to alpha.6 was not run in this pass (building freshly
  downloaded code was blocked by the session's permission policy); it is the first gate of A3.

**Plan for the fork (Track A step A3, done in the local fork repo):**

1. Rebase the two fork commits onto upstream `main` (rc.27), or re-apply them: the conflict surface
   is `Cargo.toml` only.
2. Workspace dependencies: `tauri` 3.0.0-alpha.6 (`default-features = false`), `tauri-build`
   3.0.0-alpha.4, `tauri-plugin` 3.0.0-alpha.4; specta family at the newest published (try rc.28 /
   0.0.15; fall back to upstream's `=2.0.0-rc.27` / 0.0.14 if rc.28 breaks); **remove** the
   `[patch.crates-io]` git-main block.
3. Test-only fix: `tauri::Wry` -> `tauri::DynRuntime` (or `tauri_runtime_wry::Wry` where a real
   runtime is needed).
4. Examples: retarget to Tauri 3 where cheap; otherwise drop them from `[workspace].members` in the
   fork so `cargo test -p tauri-specta` and `-p tauri-specta-macros` run.
5. Gate: `cargo test -p tauri-specta --all-features` green on Windows; the kitchen-sink snapshot
   and TS type-check tests from upstream pass; push; record the commit hash.
6. In AV_AudioVisual, depend on it by **git `rev`**, never a branch:
   `tauri-specta = { git = "https://github.com/NairoDorian/tauri-specta-v3", rev = "<hash>", features = ["derive", "typescript"] }`,
   and pin `specta`/`specta-typescript` to exactly the versions the fork pins (lockstep group).
   No `[patch]` for the Specta family is needed any more.
7. Configure explicitly: `ErrorHandlingMode::Result` (the Studio shows typed errors; do not rely
   on the new `Throw` default), `Builder::<tauri::DynRuntime>`, camelCase commands.

**Containment.** All `tauri-specta` use lives in `src-tauri/src/ipc/` (command collection, event
registry, `export_bindings` test). Commands are plain `#[tauri::command]` + `#[specta::specta]`
functions, so leaving `tauri-specta` later means replacing that one module, not the commands.

**Exit criterion.** When a published `tauri-specta` declares `tauri ^3`, switch to it and delete the
git dependency. `update-deps.ts` checks this on every run (section 8.1).

---

## 7. Studio frontend (SolidJS 2)

```text
src/
  main.tsx                    mount via @solidjs/web; load i18n bundle before first paint
  App.tsx                     shell: sidebar sections, status bar (source, fps, DSP us, idle)
  lib/
    bindings.ts               GENERATED, never edited (ADR-014)
    ipc.ts                    the only module that calls invoke(); typed wrappers
    frame/wire.ts             header/flags/version decode, timestamp lo/hi -> ms
    frame/bus.ts              single rAF poller, latest-frame bus, subscribe/unsubscribe
    settings/store.ts         desired/applied mirror of AppConfig; debounced patch commands
    i18n/                     lazy-loaded locale bundles; en first
  features/
    visualizer/               scene list, style/shape/palette editors, live preview canvas
    audio/                    source picker, capability matrix, permission and error cards
    displays/                 monitors, overlay placement, click-through, per-monitor scene
    performance/              FFT preset, fps cap, idle policy, battery mode
    diagnostics/              counters, copy diagnostics, log location
  styles/                     tokens.css (light/dark), no CSS framework in v0.1
test/
  wire.parity.test.ts         decodes Rust-generated fixtures byte for byte
  settings.parity.test.ts     defaults/clamps match Rust fixtures
  fixtures/                   generated by `bun run fixtures`, committed
```

Rules:

- Config state is a Solid store mirrored from Rust. Edits are optimistic in `desired`; Rust answers
  with `applied` (sanitized). The UI shows a diff when Rust clamps a value.
- The frame bus is **outside** the reactive graph: canvases subscribe imperatively and draw in rAF.
  No signal is written at frame rate.
- The preview canvas: preallocated geometry, DPR capped at 2, gradients/fonts built on theme or
  size change only, `ResizeObserver` for size.
- No router in v0.1 (sections are local state). Add `@solidjs/router` only when deep links exist.
- Solid 2 API usage is confirmed against the installed rc in 1A.1 and recorded in a short
  `docs/solid2-notes.md`; do not write Solid 1 idioms from memory.

---

## 8. Scripts and gates

| Script | Command | Purpose |
| :-- | :-- | :-- |
| `dev` | `vite` | Used by `beforeDevCommand` only. |
| `tauri` | `tauri` | `bun run tauri dev` is the only launch command. |
| `build` | `vite build` | Production front-end build. |
| `typecheck` | `tsc -b` | TS 7. |
| `lint` | `oxlint src scripts test` | |
| `format` / `format:check` | `prettier --write .` / `--check .` + `cargo fmt` | |
| `test` | `bun test` | TS unit and parity tests. |
| `fixtures` | `bun scripts/fixtures.ts` | Runs `cargo run -p av-protocol --example dump_fixtures` and writes `test/fixtures/`. |
| `bindings` / `bindings:check` | `cargo test -p av-app export_bindings` | Writes or diffs `src/lib/bindings.ts`; CI fails on drift. |
| `meta:sync` / `meta:check` | `bun scripts/app-meta.ts` | Mirrors `app.meta.json` into manifests. |
| `update` | `bun scripts/update-all.ts` | Routine: rtk -> Bun canary -> `update-deps` -> Actions audit -> repomix. |
| `update-deps` | `bun scripts/update-deps.ts` | Always newest (prerelease or latest, whichever is newer). `--dry-run`, `--only <name>`; no stable mode. |
| `precommit` | `bun scripts/pre-commit.ts` | identity -> translations -> lint -> typecheck -> unit -> format -> crate-graph -> repomix |
| `precommit:full` | `... --full` | + `cargo clippy --workspace --all-targets -D warnings`, `cargo test --workspace`, bindings check |
| `precommit:routine` | `... --routine` | `update`, then `precommit:full` |
| `hooks:install` | `git config core.hooksPath .githooks` | Once per clone. |
| `bench` | `cargo bench -p av-dsp -p av-protocol` | Criterion; numbers go to `evidence/`. |

### 8.1 The dependency updater (ported from the references)

Policy: **every dependency is always on its newest published version, prerelease or latest,
whichever is newer.** There is no stable mode. The only way to hold something back is an entry in
`scripts/update-policy.ts` with a one-sentence reason, and every report prints every hold.

The script is a port, not a rewrite. Both reference updaters are permissively licensed (CursorFX
MIT OR Apache-2.0, ZER0 MIT), so porting is allowed; keep a provenance header naming both files.
ZER0's is the mature one; CursorFX's has better defaults for an always-newest policy.

| Take from | What | Reference |
| :-- | :-- | :-- |
| CursorFX | Newest is the default (no flag needed); `--dry-run`, `--help` | `CursorFX scripts/update-deps.ts:22-24` |
| CursorFX | Dist-tags **merged with the full version list**, so an untagged newest prerelease is found; `UNPUBLISHABLE_VERSION` filter for `0.0.0`/bare `-alpha.0` placeholders | `:27-45`, `:134-212` |
| CursorFX | crates.io **full version list** (not only `max_version`/`newest_version`), yanked skipped, strict upgrade only, never a lower prerelease tier | `:214-253` |
| CursorFX | Workspace manifests **discovered** (`Cargo.toml`, `src-tauri`, every `crates/*`), every dependency table incl. `[target.'cfg(..)'.dependencies]` and `[workspace.dependencies]`; anchored regex that keeps the original operator (`=` stays `=`) and does not rewrite `tauri-specta` when bumping `specta` | `:326-336`, `:421-466`, `:496-519` |
| CursorFX | `bun update` **without** `--latest` for transitive refresh, so it cannot clobber prerelease pins | `:521-525` |
| ZER0 | **Line pins**: a package resolved only on its accepted major from named tags (`next`, `latest`), never downgraded (Solid 2 trio) | `ZER0 scripts/update-deps.ts:355-418` |
| ZER0 | **Major ceilings** by prefix (`@tauri-apps/*` stays 3.x; 0.x ceilings on the minor) | `:227-343` |
| ZER0 | **Version lockstep**: `@tauri-apps/cli` and the `tauri` crate are one release, because the CLI validates `tauri.conf.json` before cargo runs | `:257-294` |
| ZER0 | Crates that stay stable under prerelease (`libc`, whose `1.0.0-alpha` would compile a second libc) | `:102-109` |
| ZER0 | **Cargo conflict retry**: parse "failed to select a version", hold the culprit with its reason, retry; fall back to one-at-a-time | `:1013-1063`, `:1677-1757` |
| ZER0 | **Clobber guard**: after the transitive refresh, re-add every prerelease and line-pinned spec that moved | `:1773-1836` |
| ZER0 | **Query deadline** (30 s default) so a dead registry cannot hang the run | `:89-100` |
| ZER0 | Report: held packages with reasons, current registry tags under each line pin, transitive crates behind latest (`cargo update --dry-run --verbose`), GitHub Actions `uses:` audit | `:1140-1338` |
| ZER0 | `update-all.ts` orchestration order: rtk -> deps -> repomix, with dry-run skipping writes | `ZER0 scripts/update-all.ts:47-70` |

What changes for AV_AudioVisual:

1. **No stable mode.** ZER0 defaults to stable and needs `--prerelease`; here newest is the only
   mode, so `bun run update-deps` and `bun run update` behave the same.
2. **Tauri family from crates.io.** The v3 plugins are published, so ZER0's
   `CARGO_VERSION_LOCKED` reason (git plugins written against alpha.2) no longer applies. The
   family rule becomes a **transaction**: `tauri`, `tauri-build`, `tauri-runtime-wry`,
   `tauri-plugin-*`, `@tauri-apps/cli`, `@tauri-apps/api` and `@tauri-apps/plugin-*` move together
   to the newest set that `tauri`'s own requirements accept; if validation fails, the whole set
   returns to the previous one and the report says which step failed.
3. **Lockstep groups** are data, not code paths: `solid-js` = `@solidjs/web` exactly;
   `@tauri-apps/cli` = `tauri` release; `specta`/`specta-typescript` = exactly what the
   `tauri-specta` fork pins (ADR-014); `windows` on the major `tauri` requires.
4. **Transactional rollback.** Both references exit with "files may already be modified". This
   one snapshots `package.json`, `bun.lock`, every `Cargo.toml` and `Cargo.lock` into the scratch
   area first and restores them on any failure, so a failed update leaves the tree as it was.
5. **Validation adds the Tauri config check.** ZER0 documents a CLI ahead of the core that broke
   `bun run tauri dev` while typecheck, clippy and tests passed. Fast mode validates
   `tauri.conf.json` against the installed CLI's `config.schema.json`; `--full` also runs
   `bun run tauri build --debug --no-bundle`.
6. **Bun and Actions.** `update-all.ts` runs `bun upgrade --canary` (the local Bun is already a
   canary) and audits/bumps `uses:` refs in `.github/workflows` to their newest tags.
7. **Toolchain.** `rustup update stable`; `rust-toolchain.toml` stays on `stable` as the one
   documented exception (see README section 11.2).
8. **Report file.** The report is also written to `evidence/updates/<date>.md` so the commit that
   lands an update can cite it.
9. **Fork awareness for `tauri-specta`.** A git dependency is not a crates.io version, so the
   updater handles it separately on every run: (a) query crates.io `tauri-specta` and read the
   newest release's `tauri` requirement; if it accepts 3.x, report **"upstream supports Tauri 3:
   drop the fork"** as the top line; (b) compare the pinned `rev` with the fork's `main` and with
   upstream `main`, and report how many upstream commits the fork is behind; (c) when the Tauri
   family moves, report that the fork's `tauri` requirement must be checked, because a fork pinned
   below the new alpha holds the whole family back. It never rewrites the `rev` on its own: moving
   it is a fork update with its own test run.

Assertions that fail the run: any `tauri*` crate or `@tauri-apps/*` package off 3.x; `solid-js`
and `@solidjs/web` different; Solid off 2.x; `tauri-specta` from any source other than the
ADR-014 one; a downgrade of anything.

Validation sequence (no flag skips it): `bun install` -> clobber guard -> Tauri config check ->
`bun run typecheck` -> `bun run build` -> `cargo check --workspace --all-targets` ->
`cargo test --workspace` (fast set) -> `bun test`. Then the report.

CI (`.github/workflows/ci.yml`): Windows, Linux and macOS run `bun install --frozen-lockfile`,
`bun run precommit:full` minus repomix. GPU tests are skipped in CI unless an adapter exists; the
offscreen snapshot test is required on the Windows developer machine before a renderer change.

---

## 9. Ordered work plan

Two tracks run in parallel. Track A builds the parts of the product that no spike can invalidate.
Track B answers the risky questions with throwaway code. Phase 1B starts only when both are green.

### Track A: product foundation (not throwaway)

| Step | Deliverable | Gate | Absorbs |
| :-- | :-- | :-- | :-- |
| **A1** | Workspace skeleton: `app.meta.json`, Bun + Vite + Solid 2 Studio shell, Tauri 3 `src-tauri` with startup order stub, Rust workspace with the six empty crates, rust-toolchain, oxlint/prettier/tsconfig. | `bun run tauri dev` opens the Studio; `bun run typecheck`, `vite build`, `cargo check --workspace` pass. | P0.1 |
| **A2** | Scripts ported from the references: `update-deps.ts` + `update-policy.ts` + `update-all.ts` (section 8.1), `pre-commit.ts` (ZER0), `app-meta.ts` (ZER0 identity mirror), crate-graph check, `.githooks/pre-commit`, CI. | `bun run update --dry-run` reports with holds and reasons; a forced validation failure restores every manifest and lockfile; `bun run precommit` green on Windows; CI green on all three OSes. | AGENTS 1.4-1.5 |
| **A3** | IPC: rebase and retarget the local `tauri-specta-v3` fork to upstream rc.27 + Tauri 3.0.0-alpha.6 (section 6.1 steps 1-5), push, pin by `rev`; `src-tauri/src/ipc/` with `ErrorHandlingMode::Result`; `export_bindings` test; `ipc.ts`; one `get_app_info` command and one typed event round-trip. | Fork: `cargo test -p tauri-specta --all-features` green. App: bindings check fails on a deliberate drift and passes after regenerate. | ADR-014 |
| **A4** | `av-protocol`: header, flags, wire version, encode/decode, fixtures dumper; `wire.parity.test.ts`. | Byte-identical Rust/TS fixtures incl. sequence wrap, header-only, 64-bit timestamp, unknown version rejected. | P0.5 |
| **A5** | `av-protocol` config: `AppConfig`/`Scene` v1, migrate -> heal -> sanitize -> normalize, atomic save with debounce, partial/corrupt-store tests, settings parity fixture. | Migration and partial-store tests pass; NaN and range clamps pinned. | |
| **A6** | `av-dsp`: plan cache, Hann, band layouts (log first), weighting, ballistics, DC blocker, auto-gain; golden vectors; allocation gate; criterion bench. | Pre-plan section 6 numeric gates; zero allocations after warm-up; p99 < 2 ms recorded. | P0.4 |
| **A7** | `av-audio` file source (symphonia) + `av-engine` worker + slot ring + status channel. | File -> analyzer -> slot ring golden frames; reader retry test; no allocation in publish. | |
| **A8** | Studio preview: `get_frame` raw response, `frame/bus.ts`, preview canvas drawing the file source. | Preview matches fixtures; idle Studio with hidden preview polls nothing. | |
| **A9** | `av-render` headless: device, instanced SDF bars, offscreen target, PNG readback; premultiplied invariant test. | Deterministic snapshot at fixed size on the dev GPU; `rgb <= a` everywhere. | |
| **A10** | `av-platform`: monitor enumeration with stable IDs, DPI, the `SurfaceHost` trait and state machine with a fake host in tests. | Stable IDs across re-enumeration; state-machine tests. | |

### Track B: de-risking spikes (throwaway, under `spikes/`)

| Spike | Question | Gate |
| :-- | :-- | :-- |
| **P0.2** | Transparent, webview-less overlay on Windows: native HWND (and Tauri `unstable` window as variant) + wgpu DX12 with `DxgiFromVisual` presentation + premultiplied non-sRGB target. Vulkan as second variant. | Pre-plan P0.2 gate, plus: no webview exists in the overlay process path; GPU capture shows correct alpha. |
| **P0.3** | WASAPI loopback via `cpal` loopback and via the `wasapi` crate; pick one. | Pre-plan P0.3 gate, plus device-position timestamps available. |
| **P0.6** | Overlay lifecycle: panic, close, DPI change, monitor unplug. | Pre-plan P0.6 gate. |
| **P0.7** | Capture-to-photon latency of the default and low-latency presets. | Measured p50/p99 recorded; section 6.3 of the README updated with numbers. |
| **P0.8** | Background: a wgpu surface as a child behind the icons on the classic `WorkerW` and the 24H2 `Progman` layout; Explorer restart (`TaskbarCreated`) re-attach; Win+D; icons stay clickable. | Visible behind icons on both layouts (or one marked unverified); re-attaches within 2 s of an Explorer restart; no input captured. |
| **P0.9** | Taskbar: an owned, topmost, non-activating strip over a free region found with `SHAppBarMessage` + UI Automation; z-order re-asserted on WinEvent; follows edge, auto-hide and a centred/left icon layout. | Stays visible above the bar after clicking the taskbar and the Start button; never steals focus; hides with auto-hide and fullscreen; vertical taskbar handled or marked unverified. |

### Exit of Phase 1A

All Track A gates green, Track B results recorded with evidence (P0.8/P0.9 may be "feasible with
caveats", never skipped), ADR-001/003/005/007/013/014/016/019 written, and still no product
visualizer window. Then Phase 1B (pre-plan section 9) wires
WASAPI -> engine -> renderer -> overlay host.

---

## 10. The three placements in `av-platform`

The visual lives in three places (README section 1). They share the renderer and the engine and
never share window flags.

### 10.1 Types

```rust
pub enum Placement { Background, Taskbar, Overlay }

/// Geometry is an anchor, never absolute pixels. Re-resolved on display, DPI, work-area and
/// taskbar changes. Stored in AppConfig (machine-specific, excluded from share codes).
pub struct Anchor {
    pub monitor: MonitorId,        // stable device identity, not an index
    pub frame: AnchorFrame,        // WorkArea | FullMonitor | VirtualDesktop | TaskbarBand
    pub anchor: AnchorPoint,       // corners, edge centres, centre, Free
    pub offset_dip: (f32, f32),
    pub size_dip: (f32, f32),      // Background: may equal the frame; Taskbar: width x band height
    pub orientation: Orientation,  // Horizontal | Vertical (side taskbars, sidebars)
}

pub enum InputMode { Locked /* click-through */, Edit /* drag, resize, hot zones */ }

pub struct SurfaceSpec { pub placement: Placement, pub anchor: Anchor, pub scene: SceneId,
                         pub input: InputMode, pub capture_visible: bool }
```

Each placement implements `SurfaceHost` (pre-plan section 5.5) with its own attach, health check
and fallback. `HostManager` owns all live hosts; the render thread receives
`(TargetId, raw handle, physical rect, present policy)` and nothing else.

### 10.2 Behaviour per placement

| Concern | Background | Taskbar | Overlay |
| :-- | :-- | :-- | :-- |
| Windows host | Child of `WorkerW` (classic) or the 24H2 `Progman` sibling; top-level hidden message window for `WM_DISPLAYCHANGE`/`WM_SETTINGCHANGE`/`TaskbarCreated` | Owned topmost `WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE` strip; `SHAppBarMessage` for edge/rect/auto-hide; UI Automation for free regions; `SetWinEventHook` to re-assert z-order | Topmost `WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE`; `DWMWCP_DONOTROUND`; `WDA_EXCLUDEFROMCAPTURE` when `capture_visible = false` |
| Alpha / clear | Opaque or premultiplied over a cached wallpaper sample | Premultiplied | Premultiplied, full clear each frame |
| Pacing | Mailbox or carried deadline, lower default fps | Mailbox, 30-60 fps | Fifo on its monitor |
| Pause | Covered/maximized/fullscreen on that monitor; session lock | Bar hidden (auto-hide, fullscreen) | Hidden; fullscreen policy |
| Health | Every 2 s: parent still valid, still behind icons | Every 1 s and on WinEvent: bar rect, edge, visibility | On display/DPI change |
| Fallback | Overlay over the same region, `HWND_BOTTOM`-ish, with notice | Overlay docked next to the bar | In-app window |
| macOS | Desktop-level `NSWindow`, ignores mouse, all spaces | `NSStatusItem` view | `NSPanel` non-activating, status level |
| Linux | layer-shell `background` / X11 desktop type | layer-shell strip / panel applet | layer-shell `overlay` / X11 override-redirect |

### 10.3 Layout edit mode and escape hatches

- One `LayoutEditor` drives all three: it shows an interactive frame per surface (for Background
  and Taskbar, a temporary overlay frame over the region, because those hosts never take input),
  snap guides to work-area edges/corners and bar free regions, size handles, arrow-key nudge, and
  "Done". Geometry is committed once, at the end of a drag (`WM_EXITSIZEMOVE` equivalent).
- Global hotkeys from v0.1 (`tauri-plugin-global-shortcut`): show/hide all surfaces, toggle
  click-through, enter layout edit mode. The tray carries the same three items. Both work when
  every surface is click-through.
- Text scale is separate from geometry scale (Audio HUD `1/sqrt(scale)` idea) so a small taskbar
  strip or overlay keeps readable labels.

### 10.4 Z-order honesty

Topmost is above normal and topmost windows and borderless-fullscreen apps, and below the Start
menu, notification centre, secure desktop, exclusive fullscreen and UI-access windows. The Studio
says so next to the overlay settings. Going above the Start menu needs a `uiAccess="true"` manifest,
an Authenticode signature and a protected install location (ADR-018, Phase 6). Undocumented
`CreateWindowInBandEx`/`SetWindowBand` are never used.

---

## 11. Things to confirm during A1 (do not assume)

- Tauri 3 alpha.6 API names for webview-less windows under `unstable`, `run_on_main_thread` (a
  `Manager` trait method since alpha.3, per ZER0's updater notes), tray builder and
  `ipc::Response`. Tauri 3's default runtime type is `tauri::DynRuntime`; `tauri::Wry` is gone.
  Record any rename in `docs/tauri3-notes.md`.
- Solid 2 rc.21 primitives and the `@solidjs/web` render entry; record in `docs/solid2-notes.md`.
- Whether TS 7 `tsc -b` handles the Solid JSX preserve mode with `@solidjs/vite-plugin`.
- wgpu 30 DX12 `BackendOptions` field for composition-visual presentation (CursorFX reaches it
  through `WGPU_DX12_PRESENTATION_SYSTEM=Visual`; we set it in code, not through the environment).
- `cpal` 0.18 loopback behavior and timestamp quality versus `wasapi` 0.27.
- Whether the render endpoint's `IAudioMeterInformation` reports without any stream of ours, and
  whether loopback samples and the meter are before or after endpoint volume/mute (Audio HUD claims
  capture endpoints need an active stream).
- Whether wgpu's DX12 visual presentation path accepts a window created with
  `WS_EX_NOREDIRECTIONBITMAP`, and that `DWMWCP_DONOTROUND` removes the DWM corner clip.
