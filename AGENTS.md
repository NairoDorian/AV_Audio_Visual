# AV_AudioVisual - Agent and Contributor Rules

This file is the operating contract for anyone, human or AI, working on AV_AudioVisual. It is
derived from the working rules of the two Rust reference projects (CursorFX and ZER0/Handy_V2),
adapted to this product. Read it before changing code, tooling, dependencies or documentation.

The product is a cross-platform audio visualizer that can live in a desktop background, a
taskbar/menu bar, or a floating overlay. The working title is AV_AudioVisual; the final name is not
fixed. Never hand-write the product name in generated identity files once a single-source identity
module exists.

---

## 1. Primary standards

### 1.1 Bun only

Never use `npm`, `npx`, `yarn` or `pnpm`. Use **Bun** for dependency management, script execution
and tooling.

### 1.2 The only development launch command

```bash
bun run tauri dev
```

It starts Vite and the Tauri app with the front end wired up. Never run a bare debug binary
(`target/debug/...exe`). A debug build has no front end bundled; it can load an opaque webview error
page into a transparent full-desktop window. CursorFX documents three Windows reboots caused by
this. If the app has a preflight helper, do not weaken it.

### 1.3 RTK command prefix

Prefix shell commands with **`rtk`**. RTK condenses output while preserving every signal. If a
dedicated filter exists, it is used; otherwise the command passes through unchanged. RTK is always
safe.

```bash
rtk git status
rtk git diff
rtk cargo check --workspace
rtk cargo test --workspace
```

Sanctioned Bun scripts run directly, without `rtk`:

```bash
bun run tauri dev
bun run typecheck
bun run lint
bun run test
bun run update-deps
bun run precommit
```

Chains must also use RTK:

```bash
rtk git add . && rtk git commit -m "feat: ..." && rtk git push
```

If an RTK result is unusable (empty when output was expected, contradicts its exit code, or is
garbled), re-run it once as `rtk proxy <command>`. Do not disable RTK globally.

### 1.4 Always-newest dependency policy

**Always use the newest published version of every dependency: the prerelease or the latest,
whichever is newer.** This covers npm packages, crates, Bun itself (canary) and GitHub Actions.
There is no stable-only mode. A hold is allowed only as an entry in `scripts/update-policy.ts`
with a one-sentence reason, and every update report prints every hold (ADR-019). The Rust
toolchain tracks the newest stable release; that is the single documented exception.

`bun run update-deps` is the only supported updater. It is **ported from the reference
updaters**, ZER0 `scripts/update-deps.ts` + `update-all.ts` and CursorFX `scripts/update-deps.ts`,
and adapted to this workspace as specified in `docs/phase-1a-blueprint.md` section 8.1. Do not
write a new updater from scratch, and do not drop a safety rule from the references when porting
(line pins, lockstep groups, cargo conflict retry, clobber guard, query deadline).

**Tauri 3, always newest.** The app targets Tauri 3, not Tauri 2. Every update takes the newest
published 3.x version for the whole Tauri family, prereleases included. Do not fall back to Tauri 2
and do not hold a stable Tauri 3 version when a newer 3.x prerelease exists. Tauri 4 requires an ADR
and a coordinated family migration.

The updater must:

1. Probe npm dist-tags (`next`, `beta`, `rc`, `alpha`, `canary`, `experimental`, `insiders`, `dev`)
   and the full version list, and never silently downgrade.
2. Probe Crates.io `newest_version` for workspace manifests and update the lockfiles.
3. Pin the Tauri family **as a set**. A partial bump can resolve and fail to compile; ZER0
   documents three such failures. Moving Tauri majors is a coordinated family migration.
4. Run validation as part of the update: typecheck, production Vite build, `cargo check`, and the
   fast test suite. A lockfile that resolves is not a lockfile that builds.
