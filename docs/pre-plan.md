# Deep pre-plan: evidence, de-risking and implementation gates

Status: second planning pass complete (2026-11-07), no application code written. This document is
the bridge between the reference studies and Phase 1A. It records what the audits changed, which
assumptions must be tested before production code, and which contracts must be frozen before the
first real subsystem is built.

Read this after the [cross-project synthesis](../references/studies/00-cross-project-synthesis.md)
and before scaffolding. The studies remain the evidence; this document is the contract and gate
list; [`phase-1a-blueprint.md`](phase-1a-blueprint.md) is the concrete workspace, version snapshot
and ordered task list.

---

## 1. How to use this document

- A **spike** is throwaway code with one question, one timebox and one written result. A spike never
  becomes the product by accident.
- A **contract** is a type, state machine or invariant that is expensive to change after renderer and
  host code exist. Freeze it before Phase 1A.
- A **gate** is evidence, not opinion. "It looked fine" is not a pass.
- If a spike fails, record the failure and take the fallback. Do not lower the gate silently.

Evidence grades used below:

| Grade | Meaning |
| :-- | :-- |
| **A** | Direct source evidence and a measured result on the target machine. |
| **B** | Direct source evidence, but no measurement of this combination yet. |
| **C** | Product hypothesis or inference. Must be tested before it becomes a rule. |

---

## 2. What the deeper audit changed

These are the second-order findings that were not explicit enough in the first synthesis. Each one
changes a plan step, not just the prose.

| # | Finding | Evidence | Consequence |
| --: | :-- | :-- | :-- |
| 1 | **Startup order is a safety system.** | CursorFX installs the panic hook before any window, registers the app handle, runs Vite preflight, loads config, then creates the tray and render thread (`lib.rs:560-779`). | Phase 1A must implement the startup order before the overlay exists. Preflight failure must exit before a full-desktop window is created. |
| 2 | **Panic strategy is an architecture decision.** | CursorFX uses `panic = "abort"` and relies on a panic hook to destroy overlay windows; ZER0 uses `panic = "unwind"` because Tauri error handling needs it (`Cargo.toml:289-293`). | v1 defaults to `unwind` so `Drop` guards work. If `abort` is chosen later, the panic-hook teardown must be tested first. This is ADR-001. |
| 3 | **Pacing is per surface, not per app.** | CursorFX uses Fifo for the pacing panel and Mailbox for slower targets; a single virtual-desktop window is throttled to the slowest panel (`overlay/mod.rs:290-313,1125-1149`). | One surface per monitor and a present-mode decision per target. Do not apply one global Fifo rule to a mixed-refresh desktop. |
| 4 | **Overlay and Background are different presenters.** | CursorFX refuses an opaque overlay surface; wallpaper mods use opaque composition and a wallpaper base (`Fluid`, `Parallax`). | Do not share alpha mode, clear color or surface configuration between modes. Share the renderer, not the presenter. |
| 5 | **Windows has two desktop layouts.** | Classic: `WorkerW` behind `SHELLDLL_DefView`. Windows 11 24H2+: `Progman` owns the icon layer and a `WorkerW` sibling; a layered child inside the raised `WorkerW` is not displayed (`Neiz :24970-24994`). | Background host discovery must detect the raised layout by `WS_EX_NOREDIRECTIONBITMAP` on `Progman`, validate the shell PID, and choose the parent per layout. `0x052C` is a last resort. |
| 6 | **`WM_DISPLAYCHANGE` only reaches top-level windows.** | Desktop Live Overlay uses a separate hidden message window for display changes and `WM_SETTINGCHANGE` (`:3419-3489`). | A Background child needs a top-level message window for display, DPI and wallpaper events. |
| 7 | **Capture must accept the endpoint mix format.** | Desktop AV, Tourne'Table and Neiz read `GetMixFormat`; Neiz rejects 24-bit packed formats and can fail silently (`Neiz :4160-4206,5133-5146`). | Capture at the endpoint format, support f32/s16/s32/s24-in-32 and `WAVE_FORMAT_EXTENSIBLE`, and normalize in the analysis worker. Do not assume 48 kHz at the API boundary. |
| 8 | **Capture adapters differ: push versus pull.** | CPAL is a callback; WASAPI loopback is an event-driven pull thread. ZER0 and the Windhawk mods use both models. | Define one ring contract and two adapter shapes. "No callback work" means no work in either real-time path. |
| 9 | **Stateful DSP belongs to the analyzer; stateless display belongs to the renderer.** | Neiz applies EQ, auto-gain and envelopes at render read, but its single-window model hides the divergence. | Ballistics, peak hold, auto-gain and EQ must run once per hop in the analyzer so every window sees the same frame. Geometry, color and gradients stay renderer-side. |
| 10 | **Window length, FFT size, bar count and axis rate are four different values.** | The FFT plugin keeps `window_samples`, `fft_size`, `output_bins` and axis rate separate; `realfft` accepts only even sizes (`fft-plugin.md` section 8). | Freeze separate config fields. v1 may set window = FFT = 2048, but the contract must not conflate them. |
| 11 | **Denormals and thread priority are real-time concerns.** | The FFT plugin uses FTZ/DAZ and MMCSS "Pro Audio"; Rust has no portable equivalent (`fft-plugin.md` section 8.3). | Flush denormals in ballistics/biquad state or set FTZ/DAZ under `unsafe` on x86 with a safety comment. Put Windows MMCSS and macOS/Linux QoS behind a platform module. |
| 12 | **A first frame must exist before the window is shown.** | Flip3D presents before `SWP_SHOWWINDOW`; CursorFX shows after first present; ZER0 creates the overlay hidden (`window-animations`, `ZER0 app study 3.2`). | Overlay creation is `create hidden -> first present -> show`. Never show at `(0,0)` and move. |
| 13 | **Show/hide and delayed work need generation counters.** | ZER0 bumps a generation for every overlay show/hide and aborts stale delayed hides (`overlay.rs:817-864,1308-1317`). | Any async show/hide, monitor rebuild or settings apply carries a generation. Late work is discarded, not applied. |
| 14 | **64-bit timestamps need care at the JavaScript boundary.** | ZER0 enables bigint coercion but warns that sample counters can exceed the safe integer range (`ZER0 app study 4`). | The wire header keeps `timestamp_lo/hi`. The TS decoder converts to milliseconds without losing precision; the native path uses the Rust `u64` directly. |
| 15 | **The overlay is not the Background, and the taskbar is not the overlay with a different position.** | The placement matrix and host studies show different input models, z-order and lifetime rules (`references/README.md` section 6). | Every host gets its own attach/health/detach implementation behind one trait. Shared rendering is mandatory; shared window flags are a bug. |

