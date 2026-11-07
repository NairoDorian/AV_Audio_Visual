# AV_AudioVisual

> **Working title.** A cross-platform audio visualizer that lives where you want it: behind your
> desktop icons, in the taskbar or menu bar, or in a floating overlay panel. It listens to the
> system output by default, optionally to the microphone, and turns what you hear into a live,
> configurable visual.

**Status: Phase 0 complete; plan third pass done (2026-11-07).** The reference library, 14 source
studies and a cross-project synthesis are in [`references/README.md`](references/README.md). The
audits, de-risking spikes, contracts and acceptance gates are in [`docs/pre-plan.md`](docs/pre-plan.md).
The buildable workspace (verified versions, crate graph, threads, scripts, ordered tasks) is in
[`docs/phase-1a-blueprint.md`](docs/phase-1a-blueprint.md). No application code has been scaffolded
yet. The next step is Phase 1A Track A step A1 (scaffold) with the Phase 0.5 platform spikes in
parallel.

The name may gain a suffix later (for example `AV_Panel` or `AV_Background`). The product
requirements do not depend on the final name; the working title is used throughout the code and
documentation until a name is chosen.

---

## 1. Product definition

AV_AudioVisual is a desktop audio-visualizer suite, not a single window with bars. The visual lives
in **three places**, which are all part of the v1.0 product:

1. **Background / wallpaper:** behind the desktop icons, as if it were the wallpaper.
2. **Taskbar / menu bar:** a compact strip living on the shell's bar.
3. **Overlay:** a floating panel above everything else that can be moved, resized, snapped and
   made click-through.

One audio engine and one scene format drive all three, plus a live preview in the Studio. **Any
combination can run at the same time**, each with its own scene, all fed by the same analysis
worker (one FFT per source, never one per window).

| Mode | What it looks like | Windows | macOS | Linux |
| :-- | :-- | :-- | :-- | :-- |
| **Background** | Visualizer behind desktop icons, wallpaper-like | `WorkerW`/`Progman` child | Desktop-level `NSWindow` | X11 desktop type / Wayland `layer-shell` background |
| **Taskbar / Menu Bar** | Compact strip on the shell's bar | Owned topmost strip over a free region of the taskbar; native taskbar surface experimental | Menu-bar extra (`NSStatusItem`) | Panel applet or `layer-shell` strip |
| **Overlay** | Floating transparent panel above everything, movable and resizable, optional click-through | Webview-less `WS_EX_NOREDIRECTIONBITMAP` topmost window with a composition swap chain | `NSPanel` at status level | Layer-shell `overlay` / X11 override-redirect |
| **Preview** | Live preview inside the Studio | Canvas over raw frames | Same | Same |

### 1.0 Geometry and interaction per placement

"Resizable and movable" means something different in each place. The model below is the contract
for `av-platform` and for scene geometry; details are in
[`docs/phase-1a-blueprint.md`](docs/phase-1a-blueprint.md) section 10.

| | Background | Taskbar / Menu Bar | Overlay |
| :-- | :-- | :-- | :-- |
| **Placement** | Per monitor: full monitor, spanning all monitors, or a placed region (for example a band along the bottom of one screen) | A segment of the bar: anchored left, centre-gap, right or free offset, with a width; follows the bar's edge (bottom/top/left/right) and orientation | Anchored to a monitor's **work area**: a corner, an edge centre or free, plus an offset and a size, in DIPs |
| **Move / resize** | Through **layout edit mode**: a temporary interactive frame appears over the region; drag/resize, then commit. The background itself never takes input. | Drag along the bar in layout edit mode; width handles; snaps to free regions | Directly: drag anywhere in edit mode, edge/corner resize handles, snap to work-area corners/edges, arrow-key nudge |
| **Input in normal use** | None. Icons and the desktop stay clickable. | None by default (clicks reach the taskbar); optional hover/click for a menu | Locked = fully click-through; Edit = drag/resize/hot zones |
| **Z-order** | Under the icons, above the wallpaper | Above the taskbar band, re-asserted when the taskbar is raised | Topmost (see the limits below) |
| **Hides when** | Covered by fullscreen or maximized windows on that monitor (pause rendering) | The bar auto-hides, a fullscreen app takes the monitor, or the bar is unavailable | User toggle, fullscreen policy, or capture exclusion setting |
| **Fallback** | Overlay on the same region, behind normal windows, with a visible notice | Overlay docked next to the bar | In-app window |

Rules that apply to all three:

- Geometry is stored as an **anchor**, not as absolute pixels: monitor ID + anchor + offset + size
  in DIPs, re-resolved on display, DPI, work-area and taskbar changes (Audio HUD study, section
  4.6). Saved on `WM_EXITSIZEMOVE`-equivalents, never per move.
- **Escape hatches:** a global hotkey and a tray item to show/hide every surface and to toggle
  click-through, from v0.1. A click-through overlay with no way back is not acceptable.
- **Layout edit mode** is one feature shared by all three: it shows interactive frames, snapping
  guides and a "Done" control, and leaves the surfaces locked afterwards.
- **"Above everything" has OS limits.** A topmost window sits above normal and other topmost
  windows and above borderless-fullscreen games, but not above the Start menu, the secure desktop,
  exclusive-fullscreen apps or UI-access windows. Going above the Start menu needs the documented
  `uiAccess` route (signed binary, protected install location), decided in ADR-018. The overlay
  can optionally be excluded from screen capture (`WDA_EXCLUDEFROMCAPTURE`), or kept visible to it
  for streamers; this is a setting, default visible.

### 1.0.1 Delivery order

All three placements are designed into the host abstraction from Phase 1A and **de-risked by
spikes in Phase 0.5** (P0.2 overlay, P0.8 background, P0.9 taskbar), so none of them can force an
architecture change later. They **ship** in risk order: overlay first (v0.1; it exercises capture,
DSP, transport, renderer, settings and lifecycle without undocumented shell internals), then
Background (v0.4, the signature mode), then Taskbar/Menu Bar (v0.5, the hardest integration).

### 1.1 Success criteria

1. **Audible truth.** The bars measure what the user hears, with per-bar frequency edges and
   calibrated levels, not seven interpolated bands.
2. **Low latency, honestly measured.** Capture-to-photon latency is measured per preset, not
   assumed (section 6.3). The low-latency preset targets under 45 ms; the default preset trades
   some latency for resolution and targets under 80 ms until P0.7 replaces the estimate with a
   number. Visuals can be delayed to match high-latency outputs such as Bluetooth.
3. **Low idle cost.** Silence and occlusion lead to parked render/frames and a measurable drop in
   CPU/GPU use.
4. **Host safety.** A visualizer crash must not take down Explorer, Finder or the compositor. There
   is no injection into shell processes.
5. **Cross-OS honesty.** Each platform exposes what it can actually do, with a visible fallback and
   error state.
6. **Portable scenes.** A scene carries visual intent, not a machine's audio device, monitor,
   hotkeys or paths.
7. **Maintainable core.** Capture, DSP, protocol, render and platform hosts stay separate. No god
   modules.

### 1.2 Non-goals for v1

- No kernel driver or mandatory virtual audio cable.
- No injection into Explorer, Finder or a compositor.
- No mobile build.
- No GPL code copied into the product.
- No cloud account, telemetry backend or always-on network dependency.
- No 3D scene graph before the 2D renderer is stable.
- No media/lyrics integration before the core visualizer ships.

---

## 2. Scope and priorities

The references contain enough features for several products. The plan deliberately separates the
core claim from the long tail.

### 2.1 Feature priority

