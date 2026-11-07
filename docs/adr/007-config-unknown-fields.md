# ADR-007: Config unknown-field policy

Status: accepted (2026-11-07, Track A step A5)

## Decision

Unknown fields are **ignored and the file is rewritten normalized**. They are listed once in the
load report and in the session log. A migration that needs an old field reads it from the raw
JSON before healing runs, so nothing a migration needs is lost.

## Pipeline

`av_protocol::pipeline::load`: parse -> migrate (raw JSON, before any clamp) -> self-heal (start
from defaults, accept each user leaf that still deserializes in its slot) -> sanitize (numeric
limit table: non-finite -> default, clamp, round integers) -> normalize (cross-field
invariants).

## Backups

Before the normalized form replaces the stored file, the original is copied to
`config.json.bak-<unix seconds>` when the load lost data or could not understand it: corrupt
input, healed (wrong-typed) fields, or a newer schema version. Missing fields and unknown keys
alone do not trigger a backup.

## Why

A sidecar for unknown fields only pays off with forward compatibility across downgrades, which
v1 does not promise. Field-level healing means one bad value never resets its neighbours.