### 2.1 Second- and third-pass findings (2026-11-07)

Findings 16-25 come from the registry/reference re-check; 26-33 from the product direction on
the three placements, the Audio HUD reference and the updater port.

The second pass checked the plan against the live registries and re-read the reference code paths
the contracts depend on. Each finding changes a contract, a spike or an ADR.

| # | Finding | Evidence | Consequence |
| --: | :-- | :-- | :-- |
| 16 | **The opaque-desktop hazard comes from the overlay being a webview.** | CursorFX builds each overlay with `WebviewWindowBuilder` loading `overlay.html` and puts the wgpu surface on that HWND (`CursorFX src-tauri/src/overlay/windows.rs:57-68`). Without Vite the webview paints an error page. | Visualizer windows are **webview-less** native windows owned by `av-platform` (ADR-013). The only webview is the Studio. Preflight still guards the Studio, but no full-desktop webview exists to go opaque. |
| 17 | **DX12 transparency needs composition-visual presentation.** | CursorFX's instance comment: `WGPU_DX12_PRESENTATION_SYSTEM=Visual` "gives a DX12 swapchain the pre-multiplied alpha a transparent overlay needs" (`overlay/mod.rs:386-388`). | P0.2 sets the DX12 presentation system to the visual/composition path **in code**, not through an environment variable, and also tests Vulkan. "DComp later" in older notes means a hand-written DComp presenter; wgpu's own visual path is the default. |
| 18 | **Upstream `tauri-specta` does not support Tauri 3, but the fork is only a manifest retarget.** | `tauri-specta` rc.26 (crates.io) and upstream `main` rc.27 (2026-11-04) both declare `tauri = "2"`; no Tauri 3 branch exists. The local fork `tauri-specta-v3` changes `Cargo.toml` only (2 commits on upstream `ebe38d6`, ~40 commits behind). Every Tauri API the library uses exists in 3.0.0-alpha.6; `tauri::Wry` is used only in `#[cfg(test)]` and Tauri 3 replaced it with `tauri::DynRuntime`. | We need `tauri-specta` (typed commands, events, error unions, drift checks; blueprint 6.1). Rebase the fork onto upstream rc.27, retarget to alpha.6, drop the specta git `[patch]`, pin by `rev` (ADR-014). |
| 19 | **`Arc<Frame>` per hop is a per-frame allocation.** | ADR-003's earlier default published a fresh `Arc<Frame>` each hop; the allocation gate (section 6) forbids that. | Frames go into a **preallocated 64-slot ring** with per-slot seqlock sequences and an atomic latest index (`phase-1a-blueprint.md` section 5.2). |
| 20 | **The <50 ms latency target is not achievable on the default path.** | Budget in README section 6.3: half a 2048 Hann window alone is 21 ms; FIFO present plus DWM composition is 25-40 ms at 60 Hz. | Targets are restated per preset (default <= 80 ms p50 estimate, low-latency <= 45 ms) and P0.7 measures them. The 50 ms figure becomes the low-latency target, not the default. |
| 21 | **Loopback runs ahead of what the user hears on high-latency outputs.** | Loopback captures the mix before the output device; Bluetooth adds roughly 100-250 ms of output latency (unmeasured here, grade C). | The renderer selects frames by timestamp from the slot-ring history with a user **A/V offset** (ADR-016). This needs `timestamp` to be the device-clock time of the newest analysed sample. |
| 22 | **An SPSC ring cannot drop the oldest sample.** | `rtrb`'s producer cannot pop; "drop-oldest" in section 5.2 was not implementable as written. | The producer drops the newest chunk and counts it; the consumer detects the gap by sample position, resets continuity and skips to the newest hop. |
| 23 | **Registry dist-tags are not reliable version sources.** | 2026-11-07: `@solidjs/vite-plugin` `next` = 3.0.0-next.35 while `latest` = 3.0.0-next.57; `@solidjs/web` `latest` = 2.0.0-rc.0 while `next` = rc.21; `vite` `beta` is older than `latest`. | `update-deps.ts` takes the highest semver across all versions on allowed channels and never moves backwards. Tags are hints. |
| 24 | **Tauri family members do not share one version.** | 2026-11-07: `tauri` 3.0.0-alpha.6, `tauri-build` alpha.4, plugins alpha.3, all compatible through `tauri`'s own ranges. The v3 plugins are now on crates.io. | The family pin is resolved from `tauri`'s dependency requirements, not by matching numbers. Plugins come from crates.io, not the `plugins-workspace` git branch. |
| 25 | **The wire header had no version.** | P0.5's fallback was "add an explicit protocol version"; the frozen header had no field for one. | `flags` bits 24-31 carry `wire_version` (v1 = 1). Bits 6-23 stay reserved. Decoders reject unknown versions. |
| 26 | **All three placements are product requirements, not a roadmap tail.** | Product direction (2026-11-07): the visual must live in the background/wallpaper, the taskbar and an overlay above everything that is movable and resizable. | Background and Taskbar get feasibility spikes (P0.8, P0.9) in Phase 0.5 so their host shapes are proven before the host trait freezes; they still ship after the overlay. Any combination runs at once from one engine. |
| 27 | **The endpoint peak meter is a free wake signal.** | Audio HUD reads `IAudioMeterInformation::GetPeakValue` per tick with no sample capture (`audio_hud.wh.cpp:630-637,654-657`); its comment says capture endpoints need an active stream to report (`:603-606`). | Deep idle stops loopback and polls the render endpoint meter at ~4 Hz. P0.3 verifies when the meter reports without our stream. |
| 28 | **Muted is not silent.** | Audio HUD reads `IAudioEndpointVolume::GetMute` and draws "Muted" (`:639-640,1649-1650`). | A `Muted` state in capture status and the idle machine; the UI says "Muted" instead of decaying bars. P0.3 records whether loopback is pre- or post-volume/mute. |
| 29 | **The overlay window needs `WS_EX_NOREDIRECTIONBITMAP` and no DWM corner clip.** | Audio HUD: NOREDIRECTIONBITMAP + DComp premultiplied swap chain; `DWMWCP_DONOTROUND`; acrylic via `SetWindowCompositionAttribute` together with DComp bleeds rectangular blur (`:456-478,1143-1185,2587-2591`). | Native overlay host uses these styles; no accent/acrylic API on the visualizer window; blur, if ever, is drawn by the scene from a wallpaper sample. |
| 30 | **Click-through needs an exit that does not depend on the overlay.** | Audio HUD's only way back from click-through without the host's settings is a global hotkey (`:2456-2475`). | Show/hide, click-through and layout-edit hotkeys plus tray items ship in v0.1 (`global-shortcut` plugin moves from Phase 3 to v0.1). |
| 31 | **Overlay geometry is an anchor to the work area.** | Audio HUD computes corners from `rcWork` with a margin and re-runs on `WM_DISPLAYCHANGE`; saves only on `WM_EXITSIZEMOVE` (`:1012-1057,2504-2517`). | `Anchor { monitor, frame, anchor, offset_dip, size_dip, orientation }` for all three placements (blueprint section 10). |
| 32 | **"Above everything" has a documented ceiling.** | Audio HUD reaches the UI-access band through undocumented `CreateWindowInBandEx`/`SetWindowBand`, which only works from inside Explorer (`:351-401`). | Topmost is the v1 contract with its limits stated in the UI; the documented `uiAccess` manifest route is ADR-018. |
| 33 | **Both reference updaters have holes an always-newest policy exposes.** | CursorFX: no family transaction, no lockstep CLI/core, no rollback (`scripts/update-deps.ts:316`). ZER0: stable by default, git-plugin family lock, `bun update --latest` needs a clobber guard, no rollback (`scripts/update-deps.ts:84-87,1667,1773-1836`). | Port and merge them (blueprint section 8.1): newest by default, line pins, lockstep groups, Tauri family transaction, cargo conflict retry, clobber guard, Tauri config validation, transactional rollback. ADR-019 records the policy. |