| Area | P0: v0.1 (must ship) | P1: v0.2-v0.4 | P2: v0.5+ / later |
| :-- | :-- | :-- | :-- |
| Audio sources | WASAPI loopback, CPAL microphone | macOS tap/ScreenCaptureKit, PipeWire monitor, default-device changes | Per-process capture, mixed sources, file playback as a first-class source |
| Analysis | 2048/75% real FFT, 48 per-bar bands, Hann, dB, time ballistics, auto-gain | Multi-tier bass, A/C/Z weighting, EQ, stereo features, BS.1770 loudness | GPU compute analysis, true-peak metering, advanced psychoacoustic layouts |
| Visualizers | Bars, line, area, mirrored bars | Radial, waveform, spectrogram/waterfall, VU/peak meter scene with mic + system side by side (Audio HUD) | Goniometer, particles, trails, bloom, reflections, 3D scenes |
| UX | Settings studio, scene save/load, tray, diagnostics, **show/hide and click-through hotkeys + tray escape hatches**, overlay move/resize/snap, muted state | Share codes, per-monitor scenes, layout edit mode for all placements, configurable hotkeys | Lyrics, media cards, onboarding library |
| Hosts | Single-monitor transparent overlay (spikes prove Background and Taskbar feasibility) | Multi-monitor overlay, Background mode, Taskbar strip | Native Windows taskbar surface, menu-bar extra polish, uiAccess z-band (ADR-018) |
| Media | None | Album art and deterministic palette | SMTC/MPRIS/Now Playing, lyrics, transport |
| Platform | Windows | macOS, Linux, HDR deferred | Packaging, updater, portable mode |

### 2.2 MVP cut: Slice 1

The first usable build does not yet ship all three placements (they are proven by spikes P0.2,
P0.8 and P0.9 before it, and ship in v0.4/v0.5). It must prove the core claim end to end:

- system audio in, per-bar spectrum out, transparent overlay on screen;
- settings persisted and a scene saved/loaded;
- no virtual audio driver;
- no per-frame allocation in capture/DSP/render;
- no webview as the primary 60 fps renderer;
- no JSON spectrum frames;
- measurable idle cost when silent;
- a real-GPU capture that proves the output.

If Slice 1 is not excellent, Background and Taskbar multiply the problems instead of validating
them.

### 2.3 Deferred on purpose

- **Taskbar XAML injection** is not the first Windows taskbar implementation. An adjacent owned
  HWND is portable, debuggable and does not depend on private `taskbar.dll` symbols.
- **Lyrics and media cards** are a later product layer. They add network, permissions and UI
  surface without improving the audio path.
- **GPU analysis** is an optimization, not a dependency. CPU DSP is easier to verify and remains the
  reference implementation.
- **3D and particle systems** wait for the 2D renderer, scene format and visual test harness.

---

## 3. What the research changed

The full reconciliation is in
[`references/studies/00-cross-project-synthesis.md`](references/studies/00-cross-project-synthesis.md).
The short version is below.

### 3.1 Ten consensus rules

1. **The audio callback is real-time.** No allocation, locks, logging, FFT or blocking sends.
   ZER0's callback/ring split and the FFT plugin's allocation gate support this directly.
2. **One analysis worker per active source.** Every window reads the same latest frame. Never run
   one FFT per window.
3. **Per-bar measurement beats seven-band interpolation.** Tourne'Table's Precision model and
   Neiz's measured bands are the positive evidence; Desktop AV's seven bands are the anti-pattern.
4. **Ballistics are time-based, not frame-count-based.** The same preset must feel identical at 30,
   60, 120 and 144 fps.
5. **Analysis cadence is independent of display cadence.** A 512-sample hop at 48 kHz gives 93.75
   analysis frames/s; a 60 fps display samples and interpolates.
6. **Frames are binary.** Use an 8-word header, a sequence number, generation and `header_only`
   replies. Send status on a separate low-rate channel.
7. **Transparent windows are a lifecycle problem.** Preflight, Drop guards, surface release order,
   per-monitor uniforms and main-thread geometry hops are correctness requirements.
8. **One surface per monitor.** Use physical-pixel geometry, a stable monitor ID and a per-target
   uniform buffer.
9. **Idle, occlusion and silence are product features.** Park after settle; publish silent frames;
   stop loopback in deep idle; pause when covered or on battery policy.
10. **Hosts are optional and must fail visibly.** WorkerW, taskbar internals and desktop-level
    windows are not portable contracts. Provide a fallback and an error card.

### 3.2 Rulings on disagreements

| Topic | Ruling |
| :-- | :-- |
| Default FFT | **2048-point, 512-sample hop (75%)** for 60 fps. Optional **8192/87.5% Precision** at 30 fps. 1024 for low power. |
| Bars | **Per-bar edges**, default 48 bars, max 128. Seven-band mode remains a compatibility preset. |
| Analysis rate | Up to **93.75 Hz** at 48 kHz; display at 60 fps; interpolate; never run FFT per refresh. |
| Smoothing | **Time-based** attack/release with a clamped `dt`. |
| Renderer | **Native wgpu** for all visualizer surfaces. SolidJS canvas is a preview only. |
| Transparency | **Premultiplied alpha + non-sRGB UNORM** on Windows. DComp is a later presenter optimization. |
| Background | **WorkerW/Progman child**, desktop-level `NSWindow`, X11/layer-shell. No wallpaper replacement in v1. |
| Taskbar | **Adjacent HWND / status item / layer-shell strip** first. Native Windows XAML is experimental. |
| GPU analysis | **CPU first**, GPU later behind a measured threshold. Never depend on a delayed readback for visuals. |
| Config | One versioned `AppConfig`/`Scene`; UI-only state separate. |
| Modules | Separate capture, DSP, protocol, render and platform crates. No god modules. |
| Motion | Three motions: panel show/hide, monitor move/detach, idle breathing. |
| Media | Deferred to Phase 6. Core visualizer first. |
| Tauri | **Tauri 3, newest 3.x**, family pinned and validated as a set. |
| Visualizer windows | **Webview-less native windows** owned by `av-platform`; the Studio is the only webview. Removes the opaque-error-page hazard at the source (ADR-013). |
| Frame sharing | **Preallocated 64-slot seqlock ring**, not `Arc<Frame>` per hop (which allocates). History enables A/V offset and interpolation (ADR-003, ADR-016). |
| Typed IPC | **`tauri-specta` is needed** (typed commands, events, error unions, drift checks). Upstream still targets Tauri 2, so the fork `tauri-specta-v3` is rebased onto upstream rc.27 and retargeted to Tauri 3; pinned by `rev`, dropped when upstream ships Tauri 3 support (ADR-014, blueprint 6.1). |
| Latency | Targets per preset, measured by P0.7. Interpolate only when the display is faster than the analysis rate. |

### 3.3 Reference ledger

This is the compact ledger. The full 17-entry version, including GPL and duplicate handling, is in
the [cross-project synthesis](references/studies/00-cross-project-synthesis.md).

