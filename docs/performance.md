# Performance record

Every number here has a command and a machine (README section 6.2). Newest first.

## DSP: analyzer hop (Track A step A6)

Command: `cargo run --release -p av-dsp --example dsp_timing` (20,000 hops after 100 warm-up,
128 bars, per-bin levels on, Hann, third-octave reference).
Machine: Intel Core i9-13900H, Windows 11 Pro build 26300, AC power. Commit: A6.

| Preset | FFT / hop | p50 | p99 | max |
| :-- | :-- | --: | --: | --: |
| Default | 2048 / 512 | 11.9 us | 18.6 us | 104.1 us |
| Low-latency | 1024 / 256 | 6.2 us | 9.3 us | 63.9 us |
| Precision | 8192 / 1024 | 42.8 us | 62.1 us | 214.9 us |

Budget: < 2 ms per hop (AGENTS.md section 4). The default preset uses about 1% of it at p99.
Allocation gate: `cargo test -p av-dsp --test alloc` (zero allocations in 500 steady-state hops
after 20 warm-up, with and without per-bin output).

Criterion: `bun run bench` (`cargo bench -p av-dsp`).
