# Progress

What is implemented versus planned, kept truthful (AGENTS.md section 8). Steps refer to
`docs/phase-1a-blueprint.md` section 9. "Verified" names the evidence.

## Phase 1A, Track A (product foundation)

| Step | State | What exists | Verified |
| :-- | :-- | :-- | :-- |
| A1 scaffold | Done | Bun + Vite 8 + TS 7 + SolidJS 2 Studio; Rust workspace with six crates and `av-app`; Tauri 3.0.0-alpha.6 family; startup order enforced at runtime; dev preflight; session log; ADR-001 | `bun run tauri dev` opens the Studio on Windows 11 |
| A2 scripts and gates | Partial | `app-meta.ts` identity mirror, crate-graph check, `pre-commit.ts` (fast/full/routine), `.githooks/pre-commit`, repomix, CI on Windows/Linux/macOS | `precommit:full` green locally; CI green on all three OSes |
| A2 updater | Not started | `update-deps.ts` / `update-policy.ts` / `update-all.ts` port from ZER0 + CursorFX (blueprint 8.1) | - |
| A3 typed IPC | Not started | Commands are hand-typed in `src/lib/ipc.ts` until the `tauri-specta-v3` fork is rebased and pinned (ADR-014) | - |
| A4 wire format | Done | `av-protocol` frame/wire v2 (waveform block; v1 still decodes), fixtures, `src/lib/frame/wire.ts` | 18 fixtures byte-identical in both directions |
| A5 config | Done | `AppConfig` v2 (every FFT CHOP parameter, migration from v1), `Scene` v1, user analysis presets, migrate -> heal -> sanitize -> normalize, atomic debounced store with backups, ADR-007, ADR-021 | Pipeline, migration, preset and store tests; settings and parameter-table parity fixtures |
| A6 DSP | Done | Analyzer: layered FFT in front of the FFT CHOP chain (6 windows, 7 scales, warp interpolation and aggregation, A/C/ITU-R 468, references, loudness, ballistics, spectral features) | Golden, layered and allocation gates; default p99 181 us parallel (`docs/performance.md`) |
| A7 engine | Done | rtrb ring, symphonia file source, analysis worker, 64-slot seqlock ring, status snapshot, preview encoder | Bit-exact golden frames; no torn reads; zero allocations on the frame path |
| A8 preview | Done | `get_frame` raw pulls, frame bus, preview canvas, demo track | Bus tests; manual run with `--demo` (moving bars captured) |
| A9 headless render | Not started | wgpu instanced bars, offscreen target, premultiplied invariant | - |
| A10 platform | Not started | Monitor ids, DPI, `SurfaceHost` state machine with a fake host | - |

## Analysis goal: Super FFT (ADR-020, docs/super-fft.md)

| Item | State |
| :-- | :-- |
| Layered FFT (1-6 layers, zero-padded, 1/2^(k-1) scaled, averaged) | Done: default 4 layers of 1,024-8,192, FFT 32,768, parallel from 3 layers; magnitudes only where read. Open: layer decimation, AVX2 kernels, MMCSS priority |
| FFT CHOP integration (every parameter, ADR-021) | Done: all pages in the Studio, one built-in default, user presets saved as files. Not ported: FFTW planner/backend, ingest, worker wake (TouchDesigner-only) |
| FFT CHOP v2.13.0 parity audit (docs/super-fft.md 2.1) | Done: per-layer silence (bug fixed), Hop-Scaled refresh, layer threads 1..6, folded gains, layer-1 features, history kept across setting changes, Reset, F10 guards. Open: AVX2 kernels |
| Concentric spectrum view (ZER0 circular scope) | Done in the Studio preview: outer/inner lines, outer/inner/spanning bars, bar inset, base circle, up to 4,096 ring bins (scene format v2). Native wgpu version with A9 |
| Raw audio scope (tapered one-frame trace, linear and inside the ring with mirrors and rotational copies) | Done in the Studio preview: wire v2 waveform block, half-sine taper in the engine, linear scope in bar scenes, ring traces with both mirrors and 1-8 rotational copies |

## Phase 0.5, Track B (platform spikes)

- P0.3 WASAPI loopback: decided (cpal 0.18), live system audio and microphone in the Studio; format matrix, device-loss recovery and soak still open (docs/spikes/p0.3-loopback.md).
- P0.2 overlay, P0.6 lifecycle, P0.7 latency, P0.8 Background, P0.9 Taskbar: not started.

## Known gaps

- Analysis runs at the source rate; resampling to 48 kHz arrives with live capture.
- Timestamps use the sample-count clock; device-clock anchoring arrives with WASAPI.
- The analysis worker polls every quarter hop while active; deep-idle parking is Phase 1B.6.
- Scene editing is not in the Studio yet: the display bar count is a preview-only control.
- Linux and macOS: compile and unit tests pass in CI; nothing has been run interactively there.