| Reference | What enters the plan | What stays out |
| :-- | :-- | :-- |
| CursorFX | Per-monitor wgpu surfaces, per-target uniforms and pacing, settle/park, preflight, Drop guards, config pipeline, GPU capture | Cursor hooks, cursor physics, trail effects |
| ZER0 audio | Callback -> `rtrb` -> worker, binary 8-word frames, `header_only`, latest-frame bus, generation counters, status split | 16 kHz speech path, 30 Hz-only tuning, mono analysis |
| ZER0 app shell | Programmatic windows, main-thread hops, tray snapshots, typed Specta IPC, schema migrations, portable `Data/` layout, identity gates | 4,057-line settings module, 3,249-line actions module, patched forks |
| Tourne'Table | Per-bar Precision bands, three resolution tiers, BS.1770/true peak, time ballistics, frame-hash skip, deep idle | 19,934-line translation unit, Windhawk injection, private taskbar path, versionless share format |
| Desktop Audio Visualizer | Minimal WASAPI loopback, WorkerW child, cached wallpaper blur, fullscreen pause | Seven-band interpolation, frame-dependent smoothing, always-rendering silence; unlicensed |
| Neiz | 32-band EQ, auto-gain, frame-independent smoothing, SMTC, OKLab palette, per-monitor throttle | 26,775-line monolith, private taskbar symbols, GPU path that silently drops styles |
| Island Media Controls | Taskbar geometry, focusless hit testing, theme/accent, reserved space | Private `taskbar.dll` symbols in v1 |
| Desktop Live Overlay | WorkerW/Progman host, DComp layering, click-through child | GPL-3.0 code |
| Calendar Live Overlay | Desktop lifecycle, minute-aligned repaint | GPL-3.0 code |
| Fluid Wallpaper | Stable-fluids solver, low-res velocity/high-res dye, idle pacing | Full-screen simulation as default; study-only mod code |
| Parallax Wallpaper | Critically damped spring, dead zone, perspective cover, scRGB HDR mapping | Pointer-only scope; reuse without MIT notice |
| Live Video Wallpaper | Occlusion profiler structure, pause on hidden/covered/battery | GPL-3.0 code; video is not v1 |
| WinDuo | Capture exclusion, spring state machine, lid/session handling | Camera dependency; Apache NOTICE must be preserved |
| Flip 3D | Transient overlay, WGC previews, springs, focusless input | Global Alt+Tab takeover, keyboard hooks, injection |
| CoverTabby | Two-buffer pools, snapshot cache, drag handoff | GPL-2.0-only code |
| FFT plugin | Zero-allocation gate, key-cached rebuilds, off-thread FFT plans, golden vectors, rate/axis split | TouchDesigner glue, FFTW/oneMKL ABI, wisdom files |
| MAC_OS_ANIM | Show/hide state machine, time-remapped reversal, smootherstep, cubic ease-out, damped spring | Hooks, DWM thumbnails for our own panel, 20-engine enum |
| Audio Level HUD | Endpoint peak meter as zero-capture wake/VU signal, muted-is-not-silent, NOREDIRECTIONBITMAP + DComp premultiplied window, `DWMWCP_DONOTROUND`, locked vs edit hit testing, hotkey escape hatches, work-area anchors | Per-frame brushes, timer pacing without park, stretched fixed-size swap chain, private UI-access band, name-substring device matching; no licence (study only) |
| CAVA | Cross-OS capture inventory, default-method ordering, `auto` semantics, log-band + auto-gain shape, raw output idea | FFT under the capture mutex, display-driven hop, exit-on-failure; cavacore/FFTW linkage (GPL taint) |
| GLAVA | Named multi-pass shader graph, audio-texture upload, hot reload, EWMH desktop presets | Dead GPU FFT, render-thread FFT, GPL-3.0 code, shaders and presets |

### 3.4 Product thesis

The references point to a product that is **measurement-grade and cheap at rest**:

- **Truth:** per-bar measurement, calibrated levels and honest loudness labels.
- **Presence:** native surfaces in the three places a desktop visualizer belongs.
- **Restraint:** no shell injection, no virtual driver, no per-frame UI churn, no 60 Hz event storm.
- **Portability:** one scene format, one engine, three hosts.
- **Evidence:** every visual and performance claim has a capture or a benchmark.

The differentiator is not "more effects". It is a visualizer that looks correct, behaves like a
native desktop citizen, and stays quiet when there is nothing to show.

---

## 4. Architecture

### 4.1 Stack

Versions were probed on 2026-11-07; the full snapshot and the registry traps are in
[`docs/phase-1a-blueprint.md`](docs/phase-1a-blueprint.md) section 1. `bun run update` moves them
forward.

| Layer | Choice | Newest at plan time | Notes |
| :-- | :-- | :-- | :-- |
| App shell | **Tauri 3 (newest 3.x)** | `tauri` 3.0.0-alpha.6, CLI 3.0.0-alpha.6, `@tauri-apps/api` 3.0.0-alpha.3, plugins 3.0.0-alpha.3 | Family resolved from `tauri`'s own ranges (members have different numbers). Plugins from crates.io, not git. Never Tauri 2. |
| Core | **Rust 2024** | rustc 1.99.1 | Capture, DSP, rendering, platform hosts, config. |
| GPU | **wgpu** | 30.0.1 | DX12 (visual/composition presentation for alpha) + Vulkan on Windows, Metal, Vulkan on Linux. |
| Audio capture | `cpal`, `wasapi`, `rtrb`, `rubato` | 0.18.2 / 0.27.0 / 0.4.1 / 5.0.1 | CPAL for microphones; P0.3 picks CPAL loopback or the `wasapi` crate for system output. |
| macOS / Linux capture | `objc2-core-audio`, `screencapturekit`, `pipewire` | 0.3.2 / 11.1.2 / 0.10.1 | Phase 2. |
| DSP | `realfft` / `rustfft` | 3.5.1 / 6.4.1 | Preallocated plans, scratch buffers, no per-frame allocation. |
| Frontend | **SolidJS 2** | `solid-js` + `@solidjs/web` 2.0.0-rc.21, `@solidjs/vite-plugin` 3.0.0-next.57 | Studio only. `solid-js` and `@solidjs/web` must match exactly. Not `vite-plugin-solid`. |
| Tooling | **Bun**, Vite, TypeScript, oxlint, Prettier, repomix | Bun 1.4.4, Vite 8.3.6, TS 7.1.0-dev, oxlint 1.92.0, Prettier 4.0.0-alpha.13 | Bun is the only package manager and script runner. |
| IPC | `tauri-specta` (rebased v3 fork) + `specta` | upstream rc.27 is Tauri 2 only; `specta` 2.0.0-rc.28 | ADR-014. Spectrum frames bypass typed IPC as raw `ipc::Response` bytes. |

### 4.2 Process and thread model

```text
Tauri main thread
  - window creation, monitor/DPI queries, tray/menu, commands
  - no FFT, no audio callback, no GPU present

capture callback (one per active source)
  - interleaved f32 -> bounded rtrb producer
  - no allocation, locks, logging, FFT or blocking sends

analysis worker (one per active source)
  - drains ring, downmixes/resamples, runs DSP
  - owns FFT plans, windows, band maps, ballistics and feature state
  - publishes latest frame, sequence and status

render thread
  - owns wgpu device, surfaces, pipelines and per-target uniforms
  - samples the latest frame; never waits on the webview
  - parks after settle frames; wakes on audio/config/monitor events

platform host thread(s)
  - native webview-less visualizer windows and their message pump (Windows)
  - WorkerW/Progman, NSPanel/NSWindow, layer-shell, taskbar HWND
  - health checks, shell restart recovery, monitor hotplug
```

The render thread and the Tauri main thread are separate. A monitor or DPI query that must cross
into Tauri is cached and refreshed at about 1 Hz, never per frame. On Windows, visualizer HWNDs are
created and pumped on the host thread and presented from the render thread; on macOS, AppKit window
work hops to the main thread. Thread names, owners and shutdown order are in the blueprint
section 4.

### 4.3 Data flow

```text
system output / microphone / process / file
        |
        v
capture backend
  Windows: WASAPI loopback / CPAL mic
  macOS:   Core Audio tap / ScreenCaptureKit / CPAL mic
  Linux:   PipeWire monitor / Pulse / CPAL mic
        |
        v
real-time callback: interleaved f32 + device-clock anchor -> bounded rtrb ring
  - no allocation, no locks, no FFT, no logging
  - on overflow drop the newest chunk and count it; the analyzer skips forward
        |
        v
analysis worker (one per active source)
  - deinterleave / downmix / optional resample
  - window -> real FFT -> magnitude -> warp -> weighting
  - per-bar aggregation -> dB -> ballistics -> features
  - encode latest frame: header + bars + optional bins/features
        |
        v
frame slot ring (64 preallocated slots, seqlock, atomic latest index)
        |
        +-------------------------------+
        |                               |
        v                               v
native wgpu renderer                 Studio preview
reads bars in place, selects by      pulls raw wire bytes per rAF (<= 30 Hz),
timestamp - A/V offset               header-only when unchanged
(overlay / background / taskbar)     (real pipeline, reduced rate)
```

### 4.4 Frame contract

