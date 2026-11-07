# Performance record

Every number here has a command and a machine (README section 6.2). Newest first.

## DSP: parallel FFT layers (ADR-020, 2026-11-07)

Command: `cargo run --release -p av-engine --example layer_timing` (3,000 hops after 100 warm-up,
48 bars, Kaiser beta 15, no EQ). Machine: Intel Core i9-13900H, Windows 11 Pro 26300, AC power.
Parallel = layer 0 on the analysis thread, layers 1.. on their own threads (park/unpark hand-off).

| Config | Seq p50 | Seq p99 | Par p50 | Par p99 | Speed-up (p50) |
| :-- | --: | --: | --: | --: | --: |
| L=2 N=16,384 | 126.7 us | 209.7 us | 75.2 us | 133.3 us | 1.68x |
| L=4 N=16,384 | 239.5 us | 364.3 us | 130.5 us | 218.4 us | 1.84x |
| L=2 N=32,768 | 270.0 us | 473.8 us | 253.1 us | 300.8 us | 1.07x |
| L=3 N=32,768 | 394.3 us | 668.8 us | 262.0 us | 327.6 us | 1.50x |
| **L=4 N=32,768 (default)** | 543.8 us | 874.4 us | 269.5 us | 349.5 us | 2.02x |
| L=5 N=32,768 | 685.6 us | 1,087.8 us | 279.6 us | 393.1 us | 2.45x |
| L=6 N=32,768 | 838.7 us | 1,320.0 us | 296.6 us | 444.8 us | 2.83x |
| L=4 N=65,536 | 1,172.3 us | 1,785.2 us | 560.4 us | 674.9 us | 2.09x |
| L=6 N=65,536 | 1,788.2 us | 2,630.4 us | 586.2 us | 783.8 us | 3.05x |

The engine uses the parallel runner from 3 layers up. Every configuration fits the 2 ms budget
in parallel; sequentially L=6 N=65,536 does not. Allocation gate: `cargo test -p av-engine --test
alloc` (parallel dispatch and collect, 500 hops, zero allocations on the analysis thread).

## DSP: CHOP chain with layers (ADR-021), 2026-11-08

Commands: `cargo run --release -p av-dsp --example dsp_timing` (sequential) and
`cargo run --release -p av-engine --example layer_timing`. Same machine. Layers compute
magnitudes only up to the warp's highest bin.

| Config | Sequential p50 | Sequential p99 | Parallel p50 | Parallel p99 |
| :-- | --: | --: | --: | --: |
| **app default** (4 layers, N = 32,768, 1,024 log bins) | 197.9 us | 358.3 us | 112.5 us | 180.5 us |
| 6 layers, N = 32,768 | 311.3 us | 540.8 us | 134.7 us | 302.8 us |
| 6 layers, N = 65,536 | 684.1 us | 1,071.4 us | 286.2 us | 498.3 us |
| classic 2,048 / 512 | 7.3 us | 11.4 us | - | - |

Before this change the default was 874 us (sequential) and 350 us (parallel) at p99. Allocation
gates: `cargo test -p av-dsp --test alloc` (default, 6 layers, classic, and every optional stage
on) and `cargo test -p av-engine --test alloc`.

## DSP: layered FFT (ADR-020), first pass

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