5. Print a report of every package held back and why (a `LINE_PINNED`-style exception).
6. Assert that every resolved Tauri crate is 3.x and report the selected family version. Fail the
   update if Tauri drifts to 2.x or 4.x without an explicit ADR.
7. Move the Tauri family as **one transaction** (crates, `@tauri-apps/cli`, `@tauri-apps/api`,
   plugins); `@tauri-apps/cli` is always the same release as the `tauri` crate because it
   validates `tauri.conf.json` before cargo runs. Validate the config against the installed CLI's
   schema.
8. Snapshot every manifest and lockfile first and **restore them** if any step fails.

`bun run update` is the routine entry point (rtk -> Bun canary -> `update-deps` -> Actions audit ->
repomix). A bare `bun run update-deps` runs the same newest-only pipeline; there is no flag that
selects stable-only versions.

Two registry facts the updater must respect (verified 2026-11-07, see
`docs/phase-1a-blueprint.md` section 1):

- **Dist-tags are hints, not truth.** A `next` tag can point at an older build than `latest`
  (`@solidjs/vite-plugin`), and `latest` can point at an ancient prerelease (`@solidjs/web`). Take
  the highest semver across all published versions on the allowed channels; never move backwards.
  Keep `solid-js` and `@solidjs/web` at the identical version.
- **Tauri family members have different numbers.** Resolve the family from `tauri`'s own
  dependency requirements (`tauri-build`, `tauri-runtime`, `tauri-utils`, plugins), not by forcing
  one number. Use the crates.io v3 plugin releases, not the `plugins-workspace` git branch.

Do not add patched forks of foundational crates (`specta`, `tauri-specta`, plugins) unless upstream
cannot do the job. Forks accumulate merge debt and break `cargo update`. If a fork is unavoidable,
write an ADR in `docs/`, pin it by commit `rev` (never a branch), pin the whole family to one
source, and state the exit criterion. Upstream `tauri-specta` (crates.io rc.26, `main` rc.27)
requires `tauri ^2`, so typed command generation on Tauri 3 is currently such a case. ADR-014: use
`NairoDorian/tauri-specta-v3` (local clone `C:\Users\Z\Downloads\PROJECTS\tauri-specta-v3`, which
may be edited and pushed when it needs updating), rebased onto upstream and pinned by `rev`; switch
back to crates.io as soon as a release declares `tauri ^3`.

### 1.5 Pre-commit gate

The gate is one command:

```bash
bun run hooks:install      # once per clone
bun run precommit          # identity -> translations -> lint -> typecheck -> unit -> format -> repomix
bun run precommit:full     # the same plus clippy and the Rust test suite
bun run precommit:routine  # update everything to newest prerelease, then precommit:full
```

The fast gate runs on every commit. `precommit:full` runs before releases, cross-OS changes and
pull requests. The hook reads the working tree, not the index, and may stage only generated
identity mirrors. It is not a substitute for reading the diff.

Phase 1 acceptance requires these scripts to exist and to run on Windows, macOS and Linux:

- `scripts/update-deps.ts`
- `scripts/pre-commit.ts`
- `scripts/app-meta.ts` (or equivalent identity mirror)
- `.githooks/pre-commit`
- `package.json` script entries for `update`, `update-deps`, `precommit`, `precommit:full`,
  `precommit:routine`, `hooks:install`.

Do not write a PowerShell-only hook or a Windows-only update script.

---

## 2. Commit and push discipline

### 2.1 Before every commit

1. Run `rtk git status` and `rtk git diff`. Read the whole diff.
2. Confirm no secrets, `.env` files, signing material, model weights, installers or generated
   caches are staged.
3. Run `bun run precommit`. For a feature push, run `bun run precommit:full`.
4. If the change affects audio, render or window lifecycle, run the relevant manual check on the
   target OS and record it in the commit message.
5. If the change touches a reference-derived technique, confirm the licence is compatible and note
   the conceptual source.

### 2.2 Commit shape

