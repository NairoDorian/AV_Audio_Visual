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
| A4 wire format | Done | `av-protocol` frame/wire v1, fixtures, `src/lib/frame/wire.ts` | 13 fixtures byte-identical in both directions |
| A5 config | Done | `AppConfig`/`Scene` v1, migrate -> heal -> sanitize -> normalize, atomic debounced store with backups, ADR-007 | 18 pipeline + 8 store tests; settings parity fixtures |
| A6 DSP | Done | Analyzer (window, DC blocker, bands, weighting, auto-gain, ballistics, peaks) | Pre-plan section 6 gates; zero allocations; p99 18.6 us/hop (`docs/performance.md`) |
| A7 engine | Done | rtrb ring, symphonia file source, analysis worker, 64-slot seqlock ring, status snapshot, preview encoder | Bit-exact golden frames; no torn reads; zero allocations on the frame path |
| A8 preview | Done | `get_frame` raw pulls, frame bus, preview canvas, demo track | Bus tests; manual run with `--demo` (moving bars captured) |
| A9 headless render | Not started | wgpu instanced bars, offscreen target, premultiplied invariant | - |
| A10 platform | Not started | Monitor ids, DPI, `SurfaceHost` state machine with a fake host | - |

## Analysis goal: Super FFT (ADR-020, docs/super-fft.md)

| Item | State |
| :-- | :-- |
| Layered FFT (1-6 layers, zero-padded, 1/2^(k-1) scaled, averaged) | Done: default preset (4 layers, zero-pad 2); gates and timings recorded. Optimizations (decimation, per-layer FFT size) open |
| FFT CHOP integration (Kaiser, warps, ITU-R 468, EQ, aggregation, output modes) | Kaiser and zero-padding done; warps, ITU-R 468, EQ, aggregation, output modes planned |
| Concentric spectrum view (ZER0 circular scope) | Done in the Studio preview (`concentric` built-in scene); native wgpu version with A9 |

## Phase 0.5, Track B (platform spikes)

P0.2 overlay, P0.3 WASAPI loopback, P0.6 lifecycle, P0.7 latency, P0.8 Background, P0.9 Taskbar:
not started.

## Known gaps

- Analysis runs at the source rate; resampling to 48 kHz arrives with live capture.
- Timestamps use the sample-count clock; device-clock anchoring arrives with WASAPI.
- The analysis worker polls every quarter hop while active; deep-idle parking is Phase 1B.6.
- A generation swap restarts ballistics from zero (a brief dip in the preview).
- Linux and macOS: compile and unit tests pass in CI; nothing has been run interactively there.