---

## 3. Assumption ledger

These are the assumptions the plan depends on. A low-confidence assumption gets a spike before it
gets code.

| ID | Assumption | Confidence | Falsifier | Phase |
| :-- | :-- | :-- | :-- | :-- |
| A1 | Tauri 3 prereleases compile and run together with the chosen wgpu/Solid stack. | M | Family resolution or `cargo check` fails after a partial bump. | 0.5 |
| A2 | A transparent wgpu surface with premultiplied alpha and a non-sRGB UNORM target composites correctly under DWM. | M | Black/opaque window, halo, or invalid `c > a` pixels. | 0.5 |
| A3 | WASAPI shared-mode loopback at the endpoint mix format can be captured for 10 minutes without dropouts. | H | Unsupported format, deadlock on stop, or overruns. | 0.5 |
| A4 | 2048-point FFT, 512 hop, per-bar analysis stays below 2 ms p99 on one modern core. | M | Measured p99 exceeds budget at 48 kHz. | 0.5 |
| A5 | The binary frame can be encoded/decoded identically in Rust and TypeScript with negligible cost. | M | Byte mismatch, alignment bug, or decode cost above budget. | 0.5 |
| A6 | `WASAPI` device loss can be recovered visibly within 2 seconds without restarting the app. | M | Deadlock, silent freeze, or fallback to microphone. | 0.5/1B |
| A7 | One surface per monitor avoids mixed-refresh throttling. | M | Second monitor lags at the slowest panel's rate. | 1B/2 |
| A8 | The overlay can be non-focusable and click-through without stealing focus or clicks. | M | First click activates the window or is swallowed. | 0.5 |
| A9 | WorkerW/Progman attachment survives Explorer restart and 24H2 layout changes. | L | Invisible surface, wrong z-order, or icons become unclickable. | 4 |
| A10 | Native wgpu presentation is cheaper than a webview overlay at 4K. | M | GPU capture and frame timings show no benefit. | 1B |
| A11 | Settings migration and self-healing prevent data loss across schema changes. | H | Corrupt or partial store resets unrelated fields. | 1A |
| A12 | Silence/occlusion/battery idle can reach <1% CPU with the render loop parked. | M | Measured idle exceeds the budget. | 1B |
| A13 | A webview-less native window can host a transparent wgpu surface inside a Tauri 3 app without fighting Tauri's event loop. | M | Message-pump conflicts, focus theft, or the window never composites. | 0.5 |
| A14 | wgpu 30's DX12 visual presentation yields premultiplied alpha on a top-level native HWND. | M | Only opaque alpha modes reported, or halo/black corners in a GPU capture. | 0.5 |
| A15 | Device-clock timestamps are available from the chosen loopback API with sub-millisecond jitter. | M | Only arrival-time timestamps exist, or jitter > 2 ms. | 0.5 |
| A16 | The default preset reaches <= 80 ms p50 capture-to-photon at 60 Hz; low-latency reaches <= 45 ms at 120 Hz. | L | P0.7 measurement exceeds the target. | 0.5 |
| A17 | Upstream `tauri-specta` rc.27 compiles against Tauri 3.0.0-alpha.6 with a manifest retarget and a test-only `Wry` -> `DynRuntime` change. | M (static API check passed; not compiled yet) | `cargo test -p tauri-specta --all-features` fails on the retargeted fork. | 1A (A3) |
| A18 | A standalone process can keep a strip visible above the Windows taskbar without injection. | L | The taskbar covers the strip after a click, or focus is stolen. | 0.5 (P0.9) |
| A19 | A wgpu surface can live as a child behind the icons on both desktop layouts. | M | Invisible surface or blocked icons on 24H2. | 0.5 (P0.8) |
| A20 | The render endpoint peak meter reports without our own stream. | M | Meter reads zero during playback unless a client is open. | 0.5 (P0.3) |

---

## 4. Phase 0.5 de-risking spikes

Phase 0.5 runs alongside the first, risk-free half of Phase 1A (Track A in
[`phase-1a-blueprint.md`](phase-1a-blueprint.md) section 9). Spikes that answer a platform question
(P0.2, P0.3, P0.6, P0.7, P0.8, P0.9) are throwaway binaries under `spikes/`, outside the Cargo workspace.
Spikes that test pure contracts (P0.1, P0.4, P0.5) are done directly in the real workspace and
crates, because their output is the product's first tests rather than code to discard. Record every
result in `evidence/` and in the ADR queue.