- One feature, fix or refactor per commit. Do not bundle unrelated work.
- A commit message states:
  - **what** changed,
  - **why** it changed,
  - the **cost** (new thread, poll, dependency, per-frame allocation, etc.),
  - the **evidence** (test, benchmark, capture, manual check).
- Use an imperative subject line. Keep it searchable.
- Never amend a commit that has already been pushed. Fix with a new commit.
- Never force-push `main`.

Example:

```text
audio: add WASAPI loopback backend

Adds a shared-mode, event-driven loopback capture path for the default
render endpoint behind CaptureBackend. Rebuilds on device invalidation and
publishes silent frames when nothing is playing.

Cost: one capture thread, one 2 s ring, no per-frame allocation.
Tested: cargo test -p av-audio; manual playback on Windows 11.
```

### 2.3 Main-branch policy

Direct pushes to `main` are allowed only for the solo maintainer, only when the full gate is green,
and never with `--force`. Releases are tagged after the gate. If contributors join, switch to
short-lived branches and pull requests; do not rewrite shared history.

---

## 3. Architecture invariants

These are not suggestions. Breaking one requires an ADR and a measurable reason.

1. **Capture, DSP, engine, protocol, render and platform hosts are separate crates.** The allowed
   dependency edges are listed in `docs/phase-1a-blueprint.md` section 3.1 and checked by the
   pre-commit gate; `av-dsp` stays pure (no threads, clocks, I/O or logging). Do not create a god
   module. ZER0's 4,057-line settings module and Tourne'Table's 19,934-line translation unit are
   the cautionary examples.
2. **The audio callback is real-time.** No allocation, no locks, no logging, no FFT, no device
   enumeration, no channel send that can block. Push interleaved f32 into a bounded `rtrb` ring and
   return.
3. **One analysis worker per active source.** Every window reads the same latest frame. Never run
   one FFT per window.
4. **Frames are binary, not JSON.** Use an 8-word header, a sequence number, flags (with the wire
   version in bits 24-31), f32 bins and optional features. Return a header-only reply when the
   client already has that sequence. The timestamp is the device-clock time of the newest analysed
   sample, not the encode time.
5. **The frontend draws from a latest-frame bus on `requestAnimationFrame`.** Do not drive a Solid
   store or the layout from a 60 Hz event stream. In-process consumers read a preallocated frame
   slot ring; publishing a frame never allocates (no `Arc<Frame>` per hop).
6. **Display settings never restart the capture stream.** Bump a generation counter and rebuild
   the DSP at the next hop boundary. Keep the last frame visible until the new pipeline produces
   one.
7. **Silence is published as a silent frame.** The UI decays to zero; it must not freeze on the
   last loud frame or stop the stream.
8. **One window and one surface per monitor.** Use physical-pixel geometry, a stable monitor device
   identity, and a per-target uniform buffer. `write_buffer` is queue-ordered; a shared uniform
   gives later monitors the wrong origin.
9. **Transparent surfaces use premultiplied alpha and a non-sRGB UNORM target on Windows.** An
   `*_Srgb` target encodes `c*a` after blending and produces invalid premultiplied values.
10. **The render thread never blocks on the UI thread.** Do not query Tauri window state,
    monitors or DPI per frame. Poll at 1 Hz or on events.
11. **Pacing is per surface.** A blocking Fifo acquire is the clock for surfaces that use Fifo.
    Choose Fifo or Mailbox/carried deadlines per target; never gate a Fifo acquire on a second
    per-target deadline.
12. **Release a `SurfaceTexture` before its `Surface`.** Declare `pending` first and call
    `release_pending()` on every reconfigure/drop path. Vulkan panics otherwise.
13. **Visualizer windows are webview-less; the Studio is the only webview.** This removes the
    opaque-error-page hazard at the source (ADR-013). Debug preflight still fails before the Studio
    or any fallback `viz-` webview window exists if Vite is not running. A visualizer webview, if
    ADR-013's fallback is ever used, loads an empty dedicated entry, never the Studio bundle.
