# ADR-001: Panic strategy

Status: accepted (2026-11-07, Track A step A1)

## Decision

Build with `panic = "unwind"` in every profile. Install the panic hook as startup step 1, before
any window exists, and give it the app handle at step 7.

## Why

A transparent always-on-top window that outlives its renderer covers the desktop (CursorFX
documents three reboots). Unwinding runs every host and surface `Drop` guard; the hook is the
second line of defence and, once hosts exist, sends `HideAll` and destroys native host windows
directly if the host pump is dead.

## Consequences

- `Cargo.toml` `[profile.release] panic = "unwind"`.
- `src-tauri/src/panic.rs` installs the hook; `startup.rs` enforces the order at runtime
  (`enter(Step)` asserts sequence) and asserts the hook exists before the Studio window.
- `panic = "abort"` requires superseding this ADR with a tested hook that destroys every window.