| ID | Question | Timebox | Pass gate | Fail / fallback | Artifact |
| :-- | :-- | --: | :-- | :-- | :-- |
| **P0.1** | Does the sanctioned Tauri 3 + Bun + Vite launch path work from a clean clone? (Track A step A1) | 1 day | `bun run tauri dev` opens the Studio; preflight refuses to start when Vite is absent; the ADR-014 binding path generates `bindings.ts`. | Pin the last known-good Tauri 3 family; do not fall back to Tauri 2. | Build log, preflight log, resolved family list. |
| **P0.2** | Does a **webview-less** transparent wgpu surface composite correctly on Windows? | 2-3 days | Variant A: native HWND on a host thread. Variant B: Tauri `unstable` webview-less window. Both with DX12 visual presentation, premultiplied alpha, non-sRGB UNORM; then Vulkan. Pass: correct alpha in a GPU capture; click-through; no focus theft; no ghost after close. | Fall back to a `viz-` webview window loading an empty `viz.html`, with preflight; if still opaque, stop and reassess before Phase 1B. | PNG capture, DPI matrix, chosen variant. |
| **P0.3** | Can WASAPI loopback be captured, normalized and stopped reliably, with device-clock timestamps? | 2 days | Compare `cpal` loopback and the `wasapi` crate. 10-minute soak on the default endpoint; f32/s16/s32 accepted; 5.1 downmix defined; device switch recovers within 2 s; stop joins cleanly; QPC/device-position timestamps with < 2 ms jitter. | Add explicit conversion for unsupported formats; arrival-time timestamps marked degraded; never fall back to microphone. | CSV metrics, format matrix, timestamp jitter histogram. |
| **P0.4** | Does the DSP core match independent vectors and allocate nothing? (Track A step A6) | 2 days | Sine peak within ±1 bin; level within ±0.5 dB; silence and DC pass; 0 allocations after 20 warmups over 500 calls. | Fix the DSP before any renderer work; do not tune goldens to the implementation. | Test log, golden files. |
| **P0.5** | Do Rust encode and TS decode agree byte for byte? (Track A step A4) | 1 day | Exact round-trip; sequence wrap; `header_only`; 64-bit timestamp; wire version; unknown version rejected; no TS precision loss. | Freeze a simpler header before renderer code. | Fixture pair, parity test. |
| **P0.6** | Does the overlay lifecycle survive panic, close, DPI change and monitor unplug? | 1 day | No translucent or opaque ghost window; no orphaned surface; panic path destroys windows; DPI change reconfigures once. | Add a panic hook and Drop guard before Phase 1B. | Video/PNG evidence, crash log. |
| **P0.8** | Can a wgpu surface live behind the desktop icons as a child window, on classic and 24H2 layouts? | 2 days | Visible behind icons; icons and desktop stay clickable; Win+D keeps it; Explorer restart re-attaches within 2 s via `TaskbarCreated`. | Overlay-at-bottom fallback with a visible notice; record which layout failed. | GPU capture per layout, restart log. |
| **P0.9** | Can a standalone strip stay on the Windows taskbar without injection? | 2 days | Owned topmost non-activating strip over a free region (`SHAppBarMessage` + UI Automation); stays above after clicking the bar and Start; no focus theft; hides with auto-hide/fullscreen; edge changes followed. | Dock the strip as an overlay just above the bar; native XAML stays experimental. | Video, z-order log, region log. |
| **P0.7** | What is the real capture-to-photon latency per preset? | 1 day | Click track through the default output; 240 fps phone camera (or photodiode) on speaker + screen; 20 clicks per preset. Record p50/p99 for default (2048/512, 60 Hz Fifo) and low-latency (1024/256, highest refresh). Also log in-app `present_time - frame.timestamp`. | If default p50 > 80 ms, shorten the window or reduce present latency before Phase 1B; never hide it behind a label. | Video, per-click CSV, budget table update. |

### 4.1 Spike rules

- A spike may use a minimal window and a test pattern. It must not grow a settings UI.
- Measure the real path. A CPU rasterizer or a static screenshot is not evidence for P0.2.
- Keep the spike branch and its evidence; delete it only after the gate is recorded.
- If a spike reveals a contract problem, update this document before writing production code.

### 4.2 Cross-OS pre-spikes (Phase 1.5)

These run after the Windows overlay is credible, before Phase 2 commits to a backend.

| ID | Question | Pass gate | Fail / fallback |
| :-- | :-- | :-- | :-- |
| **P1.5a** | Can macOS capture system audio with the chosen tap/SCK path and show a permission card? | Permission denial is detectable and actionable; audio flows after grant. | ScreenCaptureKit fallback; do not silently use the microphone. |
| **P1.5b** | Can Linux capture a PipeWire monitor and attach a layer-shell/X11 overlay? | Default-sink changes recover; layer-shell background/overlay appears on a wlroots compositor; X11 fallback works. | X11-only release or in-app fallback; document GNOME limits. |
| **P1.5c** | Do mixed-DPI monitors get separate physical-pixel surfaces with correct origins? | No offset, blur or wrong-origin uniform on a 100/150% pair. | Revert to single-monitor v0.2 and make multi-monitor a later gate. |

---

## 5. Contracts to freeze before Phase 1A

### 5.1 Startup order

The order is a safety property. Implement it in Phase 1A, not after the overlay.

1. Install the panic hook.
2. Resolve portable mode and data directories.
3. Initialize session logging.
4. Parse CLI and detect headless mode.
5. Run the development preflight. In a debug build, refuse to create the Studio webview if the Vite
   entry point is not reachable. Visualizer windows are webview-less (ADR-013); if P0.2 forces the
   webview fallback, preflight also gates every `viz-` window.
6. Load config through migrate -> self-heal -> sanitize -> normalize.
7. Register the app handle with the panic hook.
8. Create the settings window and tray.
9. Create visualizer hosts hidden.
10. Start capture, analysis and render; show a surface only after its first successful frame.

A failure before step 8 may show a normal error dialog. A failure after a full-desktop window exists
must use the panic/cleanup path.

### 5.2 Capture contract

```rust
pub struct CaptureFormat {
    pub sample_rate: u32,
    pub channels: u16,
    pub layout: ChannelLayout,
    pub sample_format: SampleFormat,
}

pub trait CaptureBackend {
    fn open(&mut self, request: &CaptureRequest) -> Result<CaptureFormat, CaptureError>;
    fn start(&mut self, sink: AudioSink) -> Result<(), CaptureError>;
    fn stop(&mut self);
    fn status(&self) -> CaptureStatus;
}
```