Frames are binary little-endian. Freeze this 8-word header before writing renderer code:

| Word | Field | Type | Meaning |
| --: | :-- | :-- | :-- |
| 0 | `sequence` | u32 | Monotonic frame sequence; wraps are allowed |
| 1 | `timestamp_lo` | u32 | Low 32 bits of monotonic microseconds of the newest analysed sample |
| 2 | `timestamp_hi` | u32 | High 32 bits of the same timestamp |
| 3 | `sample_rate` | u32 | Analysis rate |
| 4 | `fft_size` | u32 | Analysis FFT size |
| 5 | `bar_count` | u32 | Number of per-bar levels in the payload |
| 6 | `flags` | u32 | Bits 0-5: silent, clipped, stereo, bins-present, features-present, header-only. Bits 6-23 zero. Bits 24-31: `wire_version` (v1 = 1) |
| 7 | `generation` | u32 | DSP/config generation that produced the frame |

Payload: `bar_count` f32 levels, then optional `fft_size / 2 + 1` f32 bins and an optional fixed-size
feature block. The feature block is reserved in v1 (`FEATURES_PRESENT = 0`). If the client sends the
current `sequence`, reply with the 8-word header only. This follows ZER0's 8-word header and
`header_only` pattern while adding generation, a wire version and a 64-bit timestamp. The timestamp
comes from the capture device clock, not the encode time, so `present_time - timestamp` is the real
in-app audio age.

### 4.5 DSP

- Capture at the endpoint mix format; normalize to 48 kHz stereo in the worker. Downmix to mono for
  the main bars, preserve stereo for width/correlation.
- 2048-point real FFT, 512-sample hop, Hann by default; Blackman-Harris and Kaiser available.
- DC blocker before the FFT; coherent-gain normalization by default, RMS/energy normalization as an
  option.
- Per-bar edges built from Log/Linear/Mel/Bark/ERB/IEC 61260/note layouts. Calibration includes
  A/C/Z weighting, dB-per-octave tilt and level reference.
- Optional three-tier bass detail: full rate, /4, /16, each with its own FFT window. This is the
  Tourne'Table Precision idea, not one enormous FFT.
- Time-based attack/release, peak hold, gravity cap, auto-gain, 10-band display EQ.
- BS.1770-5 / EBU R128 loudness and true peak only when explicitly enabled; never label dBFS as
  LUFS.
- Status counters: analysis microseconds, dropped samples, overruns, last device change. Keep them
  out of the hot frame payload.

### 4.6 Renderer

- One wgpu device per process; one surface/window per monitor.
- Bars as instanced quads with SDF rounded corners in one pass. Geometry prepared once per frame,
  not per monitor.
- One uniform buffer per render target. `write_buffer` is queue-ordered; a shared uniform gives the
  second monitor the first monitor's origin.
- Premultiplied alpha. On Windows, use a non-sRGB UNORM target for transparent surfaces and set
  wgpu's DX12 presentation to the composition-visual path in code (CursorFX reaches it only through
  `WGPU_DX12_PRESENTATION_SYSTEM=Visual`). Refuse an opaque-only surface rather than cover a monitor.
- Full-surface clear each frame for shapes that can shrink or mirror; optimize only after a ghosting
  test exists.
- Pace each surface with its compositor acquire. Use FIFO for the pacing overlay; use Mailbox or a
  carried deadline for slower/background surfaces. Never gate a FIFO acquire on a second per-target
  deadline. Use `desired_maximum_frame_latency = 1`.
- Frame selection: draw the slot-ring frame whose timestamp is nearest `now - av_offset`. When the
  panel refresh exceeds the analysis rate (for example 144 Hz against 93.75 Hz), interpolate between
  the two bracketing frames; this costs up to one hop (10.7 ms) and is a setting.
- Settle, then park. Count loop iterations, not presents, for FPS diagnostics.
- Keep an offscreen capture target for visual regression and onboarding previews.

### 4.7 Platform hosts

- **Visualizer windows are webview-less.** `av-platform` creates native windows and hands
  `raw-window-handle` targets to `av-render`. The Studio is the only webview, so no transparent
  full-desktop window can ever show a webview error page (ADR-013; P0.2 verifies, with a Tauri
  `unstable` window and an empty-page `viz-` webview as ordered fallbacks).
- **Overlay (first):** transparent, non-focusable, per-monitor, resizable, movable, optional
  click-through. This is the reference implementation for the renderer.
- **Background (second):** Windows WorkerW/Progman child; macOS desktop-level window; Linux X11
  desktop type or layer-shell background. Always fall back to the overlay/in-app window if the host
  cannot be attached.
- **Taskbar/menu bar (last):** adjacent HWND on Windows, `NSStatusItem` on macOS, layer-shell strip
  or panel applet on Linux. Native Windows taskbar XAML is experimental and optional.
- Every host exposes `attach`, `resize`, `present`, `set_visible`, `health_check` and `detach`.
  Surface ownership and OS attachment are separate traits.
- Create each host hidden, present the first frame, then show it. Hide before destroying the
  surface. Every host has a Drop guard and a panic-path cleanup.
- v1 builds with `panic = "unwind"` so Drop guards run and Tauri error handling works. If
  `panic = "abort"` is adopted later, ADR-001 requires a tested panic hook that destroys windows.

### 4.8 Settings and scenes

- One `AppConfig` / `Scene` model in `av-protocol`, with an explicit schema version.
- Load pipeline: **migrate -> self-heal missing fields -> sanitize NaN/range -> normalize**.
- Save pipeline: atomic temp-file + rename, 400 ms debounce, flush on exit.
- Built-in presets are generated from Rust into the frontend; user scenes are individual files
  with stable slugs.
- Share codes exclude machine-specific settings and include a format version.
- Every new numeric field gets a clamp and a parity test.
- Domain settings (device, FFT preset, scene) are separate from ephemeral UI state (hover bin,
  panel width).

### 4.9 Frontend

- SolidJS 2 for the settings studio, scene editor and preview. File layout and state rules are in
  the blueprint section 7.
- The primary visualizer is not a webview. If the ADR-013 fallback ever needs a webview visualizer
  window, it gets a separate, empty Vite entry; the Studio bundle is never loaded into it.
- Preview runs the real DSP pipeline at reduced rate: one rAF poller calls `get_frame(last_seq)`,
  which returns raw wire bytes as an `ipc::Response` (header-only when unchanged), capped at 30 Hz
  and paused when the preview is hidden. Frames never become Tauri events or store writes.
- Config is a Solid store mirrored from Rust as desired/applied; Rust sanitizes and answers with the
  applied value.
- Preview canvases use `requestAnimationFrame`, a preallocated `Float32Array`, DPR capped at 2,
  cached gradients/fonts/paths, and theme colors read only on theme change.
- Hover and pointer state stay local; they do not go into the global settings store.
- i18n is lazy-loaded; any overlay waits for its bundle before the first paint.

### 4.10 Proposed repository layout

```text
AV_AudioVisual/
  README.md                  product vision, architecture, roadmap
  AGENTS.md                  agent and contributor rules
  app.meta.json              single source of product identity
  references/                reference sources + study notes + master index
  src/                       SolidJS 2 Studio: settings, scene editor, preview, diagnostics
  src-tauri/                 composition root (package av-app): startup order, tray, commands
  crates/
    av-protocol/             config, scenes, migrations, sanitize, share codes, frame + wire
    av-dsp/                  pure DSP: FFT, bands, weighting, ballistics, loudness, features
    av-audio/                capture backends, ring, format conversion, device model
    av-engine/               source supervisor, analysis worker, frame slot ring, idle machine
    av-render/               wgpu device, pipelines, targets, offscreen capture; no windowing
    av-platform/             native host windows, monitor/DPI, power/occlusion signals
  spikes/                    throwaway Phase 0.5 binaries, outside the workspace
  scripts/                   update-deps, pre-commit, app-meta, fixtures, icons, repomix
  docs/                      plans, ADRs, performance, testing
  test/                      fixtures and cross-language parity tests
  evidence/                  git-ignored spike and release evidence
```