14. **All window geometry and monitor access hops to the Tauri main thread** on platforms where the
    window API requires it (notably Linux/GTK). This is a crash fix, not a style preference.
15. **Every window and surface has a Drop guard and a panic-path cleanup.** Create hosts hidden,
    present the first frame, then show; hide before destroying the surface. A transparent
    always-on-top window outliving its renderer covers the desktop.
16. **No injection into Explorer, Finder or a compositor.** The visualizer is its own process. A
    crash must not take down the shell.
17. **The three placements (Background, Taskbar, Overlay) share the renderer and engine, never
    window flags.** Geometry is an anchor (monitor ID + anchor + offset + size in DIPs), saved at
    the end of a drag, never per move. Show/hide, click-through and layout-edit always have a
    global hotkey and a tray item, so no surface can lock the user out. Never use undocumented
    z-band APIs (`CreateWindowInBandEx`, `SetWindowBand`).
18. **Every host integration has a fallback and a visible error state.** Never silently fall back
    from system audio to the microphone. Never assume `always-on-top` means the same thing on every
    compositor.
19. **Config flows migrate -> self-heal -> sanitize -> normalize.** Save atomically (temp file +
    rename) with a debounce, and flush on exit.
20. **Scenes and share codes exclude machine-specific settings** (audio device, monitor, paths,
    hotkeys, GPU) and carry a format version.
21. **GPL and unlicensed code is never copied.** Architecture study is allowed; independent
    implementation is required. See `references/README.md` for the licence map.
22. **Panic strategy is explicit and tested.** The v1 default is `panic = "unwind"` so Drop guards
    run. `panic = "abort"` requires ADR-001 and a tested panic hook that destroys every window.

---

## 4. Performance-first rules

Read `references/studies/zer0-audio-capture-and-fft.md` and the performance sections of the other
studies before adding a thread, poll, event, dependency or per-frame allocation. State the cost in
the commit message.

- **Hot toggles are atomics**, not mutexes on the audio path.
- **Commands never block the webview** on audio, GPU or shell work; offload and reply.
- **Events are throttled and gated.** No 60 Hz event per frame. Use a latest-frame mailbox.
- **Resources stay warm.** Cache FFT plans, windows, band maps, gradients, fonts, paths, GPU
  pipelines and decoded icons. Rebuild only on a real config/resize/theme/device change.
- **Meters sample on one thread** at a low rate; do not poll CPU/GPU from the render loop.
- **Child processes run detached** on a supervisor thread and are killed on session end.
- **Idle parks the loop.** Settle frames, then wait on a condvar/event with a timeout. Deep idle
  stops loopback and polls a peak meter at a low rate.
- **Occlusion and fullscreen are optimizations, not correctness requirements.** Pause rendering
  when covered, but never rely on the heuristic to keep the app alive.
- **Benchmark the whole path.** Separate capture, DSP, submission and presentation costs. Measure
  before optimizing; CursorFX optimized its trail before discovering the presentation path was the
  bottleneck.
- **No per-frame allocation in DSP or rendering.** Preallocate buffers and reuse them. `Vec::new`
  in a hot loop is a bug.

Suggested budgets (see README for the full table): audio callback < 0.5 ms, DSP < 2 ms per
2048-sample hop at 48 kHz, render CPU < 2 ms at 60 fps, idle < 1% CPU.

---

## 5. Logging and diagnostics

- One session log with rotation. Console and file capture every severity in every build profile
  during development; release defaults to Info.
- `RUST_LOG` filters the console view, not the durable capture.
- Do not log per audio callback or per FFT frame. Aggregate overruns, device changes and DSP timing;
  log once per session or per state change.
- Expose diagnostics in the UI: source, sample rate, channels, FFT size, hop, update rate, dropped
  samples, DSP microseconds, frame time, present mode, device changes.