| Concern | Decision |
| :-- | :-- |
| Rate at the API | Accept the endpoint mix format. Do not force 48 kHz at the capture boundary. |
| Analysis rate | Normalize to 48 kHz in the worker only when needed; record both rates in status. |
| Sample formats | f32, s16, s32, s24-in-32, and `WAVE_FORMAT_EXTENSIBLE` float/PCM. Reject packed 24-bit with an explicit error, not silence. |
| Channels | Preserve the channel mask. Downmix policy is explicit: mono for bars, stereo pair for width/correlation. |
| Ring | Bounded `rtrb`, preallocated, at least 2 seconds at the capture rate. On overflow the producer drops the **newest** chunk (an SPSC producer cannot pop) and increments an overrun counter; the analyzer detects the sample-position gap, resets continuity and skips forward to the newest hop. Never block the producer. |
| Timestamps | Each chunk is anchored to the device clock (WASAPI `pu64QPCPosition`, CPAL capture instant, Core Audio host time, PipeWire `pw_time`) mapped to the process monotonic clock. Arrival-time stamping is a degraded mode shown in diagnostics. |
| Silence | Publish a silent frame. Do not stop the analyzer and do not freeze the last frame. |
| Device loss | Emit `DeviceLost`; show a visible reconnect card; rebuild without restarting the app. |
| Fallback | Never fall back from system audio to microphone without explicit user consent. |

Two adapter shapes feed the same ring:

- **Push adapter:** CPAL callback validates the format and pushes interleaved f32. No allocation,
  locks, logging, resampling or FFT.
- **Pull adapter:** WASAPI event thread waits, drains packets, converts to f32, pushes. Same rules.

### 5.3 Analyzer contract

```rust
pub struct SpectrumConfig {
    pub sample_rate: u32,
    pub window_samples: usize,
    pub fft_size: usize,
    pub hop_samples: usize,
    pub output_bins: usize,
    pub window: WindowKind,
    pub weighting: Weighting,
    pub detector: Detector,
    pub level_ref: LevelRef,
    pub db_floor: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
}
```

| Setting | v1 default | Rule |
| :-- | :-- | :-- |
| `sample_rate` | 48 kHz analysis | Capture rate may differ; resample before analysis. |
| `window_samples` | 2048 | May be smaller than `fft_size` for zero-padding. |
| `fft_size` | 2048 | Even; validate at the boundary; `realfft` rejects odd sizes. |
| `hop_samples` | 512 | 75% overlap at 2048. |
| `output_bins` | `fft_size / 2 + 1` | Kept separate from `bar_count`. |
| `bar_count` | 48 | Per-bar edges, not interpolation of seven bands. |
| `window` | Hann | Blackman-Harris and Kaiser available later. |
| `level_ref` | Third-octave | Makes pink noise flat and bar height less dependent on bar count. |
| `db_floor` | -72 dB | Display floor; DSP floor may be lower. |

**State ownership:**

- Analyzer owns DC blocker, window, FFT plan, band map, weighting, EQ, auto-gain, ballistics, peak
  hold, feature state and generation.
- Renderer owns geometry, bar width/gap, corner radius, palette, gradients, glow, trails and scene
  animation.
- A display-only setting must not restart capture. A stateful DSP setting bumps `generation` and
  rebuilds at the next hop boundary; the last frame stays visible until a new one arrives.

**Real-time rules:**

- Build FFT plans off the analysis thread and swap an `Arc` plan; never plan in `process`.
- Reuse input, output and scratch buffers; no per-hop allocation after warm-up.
- Clamp `dt` to a sane range; compute attack/release from milliseconds.
- Flush denormals in ballistics/biquad state, or set FTZ/DAZ under `unsafe` on x86 with a safety
  comment and a test.
- Put Windows MMCSS, macOS QoS and Linux nice/RT priority behind a platform module. Do not raise
  priority in a library constructor.

### 5.4 Frame contract

The 8-word header is frozen in the root README. The implementation must match it exactly.

| Word | Field | Type | Notes |
| --: | :-- | :-- | :-- |
| 0 | `sequence` | u32 | Monotonic; wraps are allowed. |
| 1 | `timestamp_lo` | u32 | Low 32 bits of monotonic microseconds. |
| 2 | `timestamp_hi` | u32 | High 32 bits. |
| 3 | `sample_rate` | u32 | Analysis rate. |
| 4 | `fft_size` | u32 | Analysis FFT size. |
| 5 | `bar_count` | u32 | Bars in the payload. |
| 6 | `flags` | u32 | See below. |
| 7 | `generation` | u32 | DSP/config generation that produced the frame. |

| Flag bit | Name | Meaning |
| --: | :-- | :-- |
| 0 | `SILENT` | Frame is silent; bars are at the floor. |
| 1 | `CLIPPED` | Input clipped during this hop. |
| 2 | `STEREO` | Stereo features are valid. |
| 3 | `BINS_PRESENT` | Optional spectrum bins follow the bars. |
| 4 | `FEATURES_PRESENT` | Fixed-size feature block follows the bins. |
| 5 | `HEADER_ONLY` | Reply carries no payload. |
| 6-23 | reserved | Must be zero in v1. |
| 24-31 | `wire_version` | 8-bit wire version; v1 = `1`. A decoder rejects an unknown version with an explicit error. |

`timestamp` is the monotonic time, in microseconds, of the **newest sample in the analysis window**,
derived from the device clock (section 5.2). It is not the encode time. `present_time - timestamp`
is the in-app audio age; A/V offset and interpolation select frames by it.

Payload order: `bar_count` f32 bars, then `fft_size / 2 + 1` f32 bins if `BINS_PRESENT`, then 8 f32
features if `FEATURES_PRESENT`. Little-endian, f32 IEEE-754, no padding. v1 sets
`FEATURES_PRESENT = 0`; the feature block is reserved.

**Two transport paths, one frame type:**

- Native renderer: reads the slot ring in place. No encode, no allocation, no payload copy beyond
  the bars it draws.
- Preview or external client: encode on demand with `wire::encode(&FrameSlot, &mut [u8])` into a
  reused buffer and return it as a raw `tauri::ipc::Response`. This is where the header,
  `header_only` and binary IPC apply.
- The renderer and the preview must decode the same logical frame; a wire-only field is a bug.

**Slot-ring contract** (replaces the earlier `Arc<Frame>` mailbox):

- 64 preallocated fixed-capacity slots (max 128 bars, 4,097 bins, 8 features), allocated once at
  engine start. Never allocate after warm-up.