`av-engine` is new in the second pass: it keeps `av-dsp` pure (no threads, clocks or I/O, so it can
be tested against golden vectors) and keeps `av-audio` about devices and samples. The allowed
dependency edges are a checked contract (blueprint section 3.1): `av-protocol` depends on nothing
internal; `av-render` and `av-platform` never depend on each other; only `src-tauri` sees
everything. ZER0's 4,057-line settings module and Tourne'Table's 19,934-line translation unit are
the cautionary examples.

---

## 5. Platform strategy

### 5.1 Capability and fallback matrix

| Capability | Windows | macOS | Linux | Fallback |
| :-- | :-- | :-- | :-- | :-- |
| System output | WASAPI shared-mode loopback | Core Audio process tap (14.2+) or ScreenCaptureKit audio (13+) | PipeWire monitor source; `parec --monitor` | Visible error; optional microphone only with consent |
| Microphone | WASAPI/CPAL | Core Audio/CPAL | PipeWire/Pulse/ALSA | Device picker + permission card |
| Transparent overlay | Layered/DComp top-level window | `NSPanel`/`NSWindow` at status level | Layer-shell/X11 ARGB | In-app preview window |
| Background | WorkerW/Progman child | `NSWindow` at `kCGDesktopWindowLevel` | X11 desktop type / layer-shell background | Overlay panel |
| Taskbar/menu bar | Adjacent HWND first; native XAML later | `NSStatusItem` | Layer-shell strip or panel applet | Overlay panel |
| Media metadata | SMTC | Now Playing / `MPNowPlayingInfoCenter` | MPRIS | None; visualizer works without it |
| HDR | DXGI scRGB/HDR10, later | EDR, later | Compositor-dependent, later | SDR |

### 5.2 Windows

- **Capture:** WASAPI shared-mode loopback on the default render endpoint, event-driven, at the
  endpoint mix format. Support f32, s16, s32 and 24-in-32 samples, including
  `WAVE_FORMAT_EXTENSIBLE`; reject packed 24-bit. Convert to f32 in the capture adapter; normalize
  rate and channels in the analysis worker, not the callback. Rebuild on
  `AUDCLNT_E_DEVICE_INVALIDATED` and default endpoint changes.
- **Endpoint state:** read endpoint mute/volume (`IAudioEndpointVolume`) on change and show
  "Muted" rather than silence. In deep idle, stop loopback and poll the render endpoint's
  `IAudioMeterInformation::GetPeakValue` at ~4 Hz as the zero-capture wake signal (Audio HUD).
- **Overlay:** webview-less top-level window with `WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW |
  WS_EX_NOACTIVATE | WS_EX_TOPMOST`, composition swap chain with premultiplied alpha,
  `DWMWCP_DONOTROUND` when the scene draws its own corners, and no
  `SetWindowCompositionAttribute` acrylic on the same window. Locked mode adds
  `WS_EX_LAYERED | WS_EX_TRANSPARENT` and answers `HTTRANSPARENT`; edit mode answers `HTCAPTION`
  for drag, `HTLEFT`/`HTBOTTOMRIGHT`/... for resize and `HTCLIENT` for hot zones. Per-monitor DPI
  awareness v2.
- **Background:** child of the `WorkerW` behind `SHELLDLL_DefView`; handle the Windows 11 24H2
  `Progman` layout; re-attach after Explorer restarts (`TaskbarCreated` message).
- **Taskbar:** an owned, topmost, non-activating strip positioned over a free region of the
  taskbar. Bar geometry, edge and auto-hide come from `SHAppBarMessage` (`ABM_GETTASKBARPOS`,
  `ABM_GETSTATE`); free regions are found out-of-process with UI Automation (task-list and tray
  bounds), not private symbols; z-order is re-asserted on `SetWinEventHook` foreground/location
  events for the taskbar. Native XAML injection stays a later, experimental option because it
  depends on private `taskbar.dll` symbols. P0.9 proves this path.
- **Media:** SMTC, later.

### 5.3 macOS

- **Capture:** Core Audio process tap on macOS 14.2+, ScreenCaptureKit audio on 13+; microphone via
  CPAL. Show a permission card with a deep link to System Settings.
- **Overlay:** `NSPanel` with `.nonactivatingPanel`, status-level window, transparent, no focus.
- **Background:** `NSWindow` at `kCGDesktopWindowLevel`, `ignoresMouseEvents = true`,
  `collectionBehavior = [.canJoinAllSpaces, .stationary, .ignoresCycle]`.
- **Taskbar equivalent:** `NSStatusItem` menu-bar extra. The Dock is not extensible.
- **Media:** Now Playing, later.

### 5.4 Linux

- **Capture:** PipeWire monitor of the default sink; `parec --monitor` fallback behind the same
  trait; rebuild on default-sink changes.
- **Overlay:** `zwlr_layer_shell_v1` layer `overlay`/`top` on Wayland; override-redirect ARGB
  window on X11.
- **Background:** layer-shell `background` on wlroots-based compositors; X11 desktop window type
  with an empty input shape. GNOME requires a Shell extension for a true panel/desktop widget.
- **Taskbar equivalent:** panel applet (KDE/GNOME extension) or a layer-shell strip.
- **Media:** MPRIS, later.

Capability gaps and fallback behavior are product features, not footnotes. Each mode must show
what is available on the current compositor and why.

---

## 6. Performance and power

These are engineering targets, not marketing claims. Measure before optimizing.

| Path | Target | Notes |
| :-- | :-- | :-- |
| Audio callback | < 0.5 ms p99, zero allocation, zero locks | Push to `rtrb` and return |
| Analysis | < 2 ms per 512-sample hop at 48 kHz on one modern core | 2048 FFT at 75% overlap |
| Spectrum transport | Renderer: zero copies beyond the bars it draws. Preview: < 150 KB/s at 30 Hz with 1,025 bins | Header-only when unchanged; preview paused when hidden |
| Render | 60 fps at 2x 4K on a mid-range GPU; CPU frame < 2 ms | Measure presentation, not just geometry |
| Idle | < 1% CPU, near-zero GPU when silent and settled | Park the render loop; stop loopback in deep idle |
| Memory | < 150 MB baseline, < 300 MB with large scenes | Preallocate; no per-frame objects |
| Startup | < 1.5 s to settings window; < 300 ms to first visualizer frame | Lazy GPU init |
| Battery mode | 30 fps, reduced particles, occlusion pause | Explicit user setting |

### 6.1 Idle state machine

| State | Trigger | Capture | Analysis | Render |
| :-- | :-- | :-- | :-- | :-- |
| **Audible** | Signal above noise floor | Running | Normal hop | Normal fps |
| **Trickle** | Silence for a short grace period | Running, silent frames | Reduced rate | Lower fps |
| **Deep idle** | Silence/occlusion for a longer period | Loopback stopped; endpoint peak meter polled at ~4 Hz | None | Parked; wake on meter > floor, input or config |
| **Muted** | Endpoint muted | Unchanged (or deep idle) | Unchanged | "Muted" state drawn once, then parked |
| **Covered** | Fullscreen/occlusion per monitor | Unchanged | Unchanged | Pause that surface |
| **Battery** | On battery / energy saver | Unchanged | Unchanged | Reduced fps/effects |

Non-negotiables: no per-frame FFT plan construction, no per-frame gradient/path/font creation, no
per-frame Tauri window queries, no JSON spectrum frames, no 60 Hz reactive store updates.

### 6.2 Measurement protocol

- Record the machine, OS build, GPU, driver, display mode and power state with every benchmark.
- Measure capture, DSP, encode, decode, submission and presentation separately.
- Report p50 and p99, not only the mean.
- Run a 10-minute soak with silence, music, device changes and monitor hotplug.
- A performance claim needs a command and a measured number; an estimate is not evidence.

