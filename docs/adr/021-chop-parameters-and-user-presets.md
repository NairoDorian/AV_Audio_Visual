# ADR-021: Every FFT CHOP parameter, one default, user presets

Status: accepted (2026-11-08). Supersedes the fixed FFT presets (ADR-017's preset table) and the
band-map output of the A6 analyzer. Amends ADR-020 (layers now sit in front of the CHOP chain).

## Context

The product owner asked for the analysis to expose every parameter of the FFT CHOP (their
TouchDesigner custom operator, the primary source of truth for this analyzer), each visible with
its full range, with no mandatory presets: one default, and the ability to save the current
settings as a named preset. ZER0 (`Handy_V2 src-tauri/src/live_fft/dsp.rs`, MIT, same author)
already ports the CHOP's chain to Rust.

## Decision

- **Config schema v2.** `AnalysisConfig` holds every CHOP parameter that applies to a visualizer:
  Spectrum (channels, raw RFFT bins, 7 scales, display max, output bins fixed/auto up to 65,537,
  warp blend, linear/cubic interpolation, off/peak/RMS aggregation, log floor, window in samples
  or milliseconds, zero-padding and its length), EQ (two shelves, Q, blend to 5), Window &
  Weighting (6 windows, Kaiser beta manual/auto, A/C/ITU-R 468, coherent-gain/full-scale
  normalization), Loudness & Ballistics (dB or linear, frame-peak/dBFS/AGC reference, dB range,
  ballistics in coefficients or milliseconds), Performance (spectral features, parallel layers).
  Plus this product's own: Super FFT layers, hop, AGC release. Hard ranges are the CHOP's clamps
  (wider than its sliders); the Studio shows both.
- **Not ported** (no meaning outside TouchDesigner): FFTW planner and backend, ingest mode,
  worker wake. Worker priority (MMCSS) waits for the platform module. "dB" and "dB normalized"
  are one mode: frames always carry 0..1 display values.
- **Analyzer chain** is the CHOP's, ported from ZER0's portable kernels: layers -> average ->
  magnitude normalization -> features -> warp -> weighting -> reference -> loudness ->
  ballistics. The per-bar band map (third-octave reference, ENBW integration) is removed: frames
  carry the CHOP's output bins, and each scene pools them to its own bar count (peak pooling).
- **Presets.** No fixed presets. One built-in `default` (the `AnalysisConfig` defaults). Users
  save named presets as `presets/<id>.json`, loaded through the same migrate/heal/sanitize/
  normalize pipeline. Applying a preset copies its settings; nothing tracks it afterwards.
- **Migration v1 -> v2** maps each old preset to its window and hop, layout to scale, bar edges
  to log floor / display max, dB floor and ceiling to the dB range, auto-gain to the AGC
  reference. A migrated store is backed up before it is rewritten.
- The saved zero-pad length is never raised to fit a longer window; the analyzer grows the
  transform at run time, so the user's choice returns when the window shrinks.

## Consequences

- Frames can carry up to 65,537 values. The slot ring reserves that per slot from the zeroing
  allocator, so untouched pages stay uncommitted (one `unsafe` cast, tested).
- Layers compute magnitudes only up to the highest bin the warp reads: the default dropped from
  874 to 319 us p99 sequential and from 350 to 181 us parallel.
- Pink noise no longer reads flat by construction (per-bin magnitude, not third-octave power);
  the default +24 dB high shelf is the CHOP's answer to that, and A/C/ITU-R 468 are available.