- The writer marks a slot's sequence odd, writes, marks it even (seqlock), then stores the atomic
  `latest` index. Readers retry if the sequence changed during the read. No locks.
- A reader that sees the same sequence does not redraw; the preview receives a header-only reply.
- History (about 680 ms) serves A/V offset and interpolation (ADR-016).
- A generation change invalidates interpolation and palette crossfades, not the audio stream.
- Status (DSP time, overruns, device changes, permission state, idle state) goes on a separate
  `arc-swap` snapshot refreshed at most every 250 ms.

### 5.5 Host contract and state machine

```rust
pub trait SurfaceHost {
    fn attach(&mut self) -> Result<(), HostError>;
    fn resize(&mut self, size: PhysicalSize) -> Result<(), HostError>;
    fn present(&mut self) -> Result<PresentResult, HostError>;
    fn set_visible(&mut self, visible: bool);
    fn health_check(&mut self) -> HostHealth;
    fn detach(&mut self);
}
```

| State | Meaning | Exit |
| :-- | :-- | :-- |
| `Detached` | No window or surface. | `attach` on mode start or monitor add. |
| `Attaching` | Host discovered, window created hidden. | First successful frame -> `Live`; error -> `Degraded`. |
| `Live` | Surface presenting. | Health failure -> `Degraded`; config change -> `Rebuilding`. |
| `Degraded` | Last known-good content or fallback; error visible. | Retry with backoff; user action; `Failed`. |
| `Rebuilding` | Surfaces recreated after monitor/DPI/host change. | Success -> `Live`; repeated failure -> `Failed`. |
| `Failed` | Host unavailable; overlay/in-app fallback offered. | User retry or mode switch. |

- Health check cadence: about 2 s for Background, 1 s for monitor topology, never per frame.
- Backoff: 2 s doubling to 32 s, then hold; reset after a successful frame.
- Show only after the first frame. Hide before destroying the surface.
- Every host has a Drop guard and a panic-path cleanup. A transparent always-on-top window must not
  outlive its renderer.

### 5.6 Config and scene contract

- One `AppConfig` with `schema_version`; one `Scene` with `format_version`; one `ShareCode` with a
  version and a machine-specific exclusion list.
- Load: migrate -> self-heal missing fields -> sanitize NaN/range -> normalize. Migration runs before
  clamping, or old values become dead settings.
- Save: temp file + rename, 400 ms debounce, flush on exit. Never edit the live file in place.
- Unknown fields: preserve them in a sidecar only if forward compatibility is required; otherwise
  ignore them and rewrite normalized. Decide in ADR-007 before Phase 1A.
- A corrupt preset is skipped individually; it does not take down the preset list.
- Share codes exclude device IDs, monitor IDs, paths, hotkeys, GPU details and window positions.

### 5.7 Error model

Capture errors are typed and visible: `PermissionDenied`, `DeviceNotFound`, `UnsupportedFormat`,
`DeviceLost`, `BackendUnavailable`, `Timeout`. The UI maps each to a card with a fix action. Logging
is aggregated: one line per state change or per session, never per packet or per FFT.

---

## 6. Numeric and behavioral acceptance gates

These are initial gates. Tune only with a documented reason and a new golden file.

| Test | Signal / setup | Pass gate |
| :-- | :-- | :-- |
| Sine peak | 1 kHz at 48 kHz, 2048 FFT, Hann | Peak within ±1 bin of the expected bin. |
| Sine level | Full-scale sine at a bin center | Level within ±0.5 dB of the reference. |
| Silence | Digital zero for 10 s | Bars at floor, `SILENT` set, no NaN/Inf. |
| DC | Constant 0.5 input | DC blocker attenuates below -60 dB. |
| Nyquist | Tone near Nyquist | Top bin present, no aliasing into low bands. |
| Pink noise | Calibrated pink noise, third-octave reference | Flat within ±1.5 dB from 100 Hz to 10 kHz. |
| Impulse | Single-sample impulse | Broadband response, no NaN, no overflow. |
| Ballistics | Step input, sampled at 30/60/120 fps | Same normalized value at the same wall-clock time, within 1e-3. |
| Allocation | 20 warm-up + 500 steady-state hops | Zero allocations after warm-up. |
| Frame parity | Rust encode -> TS decode | Byte-identical fields; correct sequence wrap and header-only. |
| Capture formats | f32, s16, s32, s24-in-32, 1/2/6 channels | Correct conversion; unsupported format is an explicit error. |
| Device loss | Unplug default endpoint during playback | Visible error within 2 s; recovery after reselect; no deadlock. |
| Callback | 10-minute soak with music and silence | p99 < 0.5 ms; zero locks/allocations; overruns counted. |
| DSP | 2048/512 at 48 kHz | p99 < 2 ms on the named test machine. |
| Renderer alpha | Offscreen and GPU capture | Premultiplied invariant holds (`rgb <= alpha`); no opaque corners. |
| Idle | Silent, settled, overlay visible | < 1% CPU; render loop parked; no per-frame FFT. |

---

## 7. Failure matrix

| Trigger | Required behavior | Evidence |
| :-- | :-- | :-- |
| Vite/dev server absent at launch | Preflight exits before any full-desktop window. | Log + no window. |
| System-audio permission denied | Error card with a fix action; no microphone fallback. | Screenshot + state test. |
| Device unplug or default-endpoint change | Silent frame, visible reconnect state, rebuild; old surface stays until new frame. | Fault-injection log. |
| Unsupported mix format | Explicit error; conversion path if supported, otherwise user guidance. | Format matrix. |
| Ring overflow | Drop oldest, increment counter, never block. | Overrun counter. |
| Analyzer panic | Publish silence, log once, restart analyzer; renderer remains alive. | Panic test. |
| Render thread panic | Panic hook destroys windows before abort; no ghost surface. | Video capture. |
| Surface lost/outdated | Reconfigure once; repeated failure -> fallback/error. | GPU capture. |
| Monitor hotplug/DPI change | Rebuild targets; audio continues; no per-frame query. | Mixed-DPI test. |
| Explorer restart | Host health detects; rebuild; overlay fallback if needed. | 24H2 test. |
| Session lock/sleep/resume | Pause presentation; resume rebuilds if needed. | Manual matrix. |
| GPU device lost | Recreate device/surfaces; if repeated, visible error. | Fault injection. |
| Corrupt config | Heal fields, back up the original, continue with defaults. | Partial-store test. |
| Invalid scene/share code | Reject with a message; no partial apply. | Fuzz/negative tests. |
| Timestamp/sequence wrap | No visual glitch; interpolation continues. | Unit test. |
| Visuals lead audio (Bluetooth/high-latency output) | A/V offset setting selects older frames from the slot ring; diagnostics show the output latency when the OS reports it. | Manual check with a Bluetooth headset. |
| Unknown wire version | Preview shows a "protocol mismatch" state; no garbage bars. | Parity test with version 2 fixture. |
| Device-clock timestamps unavailable | Fall back to arrival time, mark latency diagnostics as degraded. | Capture fault test. |
| Every surface click-through and the Studio closed | Global hotkey and tray item restore edit mode or hide all; never require the overlay itself. | Manual check with only the tray. |
| Endpoint muted | `Muted` state shown; render parks after drawing it. | Mute toggle test. |
| Taskbar raised above the strip | WinEvent re-asserts z-order within one event; repeated loss -> overlay fallback with notice. | P0.9 video. |
| Taskbar auto-hides / moves edge / changes size | Strip hides with it or re-anchors; no orphan strip floating mid-screen. | Manual matrix. |
| Background region covered by maximized/fullscreen window | Pause that surface; resume on uncover. | Manual check. |
| Dependency update fails validation | Restore every manifest and lockfile from the snapshot; report the failing step and the held set. | Forced-failure test of `update-deps.ts`. |