### 6.3 Latency budget (estimate, grade C until P0.7)

"Latency" here is onset-to-photon: from a transient leaving the system mix to the bar visibly
rising. The window term is the one people forget: a Hann window weights its newest samples near
zero, so a transient reaches about half its eventual energy when it is near the window centre.

| Stage | Default (2048 / 512 hop, 60 Hz Fifo) | Low-latency (1024 / 256 hop, 120 Hz+) | Lever |
| :-- | --: | --: | :-- |
| Loopback packet (shared mode, ~10 ms period) | ~5 ms | ~5 ms | Event-driven capture; smaller period if the API allows |
| Waiting for the hop | ~5 ms | ~3 ms | Hop size |
| Window centre (N/2 at 48 kHz) | ~21 ms | ~11 ms | Window length; asymmetric window later |
| Attack ballistics (preset) | ~10-15 ms | ~5-8 ms | Attack time |
| Analysis + publish | < 2 ms | < 1 ms | Budget in the table above |
| Frame sampling (half a refresh) | ~8 ms | ~4 ms | Refresh rate |
| Present queue + compositor (latency 1) | ~17-33 ms | ~8-16 ms | `desired_maximum_frame_latency = 1`; independent flip later |
| **Estimated total, p50** | **~70-80 ms** | **~35-45 ms** | |

Consequences:

- The old "under 50 ms by default" target is withdrawn; it is the low-latency preset target.
- Loopback sees the mix **before** the output device. Bluetooth and some USB/HDMI outputs add tens
  to hundreds of milliseconds after it, so visuals can run *ahead* of sound. The A/V offset
  (ADR-016) delays visuals by selecting older frames from the slot ring; the default is 0.
- Diagnostics show `present_time - frame.timestamp` (in-app audio age) so the analysis and present
  terms are visible without a camera. P0.7 measures the full chain with a camera.

---

## 7. Roadmap and execution plan

### Phase 0 - Research (complete)

- Reference inventory, shortcut resolution, 14 source studies, cross-project synthesis.
- Architecture synthesis, placement matrix, cross-OS matrix and licence map.
- Deliverables: [`references/README.md`](references/README.md),
  [`references/studies/`](references/studies/), and [`docs/pre-plan.md`](docs/pre-plan.md).

### Phase 0.5 + Phase 1A - Spikes and foundation, in two parallel tracks

The second pass split this work by risk instead of running every spike first. Pure contracts
(workspace, wire format, DSP) cannot be invalidated by a platform spike, so they are built for real
while the platform questions are answered by throwaway code. Details:
[`docs/phase-1a-blueprint.md`](docs/phase-1a-blueprint.md) section 9.

**Track A - product foundation (kept)**

- A1-A3: Tauri 3 + Bun + SolidJS 2 workspace, startup order, scripts and gates, ADR-014 typed IPC
  (closes P0.1).
- A4-A5: `av-protocol` wire format with Rust/TS parity (closes P0.5), then `AppConfig`/`Scene` with
  migrate/heal/sanitize/normalize and atomic saves.
- A6: `av-dsp` with golden vectors, allocation gate and benchmarks (closes P0.4).
- A7-A8: file source, `av-engine` worker and slot ring, Studio preview over raw `ipc::Response`.
- A9-A10: headless `av-render` snapshots; `av-platform` monitors and host state machine (no
  attachment).

**Track B - platform spikes (thrown away under `spikes/`)**

- P0.2 webview-less transparent overlay (native HWND and Tauri `unstable` variants; DX12 visual
  presentation; Vulkan).
- P0.3 WASAPI loopback (`cpal` vs `wasapi`), formats, device loss, device-clock timestamps.
- P0.6 overlay lifecycle under panic, close, DPI change and unplug.
- P0.7 capture-to-photon latency per preset.
- P0.8 Background feasibility: attach a wgpu surface behind the icons on classic and 24H2 layouts,
  survive an Explorer restart.
- P0.9 Taskbar feasibility: a click-through strip over a free taskbar region that stays above the
  bar, follows auto-hide/edge changes and never steals focus.

**Exit gate**

- Every Track A gate green: `bun run precommit:full`, parity, DSP numeric and allocation gates,
  bindings current, CI green on Windows/Linux/macOS.
- Every Track B spike has evidence and a go/no-go; ADR-001/003/005/007/013/014/016 written.
- P0.2 fails closed: if no transparent overlay variant is proven on Windows, Phase 1B does not
  start.
- No product visualizer window yet; this slice is intentionally invisible.

### Phase 1B - Overlay first light (v0.1, Windows)

**Work**

- WASAPI loopback + CPAL microphone, device picker and ring.
- 2048/75% DSP, 48 per-bar bands, bars/line/area, time-based ballistics, auto-gain.
- Binary frame transport and rAF preview.
- wgpu transparent overlay, one monitor, basic scene, click-through toggle; create hidden, present
  the first frame, then show.
- Settings studio, scene save/load, tray, diagnostics, idle state machine.

**Exit gate**

- Microphone and system output render at 60 fps.
- Audio callback p99 < 0.5 ms; DSP < 2 ms per hop; zero dropped samples over 10 minutes.
- Settings survive restart; silence parks the renderer.
- A real-GPU capture is attached to the release evidence.

### Phase 2 - Cross-OS overlay (v0.2)

**Work**

- macOS tap/ScreenCaptureKit, Linux PipeWire monitor, permission and device-loss UX.
- Multi-monitor overlay with per-monitor DPI and position.
- Source capability matrix, latency measurement and cross-OS manual checks.

**Exit gate**

- System output works on Windows, macOS and Linux without a virtual driver.
- Device loss, format change and default-endpoint change recover visibly.
- Mixed-DPI and hotplug checks pass or are marked unverified.

### Phase 3 - Scenes and renderer depth (v0.3)

**Work**

- Radial, waveform, spectrogram/waterfall, VU/peak, goniometer.
- Palettes, album art, reflections, glow, trails, background blur.
- Scene files, share codes, per-monitor scenes, hotkeys.
- Offscreen capture harness and visual regression fixtures.

**Exit gate**

- A scene exported on one OS renders identically on another with the same GPU path.
- Migration, sanitize and parity tests cover every new field.
- Visual changes are approved from GPU captures.

### Phase 4 - Background mode (v0.4)

**Work**

- Windows WorkerW/Progman host with 24H2 layout and Explorer-restart recovery.
- macOS desktop-level window; Linux X11/layer-shell background.
- Cached wallpaper blur, occlusion/fullscreen pause, per-monitor surfaces.
- Fallback to overlay when the host cannot attach.

**Exit gate**

- Behind icons, click-through, survives shell restart, idles when covered.
- Manual matrix includes 24H2, mixed DPI, Win+D, lock/unlock and sleep/resume.

### Phase 5 - Taskbar / Menu Bar (v0.5)

**Work**

- Windows owned topmost strip over a free taskbar region (SHAppBarMessage + UI Automation +
  WinEvent z-order re-assert); optional experimental native XAML surface.
- Layout edit mode for the strip: drag along the bar, width handles, snap to free regions;
  horizontal and vertical (side taskbar) layouts.
- macOS `NSStatusItem` compact spectrum.
- Linux layer-shell strip or panel applet where available.

**Exit gate**

- Compact mode is readable at taskbar height, does not steal focus or clicks, and degrades to the
  overlay on unsupported shells.
- Native XAML is feature-flagged and never the only entry point.

### Phase 6 - Media, HDR and packaging (v0.9)

**Work**

- SMTC/MPRIS/Now Playing metadata and transport.
- Optional lyrics with local cache, network switch and licence decision.
- HDR/wide-gamut path where the compositor supports it.
- Battery mode, startup profiles, installers (NSIS/DMG/AppImage/deb/rpm) and portable mode.

**Exit gate**

- Signed builds, licence audit, diagnostics export and updater story.
- HDR is advertised only on a validated display path.

### Phase 7 - Hardening and release (v1.0)

