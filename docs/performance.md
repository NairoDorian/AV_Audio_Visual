# Performance record

Every number here has a command and a machine (README section 6.2). Newest first.

## DSP: layered FFT (ADR-020)

Command: `cargo run --release -p av-dsp --example dsp_timing` (5,000 hops after 100 warm-up, 128
bars, per-bin levels when within the wire limit, Hann, hop 512 for layered presets).
Machine: Intel Core i9-13900H, Windows 11 Pro build 26300, AC power.

| Config | FFT size | p50 | p99 | max |
| :-- | --: | --: | --: | --: |
| classic 1024 / 256 | 1,024 | 6.6 us | 12.3 us | 62.8 us |
| classic 2048 / 512 | 2,048 | 12.4 us | 21.8 us | 61.5 us |
| classic 8192 / 1024 | 8,192 | 42.7 us | 76.1 us | 183.9 us |
| layered L=2 pad=2 | 4,096 | 29.2 us | 51.1 us | 228.9 us |
| layered L=3 pad=2 | 8,192 | 76.6 us | 133.5 us | 284.0 us |
| **layered L=4 pad=2 (default)** | 16,384 | 173.8 us | 325.1 us | 569.7 us |
| layered L=4 pad=4 | 32,768 | 354.3 us | 656.0 us | 1,683.5 us |
| layered L=5 pad=2 | 32,768 | 421.4 us | 753.7 us | 963.2 us |
| layered L=5 pad=4 | 65,536 | 907.8 us | 1,503.8 us | 2,257.2 us |
| layered L=6 pad=1 | 32,768 | 520.2 us | 883.2 us | 1,179.5 us |
| layered L=6 pad=2 | 65,536 | 1,134.6 us | 1,763.6 us | 2,343.7 us |
| layered L=6 pad=4 | 131,072 | 2,363.3 us | 3,758.5 us | 6,220.3 us |

Budget 2 ms per hop: every configuration except L=6 pad=4 fits at p99; L=6 pad=2 is tight.
Cost is L FFTs of size N; the planned per-layer FFT size optimization (docs/super-fft.md 1.5)
targets the large configurations.

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
