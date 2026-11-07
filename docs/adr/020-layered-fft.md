# ADR-020: Layered FFT analysis

Status: accepted (2026-11-07). Plan and gates: [`docs/super-fft.md`](../super-fft.md).

## Context

A single FFT window trades reactivity against low-frequency precision. The product owner found
that averaging several FFTs with doubling windows, each scaled back to a common level, looks both
reactive and precise.

## Decision

The analyzer supports **1-6 layers**. Layer `k` windows the newest `1024 * 2^(k-1)` samples
(max 32,768) with the same window shape, is zero-padded to one common FFT size `N` (largest
window times a zero-pad factor of 1, 2 or 4), and contributes its magnitude spectrum divided by
`2^(k-1)` (window-length normalization) to a per-bin average. The averaged magnitude replaces the
single-window magnitude before band mapping; everything downstream (bands, weighting, dB,
auto-gain, ballistics) is unchanged. Band noise bandwidth uses the exact effective ENBW of the
averaged response, computed once per configuration.

The `layered` preset (4 layers, zero-pad 2, hop 512) becomes the default once its gates pass;
the classic single-window presets remain.

## Consequences

- Cost grows to `L` FFTs of size `N` per hop; budget and optimizations (layer decimation,
  per-layer FFT size) are measured and recorded before they change defaults.
- The FFT size can exceed the wire's bin capacity (8,192 / 4,097 bins); such frames carry bars
  only. The preview and renderer draw bars; raw bins are a diagnostic.
- The history ring holds the largest window (32,768 samples, 128 KB).