**Work**

- Crash/panic cleanup for every window host; opaque-window and surface-lifetime guards.
- Accessibility, i18n, onboarding, diagnostics.
- Full test matrix, soak tests, cross-OS manual pass and release notes.

**Exit gate**

- `bun run precommit:full` and `cargo test --workspace` pass on every target OS.
- No known crash on the host matrix.
- Release evidence is complete.

### 7.1 Critical path and parallel tracks

**Critical path:** Phase 0.5 -> protocol -> capture -> DSP -> frame contract -> renderer -> overlay
-> multi-monitor -> Background -> Taskbar.

**Parallel tracks:** settings studio, scene schema, visual fixtures, diagnostics, packaging.

**Do not start** Background or Taskbar until the overlay path is stable, measured and visually
verified. Do not start media/lyrics until the visualizer is shippable without them.

---

## 8. Test and evidence plan

| Layer | Automated | Manual / GPU |
| :-- | :-- | :-- |
| DSP | Silence, sine, white/pink noise, impulse, DC, Nyquist, clipping; 44.1/48/96 kHz; 1/2/6 channels; golden vectors with tolerances | Listen to a reference track; compare 30/60/120 fps response |
| Allocation | 20 warm-up + 500 steady-state calls with zero allocations | Long-run memory check |
| Transport | Header round-trip, sequence wrap, `header_only`, generation, Rust/TS parity | Preview under a busy UI |
| Capture | Device loss, format change, permission denial, silence, hotplug, default-endpoint change | Real system playback on each OS |
| Renderer | Offscreen snapshots for every style/shape/palette at fixed DPI | Real-GPU capture at 100/125/150/200% DPI, mixed monitors |
| Hosts | Attach/detach/rebuild state machine; Explorer restart; monitor change | Win+D, lock/unlock, sleep/resume, fullscreen, RDP |
| Config | Migration from every prior schema; partial/corrupt store; NaN/range clamp; parity fixtures | Upgrade from a previous build |
| Performance | Criterion benchmarks for DSP, encode, decode and render; CI budget check | Perf HUD and 10-minute soak |
| Cross-OS | CI compile and unit tests on all three | Manual capability check per OS; mark unverified otherwise |

Evidence rules:

- Numerical behavior is pinned by tests (`cargo test`, parity fixtures, DSP golden frames).
- Visual behavior is confirmed on a capture from the running GPU app, never from a CPU rasterizer.
- A performance claim needs a benchmark command and a measured number.
- A cross-OS claim needs a manual check on that OS, or an explicit "unverified" label.
- A bug that reaches the renderer or host gets a regression test or a guard.

---

## 9. Risk register

| Risk | Likelihood / impact | Mitigation | Early warning |
| :-- | :-- | :-- | :-- |
| Tauri 3 prerelease breakage | High / High | Pin family as a set; CI on every update; no patched forks | `cargo check` or bindings fail after update |
| wgpu transparent surface on Windows | High / High | Early spike; non-sRGB UNORM; premultiplied alpha; preflight; GPU capture | Black/opaque window, DWM artifacts |
| WASAPI loopback format/device loss | Medium / High | Endpoint mix format; invalidation state machine; silent frame | Bars freeze or silence after device change |
| macOS tap permission/entitlement | High / High | Spike before UI work; ScreenCaptureKit fallback; permission card | No frames, TCC denial |
| Linux Wayland fragmentation | High / Medium | Layer-shell, X11 fallback, in-app fallback; document GNOME limits | Surface never appears |
| WorkerW/Progman undocumented changes | High / High | Feature flag; health check; shell restart rebuild; overlay fallback | Not behind icons after 24H2/Explorer restart |
| Taskbar private API breakage | Very high / High | Adjacent HWND first; XAML experimental; never sole UI entry | Taskbar update hides or crashes the strip |
| Audio latency/jitter | Medium / Medium | Sample-clock scheduling; measure capture-to-present; ring counters | Visible lag on beats |
| Render cost at 4K | Medium / Medium | Measure; instanced quads; one pass; DPR cap; battery mode | CPU frame > 2 ms |
| Config corruption/loss | Medium / Medium | Migrate/heal/sanitize; atomic writes; partial-store tests | Settings reset after upgrade |
| GPL/unlicensed contamination | Medium / High | Clean-room implementation; provenance notes; licence audit | Code similarity or missing notices |
| Scope creep before MVP | High / High | Slice 1 gate; defer media/lyrics/HDR/taskbar/3D | Phase 1 keeps growing without a release |
| Opaque overlay covers desktop | Medium / High | Vite preflight; Drop guards; panic cleanup; no bare debug binary | Full-desktop opaque error page |
| Mixed-DPI misplacement | High / Medium | Physical pixels; stable monitor ID; per-monitor surface | Offset/blurry surface on second monitor |
| Typed-IPC fork drift (`tauri-specta` upstream is Tauri 2 only) | Medium / Medium | Fork is a manifest-only retarget, rebased onto upstream, pinned by `rev`; updater reports commits behind and upstream Tauri 3 support; IPC contained in `src-tauri/src/ipc/` | Fork fails to build after a Tauri alpha; upstream publishes Tauri 3 support |
| Solid 2 release-candidate churn | Medium / Medium | Exact pins; `solid-js` = `@solidjs/web`; record API notes in A1; keep UI logic thin | Typecheck or JSX transform breaks after update |
| Registry dist-tags point backwards | High / Low | Updater takes highest semver over all versions; never downgrades | Update report shows a lower version than installed |
| Latency worse than expected | Medium / Medium | Presets; P0.7 measurement; frame latency 1; A/V offset | Beats look late in the P0.7 video |
| Visuals ahead of audio on Bluetooth | High / Low | A/V offset from the slot-ring history | Users report "bars move before the sound" |

The full risk register and evidence mapping are in the
[cross-project synthesis](references/studies/00-cross-project-synthesis.md).

---

## 10. Decisions and open questions

### 10.1 Locked decisions

- Tauri 3, newest published 3.x, prereleases included, family pinned and validated as a set.
- Standalone process; no injection into Explorer, Finder or a compositor.
- Native wgpu visualizer; SolidJS for settings and preview only.
- System output is the default source; microphone is first-class but optional.
- Per-bar measurement and time-based ballistics from the first engine.
- Binary latest-frame transport; status on a separate low-rate channel.
- CPU DSP first; GPU analysis is a later optimization.
- Surface order: overlay -> Background -> Taskbar/Menu Bar.
- No virtual audio driver requirement.
- No GPL or unlicensed code copied.

### 10.2 ADR queue

The single numbered ADR queue, with deadlines and recommended defaults, is in
[`docs/pre-plan.md`](docs/pre-plan.md) section 11 (ADR-001 to ADR-019). ADRs are written to
`docs/adr/NNN-title.md`. The ones that must exist before Phase 1B:

| ADR | Decision | Recommended default |
| :-- | :-- | :-- |
| 001 | Panic strategy | `unwind` |
| 003 | In-process frame sharing | 64-slot seqlock ring |
| 005 | Stateful vs display transforms | Ballistics/EQ/auto-gain in the analyzer |
| 007 | Config unknown fields | Ignore and rewrite normalized |
| 013 | Visualizer window ownership | Native webview-less windows in `av-platform` |
| 014 | Typed IPC on Tauri 3 | Needed; rebased `tauri-specta-v3` fork pinned by `rev`; exit to crates.io when upstream supports Tauri 3 |
| 016 | A/V sync and interpolation | Frame selection by timestamp with a user offset |
| 019 | Dependency policy and updater | Always newest; holds only in `update-policy.ts`; ported ZER0 + CursorFX updater with rollback |

Product questions that are not architecture ADRs yet: default bar count and band layout (48/log vs
64), stereo width in the first renderer or Phase 3, and shared-memory transport if external
clients ever exist.

---

## 11. Workflow rules

The full agent rules live in [`AGENTS.md`](AGENTS.md).