- A diagnostics export must never include raw audio, secrets or personal paths without consent.

---

## 6. Testing and evidence

- **Numerical behavior is pinned by tests.** Unit tests for FFT/DSP, parity fixtures between Rust
  and TypeScript defaults, golden frames for ballistics and bands.
- **Visual behavior is confirmed on a capture from the running GPU app.** Never approve a visual
  change from a CPU rasterizer; it does not model premultiplied compositing, feathered edges,
  depth pre-passes or DWM behavior.
- **Performance claims need a benchmark command and a measured number.**
- **Cross-OS claims need a manual check on that OS** or an explicit "unverified" label.
- **Preset/config changes need a migration test** from the previous schema and a partial-store
  test.
- **New numeric fields need a clamp and a parity test.**
- **A bug that reaches the renderer or window host gets a regression test** where possible, and a
  guard (Drop, panic hook, preflight) where not.

---

## 7. Research and reference rules

The `references/` folder is evidence, not a vendored dependency.

- Do not modify reference projects.
- Before implementing a subsystem, read the matching study in `references/studies/` and the
  relevant section of `references/README.md`.
- For new reference research, use parallel agents. Assign one agent per project or coherent group,
  not one per 1 MB source file. Each agent must:
  - read the real source, not just the README;
  - cite `path/to/file:line` for every important claim;
  - separate "what the source does" from "what we should do";
  - check the licence in the header;
  - write a study note under `references/studies/` and return a short summary plus the highest-value
    references.
- When a reference updates, re-run the study and refresh the line appendix. Do not keep stale line
  numbers silently.
- When a technique is adopted, record the conceptual source in the commit message or a module
  comment.
- Never copy GPL-3.0, GPL-2.0-only or unlicensed code. The licence map is in
  `references/README.md`.

---

## 8. Documentation rules

- `README.md` is the product vision, architecture direction and roadmap. Keep it truthful.
- `references/README.md` is the master index of references and study notes. Update it whenever a
  reference or study is added.
- `AGENTS.md` (this file) is the workflow contract. Update it when a rule changes, not silently in
  a commit.
- Add an ADR under `docs/` for any decision that changes an architecture invariant, the dependency
  policy or the licence direction.
- Keep `PROGRESS.md` (once created) truthful about what is implemented versus planned.
- Regenerate `ARCHITECTURE.md` from the code when a generator exists; do not hand-maintain a second
  source of truth.
- Generated bindings (`src/bindings.ts`) are never edited by hand. Regenerate and fail CI on drift.

---

## 9. Security and privacy

- No telemetry, account or network call by default. Lyrics, album art and update checks are opt-in
  or explicitly disclosed.
- No raw audio leaves the machine. Analysis is local.
- Never log device names, file paths or media metadata in release logs without consent.
- Validate every persisted value. Treat scene files, share codes and imported palettes as
  untrusted input.
- Do not use `unsafe` for convenience. Every `unsafe` block needs a safety comment and a test.
- Do not weaken CSP, capabilities or `webview_hardening` to make a feature work.

---

## 10. Definition of done for a feature

- [ ] Behavior implemented behind the correct crate/module boundary.
- [ ] Audio path remains allocation-free and lock-free.
- [ ] Config/scene changes have defaults, migration, sanitize and parity tests.
- [ ] Unit tests pass; `cargo test --workspace` and `bun run test` pass.
- [ ] `bun run precommit:full` passes.
- [ ] Visual change confirmed on a real GPU capture.
- [ ] Cross-OS impact stated; each affected OS checked or marked unverified.
- [ ] Performance cost stated and measured if non-trivial.
- [ ] Docs/study notes updated if a reference-derived technique or public behavior changed.
- [ ] Licence compatible; no GPL or unlicensed code copied.
- [ ] Commit message explains what, why, cost and evidence.