---

## 8. Phase 1A execution order

Phase 1A is intentionally invisible. It builds the contracts and the test rig, not the visualizer.
The ordered steps (Track A, A1-A10) and the parallel spikes (Track B) are in
[`phase-1a-blueprint.md`](phase-1a-blueprint.md) section 9. The mapping from the earlier numbering:

| Earlier step | Now | Notes |
| :-- | :-- | :-- |
| 1A.1 workspace, shell, scripts | A1 + A2 | A1 also closes P0.1. |
| 1A.2 `av-protocol` | A4 + A5 | Wire first (closes P0.5), then config. |
| 1A.3 `av-dsp` | A6 | Closes P0.4. |
| 1A.4 `av-audio` | A7 | File source + new `av-engine` crate; WASAPI stays in spike P0.3. |
| 1A.5 `av-render` | A9 | Headless only. |
| 1A.6 `av-platform` | A10 | Trait, state machine, monitors; no attachment. |
| 1A.7 commands, preflight, startup | A1 + A3 | Startup order is built in A1, not retrofitted. |
| 1A.8 preview | A8 | Raw `ipc::Response` pull, 30 Hz cap. |
| 1A.9 CI, bindings check, benches | A2 + A3 + A6 | |
| 1A.10 exit review | Phase 1A exit | Adds ADR-013/014/016 to the required ADRs. |

Do not add host attachment, taskbar code, media, lyrics or HDR to Phase 1A.

---

## 9. Phase 1B execution order

Phase 1B is the first usable build. Each step adds one variable.

| Step | Deliverable | Gate |
| :-- | :-- | :-- |
| **1B.1** | File/WAV -> analyzer -> preview, deterministic timestamps. | Golden frames match. |
| **1B.2** | WASAPI loopback -> ring -> analyzer -> preview, no overlay. | P0.3 soak and format matrix pass. |
| **1B.3** | Transparent webview-less overlay host (ADR-013 variant) with bars on one monitor. | P0.2/P0.6 pass; GPU capture attached. |
| **1B.3a** | A/V offset and frame selection by timestamp; latency readout in diagnostics. | In-app audio age (capture to present) recorded next to the P0.7 camera numbers; the gap is explained (scanout, output device). |
| **1B.4** | Device picker, permission/error cards, reconnect. | Fault-injection matrix passes. |
| **1B.5** | Settings studio, scene save/load, atomic saves. | Migration/partial-store tests pass. |
| **1B.6** | Idle state machine: audible, trickle, deep, covered, battery. | Idle < 1% CPU; wake latency measured. |
| **1B.7** | Tray, diagnostics, log rotation, copy diagnostics. | Manual check; no secrets/paths. |
| **1B.8** | Real-GPU capture harness and 10-minute soak. | Evidence bundle complete. |
| **1B.9** | Exit review for v0.1. | All Phase 1B gates; no multi-monitor requirement yet. |

Sequence rules:

- Do not add multi-monitor until the single-monitor overlay is visually verified.
- Do not add Background or Taskbar until the overlay is stable, measured and shippable.
- Do not add media/lyrics until the visualizer works without them.

---

## 10. Stop/go rules

| Condition | Action |
| :-- | :-- |
| P0.2 fails after the timebox | Stop; evaluate a DComp presenter or a different Tauri window strategy. Do not build Background on an unverified overlay. |
| P0.3 cannot capture a common format | Add a conversion path and retest; do not ship a silent failure or a microphone fallback. |
| P0.4 misses the DSP budget | Reduce FFT/hop or optimize before any renderer complexity. |
| P0.5 finds a header mismatch | Freeze the protocol before Phase 1A; do not patch both sides independently. |
| Overlay alpha is correct but pacing is wrong | Fix pacing per target; do not add a second deadline to a blocking acquire. |
| Idle CPU misses the budget | Park earlier, stop loopback in deep idle, reduce analysis rate; do not hide it in a lower FPS label. |
| Tauri 3 family cannot resolve | Hold the last known-good family and file the failure; do not downgrade to Tauri 2. |
| P0.7 default latency p50 > 80 ms | Shorten the analysis window or the present queue before Phase 1B; publish the measured number, not the estimate. |
| Neither ADR-014 path builds on the current Tauri 3 alpha | Hold the last family where it builds; do not hand-write untyped `invoke` strings as a workaround. |
| Webview-less window cannot composite (P0.2 A and B fail) | Use the `viz-` webview fallback with preflight and an empty page; record the reason in ADR-013. |

---

## 11. ADR queue