### 11.1 Bun and RTK

- **Bun only.** Never `npm`, `npx`, `yarn` or `pnpm`.
- **Prefix shell commands with `rtk`** (`rtk git status`, `rtk cargo check`, `rtk cargo test`).
  Sanctioned Bun scripts run directly: `bun run tauri dev`, `bun run typecheck`,
  `bun run update-deps`, `bun run precommit`.
- The only supported development launch is:

  ```bash
  bun run tauri dev
  ```

  Never run a bare debug binary. A debug build without Vite can load an opaque error page into a
  transparent full-desktop window. CursorFX documents three Windows reboots caused by exactly that.

### 11.2 Dependency policy

- **Always the newest: prerelease or latest, whichever is newer**, for every npm package, every
  crate, Bun itself (canary channel) and the GitHub Actions in CI. There is no stable-only mode.
  Anything held back is an explicit, reasoned entry in `scripts/update-policy.ts` and is printed in
  every update report.
- `bun run update-deps` is the only supported updater. It is **ported from the two reference
  updaters** (ZER0 `scripts/update-deps.ts` and `update-all.ts`, CursorFX `scripts/update-deps.ts`)
  and adapted to this workspace; the merged design is in
  [`docs/phase-1a-blueprint.md`](docs/phase-1a-blueprint.md) section 8.1. It probes every dist-tag
  plus the full npm version list and the full crates.io version list, updates manifests and
  lockfiles, holds families together, retries cargo conflicts by holding the culprit, restores
  prerelease pins that a transitive refresh clobbers, then validates (typecheck, Vite build, Tauri
  config check, `cargo check`, fast tests) and **rolls everything back** if validation fails.
- The Tauri family stays on **Tauri 3** and tracks the newest published 3.x release, prereleases
  included. Pin the family **as a set**, not one crate at a time, and assert the resolved family is
  3.x. A partial bump can resolve and then fail to compile; ZER0 documents three such failures.
  Moving to Tauri 4 is a coordinated family migration, not a routine update.
- `bun run update` is the routine entry point: rtk -> Bun canary -> `update-deps` -> Actions audit
  -> repomix (ZER0 `update-all.ts` order).
- Dist-tags are hints, not truth: on 2026-11-07 `@solidjs/vite-plugin@next` pointed at an older
  build than `@latest`, and `@solidjs/web@latest` at rc.0. The updater takes the highest semver
  across all published versions on the allowed channels.
- Tauri family members have different version numbers (`tauri` alpha.6, `tauri-build` alpha.4,
  plugins alpha.3). The family is the set `tauri`'s own requirements resolve to.
- Never downgrade, and never move a line-pinned package off its major (Solid 2, `@solidjs/vite-plugin`
  3, `@tauri-apps/*` 3, Tauri crates 3). `@tauri-apps/cli` moves in lockstep with the `tauri` crate
  because the CLI validates `tauri.conf.json` before cargo runs (ZER0 `update-deps.ts:257-294`).
- The Rust toolchain follows the newest **stable** release (`rust-toolchain.toml` channel
  `stable`, updated by `rustup`); beta/nightly is the one documented exception in the policy file
  because clippy/rustfmt drift on nightly would make the gate non-reproducible. Change it there if
  you want nightly.

### 11.3 Pre-commit routine

```bash
bun run hooks:install      # once per clone: git config core.hooksPath .githooks
bun run precommit          # identity, translations, lint, typecheck, unit, format, repomix
bun run precommit:full     # the same plus clippy and the Rust test suite
bun run precommit:routine  # update everything to newest prerelease, then precommit:full
```

The fast gate is for every commit. `precommit:full` is for releases and pull requests. The hook
reads the working tree, not the index, and stages only generated identity mirrors. It is not a
substitute for reviewing the diff.

### 11.4 Commit discipline

- One feature or fix per commit. Do not bundle unrelated changes.
- Inspect `rtk git status` and `rtk git diff` before staging. Stage only intended files.
- Run `bun run precommit:full` before a feature push. If the gate fails, fix and create a new
  commit; never amend a failed commit that was already pushed.
- Commit messages state **what changed, why, the performance cost, and the evidence** (test,
  benchmark, capture, or manual check). Example:

  ```text
  audio: add WASAPI loopback backend

  Adds a shared-mode, event-driven loopback capture path for the default
  render endpoint, behind the CaptureBackend trait. Rebuilds on device
  invalidation and publishes silent frames when nothing is playing.

  Cost: one capture thread, one 2 s ring, no new per-frame allocation.
  Tested: cargo test -p av-audio; manual Spotify/YouTube playback on Win 11.
  ```

- Direct pushes to `main` are allowed only for the solo maintainer, only when the full gate is
  green, and never with `--force`. Releases are tagged after the gate. If the repository gains
  contributors, switch to short-lived feature branches and pull requests.
- Never commit secrets, `.env` files, signing material, model weights or generated installers.
- Never copy GPL or unlicensed code. Record conceptual sources in the commit message when a
  technique comes from a reference.

### 11.5 Evidence standard

- Numerical behavior is pinned by tests (`cargo test`, parity fixtures, DSP golden frames).
- Visual behavior is confirmed on a capture from the running GPU app, never from a CPU rasterizer.
- A performance claim needs a benchmark command and a measured number, not an estimate.
- A cross-OS claim needs a manual check on that OS, or an explicit "unverified" label.

---

## 12. Licensing and credits

The product licence is not final; the intended direction is **MIT or Apache-2.0**, with a
permissive-only dependency tree. GPL-3.0 and GPL-2.0 reference code must not be copied. Several
reference mods have no licence declaration and are study-only. Each study note records the exact
provenance.

The reference library contains GPL-3.0 Desktop/Calendar Live Overlay and Live Video Wallpaper,
GPL-2.0-only CoverTabby, and unlicensed Desktop Audio Visualizer and FFT plugin code. These are
architecture and numerical references only. The Fluid wallpaper's embedded shader port is MIT and
requires its notice if the shader math is ported.

Credits to the reference authors are listed in [`references/README.md`](references/README.md).
Upstream notices must be preserved when code or algorithms are derived from MIT/Apache sources.

---

## 13. Current status and next action

- [x] Inventory and resolve every reference.
- [x] Study CursorFX, ZER0/Handy_V2, Tourne'Table, Desktop Audio Visualizer, Neiz, Island,
      Desktop/Calendar overlays, wallpaper mods, Alt+Tab mods, the FFT plugin and MAC_OS_ANIM.
- [x] Write per-project study notes with file:line evidence.
- [x] Cross-reference the studies and resolve the major design disagreements.
- [x] Write the reference master index, placement matrix, cross-OS matrix and licence map.
- [x] Write the product README, architecture direction, MVP cut and execution plan.
- [x] Write the deeper audit, Phase 0.5 spike queue and contracts in `docs/pre-plan.md`.
- [x] Second pass (2026-11-07): registry-verified stack, webview-less visualizer windows, slot-ring
      frame sharing, wire version, device-clock timestamps, latency budget and A/V offset, typed-IPC
      ADR, `av-engine` crate, two-track execution plan in `docs/phase-1a-blueprint.md`.
- [x] Third pass (2026-11-07): three placements as first-class product modes with a geometry and
      input contract; Audio HUD study; updater ported from ZER0 + CursorFX with an always-newest
      policy; P0.8/P0.9 spikes; ADR-018/019.
- [ ] Track A step A1: scaffold the Tauri 3 / Bun / SolidJS 2 workspace and startup order (closes
      P0.1).
- [ ] Track A step A2: port and adapt the reference updater and pre-commit scripts.
- [ ] Track A A3-A10 and Track B P0.2/P0.3/P0.6/P0.7/P0.8/P0.9 in parallel, with evidence.
- [ ] Phase 1B: ship overlay first light on Windows with system audio and per-bar bars.

The next action is Track A step A1. Do not start Background, Taskbar, media or 3D work before the
overlay path is measured and visually verified.