| ADR | Question | Deadline | Recommended default |
| :-- | :-- | :-- | :-- |
| **ADR-001** | `panic = "unwind"` or `"abort"`? | Before 1A | Unwind, so Drop guards work; revisit only with a tested panic hook. |
| **ADR-002** | Capture at mix format or request a fixed rate? | Before 1B | Mix format at the API; resample in the worker. |
| **ADR-003** | In-process frame sharing: per-hop `Arc<Frame>`, latest-only triple buffer, or a preallocated slot ring? | Before 1A | 64-slot seqlock ring, read in place by the renderer; wire encode only for preview/clients. `Arc<Frame>` per hop is rejected (allocation). |
| **ADR-004** | Pacing per host: Fifo, Mailbox or carried deadline? | Before 1B | Fifo for the pacing overlay; Mailbox/carried deadline for slower/background targets. |
| **ADR-005** | Which transforms are analyzer-stateful? | Before 1A | Ballistics, peak hold, auto-gain and EQ in the analyzer; geometry/color in the renderer. |
| **ADR-006** | Host fallback ladder and visible error states. | Before 2 | Background -> overlay -> in-app; taskbar -> overlay; never a silent mode change. |
| **ADR-007** | Config unknown-field policy. | Before 1A | Ignore unknown fields, rewrite normalized; preserve only where a migration needs them. |
| **ADR-008** | Final name and licence. | Before v0.1 | Working title until the release candidate; MIT or Apache-2.0. |
| **ADR-009** | Windows Background presenter. | Before 4 | Standard wgpu surface first; DComp only after a measured need. |
| **ADR-010** | Windows taskbar target. | Before 5 | Adjacent owned HWND first; native XAML experimental. |
| **ADR-011** | macOS capture minimum. | Before 2 | 14.2 tap; ScreenCaptureKit fallback on 13+. |
| **ADR-012** | Linux baseline. | Before 2 | X11 first; wlroots layer-shell where available; GNOME in-app fallback. Decide Tauri `gtk3` vs `gtk4` feature at the same time. |
| **ADR-013** | Who owns visualizer windows: Tauri webview windows, Tauri `unstable` webview-less windows, or native windows in `av-platform`? | Before 1B (P0.2) | Native webview-less windows in `av-platform`; Studio is the only webview. Tauri `unstable` window is the second choice; a `viz-` webview with an empty page is the last resort. |
| **ADR-014** | Typed IPC on Tauri 3: is `tauri-specta` needed, and from where? | Before 1A (A3) | **Needed** (types alone leave ~25 command wrappers, events and drift checks hand-written). Upstream is Tauri 2 only, so use `NairoDorian/tauri-specta-v3` **rebased onto upstream rc.27**, retargeted to Tauri 3.0.0-alpha.6, pinned by git `rev`, specta family pinned to what the fork pins, `ErrorHandlingMode::Result` set explicitly. All use contained in `src-tauri/src/ipc/`. Exit: switch to crates.io when a release declares `tauri ^3`; the updater reports it. Fallback if the retarget fails: types-only `specta` export with a hand-listed command table and a manifest test. |
| **ADR-015** | Preview transport: pull per rAF via `ipc::Response`, or push via `ipc::Channel`? | Before 1A (A8) | Pull, capped at 30 Hz, header-only when unchanged, paused when hidden. Push channels only if measurement shows pull overhead matters. |
| **ADR-016** | A/V sync and interpolation policy. | Before 1B | Renderer selects frames by `timestamp` from the slot ring with a user A/V offset (default 0 ms, range -100..+400 ms). Interpolate only when the display rate exceeds the analysis rate; it costs up to one hop (10.7 ms). |
| **ADR-017** | Latency presets and targets. | After P0.7 | Default 2048/512 at 60 Hz Fifo; Low-latency 1024/256 at the panel's top rate; Precision 8192/1024 at 30 Hz. Targets are set from P0.7 numbers, not estimates. |
| **ADR-018** | How far "above everything" goes for the overlay. | Before 6 | v1: `HWND_TOPMOST` with stated limits. Later: optional `uiAccess="true"` build (signed, protected install path) to sit above the Start menu. Never undocumented `CreateWindowInBandEx`/`SetWindowBand`. |
| **ADR-019** | Dependency policy and updater design. | Before 1A (A2) | Always newest (prerelease or latest, whichever is newer); no stable mode; holds only in `scripts/update-policy.ts` with reasons; updater ported from ZER0 + CursorFX with family transaction, lockstep groups and transactional rollback. Rust toolchain on newest stable as the one documented exception. |

---

## 12. Evidence artifacts

Store evidence outside the source tree or under a git-ignored `evidence/` directory:

```text
evidence/
  2026-10-22/
    windows-11-24h2-rtx4070/
      p0.2-overlay/
        capture.png
        frame-times.json
        notes.md
      p0.3-wasapi/
        format-matrix.json
        soak.csv
```

Every artifact records: app commit, OS build, GPU/driver, display mode, DPI, power state, test
command and pass/fail. A performance number without a machine and a command is not evidence.

---

## 13. Recommended defaults for open questions

These are not final ADRs; they are the defaults the spikes should test.

1. **Bars:** 48 log-spaced bars, max 128; per-bar edges with third-octave level reference.
2. **FFT:** 2048 window / 2048 FFT / 512 hop at 48 kHz; 8192/7168 hop only in Precision mode.
3. **Capture:** endpoint mix format; f32/s16/s32/s24-in-32; normalize in the worker.
4. **Ring:** 2 seconds, preallocated; drop the newest chunk on overflow, count it, and let the
   analyzer skip forward.
5. **Transport:** 64-slot seqlock frame ring in-process; binary wire encode into a reused buffer
   only for preview/clients.
6. **Pacing:** Fifo for the overlay's pacing surface; Mailbox or carried deadline for slower
   surfaces; no double gate.
7. **State:** analyzer owns all time-dependent transforms; renderer owns visual-only state.
8. **Stereo:** preserve stereo in capture; defer width/correlation rendering to Phase 3.
9. **Scene format:** JSON with `format_version`; share code is compressed JSON with a version and
   machine-specific exclusions.
10. **Background:** WorkerW classic and 24H2 Progman layouts; standard wgpu surface first.
11. **Taskbar:** adjacent owned HWND; native XAML behind an experimental flag.
12. **HDR:** Phase 6; do not advertise until a real display path is validated.
13. **Idle:** silence -> trickle -> deep; stop loopback in deep idle; poll a meter at 4 Hz.
14. **Logging:** session log with rotation; aggregate overruns/device changes; no per-frame logs.

---

## 14. What not to build yet

- No taskbar injection, no Explorer hooks, no global input hooks.
- No virtual audio driver, no per-app capture in v0.1.
- No media sessions, lyrics, album art, HDR, 3D scene graph or particle system.
- No GPU compute analysis before the CPU path is measured and stable.
- No second settings store, no second frame format, no second renderer.
- No "just one more" host before the overlay has a GPU capture and a 10-minute soak.
