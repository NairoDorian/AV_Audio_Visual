//! One analysis worker per active source (AGENTS.md invariant 3).
//!
//! The worker (`av-analysis-<name>`) drains the capture ring, runs the [`Pipeline`] and publishes
//! into the slot ring. Configuration changes arrive as ready-built analyzers (plans and buffers
//! are built on the caller's thread) and are swapped at a hop boundary with a new generation;
//! the capture stream is never restarted (invariant 6). A starved source publishes silent
//! frames at the hop rate so the display decays instead of freezing (invariant 7).
//!
//! Cost: one thread. While data flows it wakes every quarter hop (~2.7 ms at 512/48 kHz) to
//! drain the ring; deep idle (parking) arrives with the idle state machine in Phase 1B.6.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use av_audio::{AudioSource, CaptureFormat};
use av_dsp::{Analyzer, DspError, PlanCache, SpectrumConfig};

use crate::pipeline::Pipeline;
use crate::slot_ring::{FrameReader, slot_ring};
use crate::status::{ClockSource, StatusSnapshot, monotonic_us};

/// How often the status snapshot is replaced.
pub const STATUS_PERIOD: Duration = Duration::from_millis(250);
/// A source silent for this many hops is treated as starved.
const STARVED_AFTER_HOPS: u32 = 2;

#[derive(Debug)]
pub enum EngineError {
    Dsp(DspError),
    Thread(std::io::Error),
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Dsp(e) => write!(f, "invalid analysis settings: {e}"),
            Self::Thread(e) => write!(f, "cannot start the analysis thread: {e}"),
        }
    }
}

impl std::error::Error for EngineError {}

impl From<DspError> for EngineError {
    fn from(e: DspError) -> Self {
        Self::Dsp(e)
    }
}

enum Control {
    Swap(Box<Analyzer>, u32),
    /// The FFT CHOP's Reset, acted on at the next hop boundary (audit F11).
    Reset,
}

/// A running analysis worker for one source.
pub struct Engine {
    reader: FrameReader,
    status: Arc<ArcSwap<StatusSnapshot>>,
    control: Sender<Control>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    plans: Mutex<PlanCache>,
    generation: AtomicU32,
    format: CaptureFormat,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("format", &self.format)
            .field("generation", &self.generation.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl Engine {
    /// Starts analysing `source`. `cfg.sample_rate` is replaced by the source rate (resampling
    /// to 48 kHz arrives with live capture; the analyzer handles any rate).
    pub fn start(
        name: &str,
        source: AudioSource,
        format: CaptureFormat,
        cfg: SpectrumConfig,
    ) -> Result<Self, EngineError> {
        let mut plans = PlanCache::default();
        let analyzer = build(&mut plans, cfg, format)?;
        let (writer, reader) = slot_ring();
        let status = Arc::new(ArcSwap::from_pointee(StatusSnapshot::default()));
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let pipeline = Pipeline::new(analyzer, 1, format.channels, monotonic_us());
        let worker = Worker {
            pipeline,
            writer,
            source,
            rx,
            status: status.clone(),
            stop: stop.clone(),
            format,
        };
        let thread = std::thread::Builder::new()
            .name(format!("av-analysis-{name}"))
            .spawn(move || worker.run())
            .map_err(EngineError::Thread)?;
        Ok(Self {
            reader,
            status,
            control: tx,
            stop,
            thread: Some(thread),
            plans: Mutex::new(plans),
            generation: AtomicU32::new(1),
            format,
        })
    }

    pub fn reader(&self) -> FrameReader {
        self.reader.clone()
    }

    pub fn status(&self) -> Arc<StatusSnapshot> {
        self.status.load_full()
    }

    /// Builds a new analyzer on this thread and hands it to the worker, which swaps it in at the
    /// next hop boundary. Returns the new generation.
    pub fn reconfigure(&self, cfg: SpectrumConfig) -> Result<u32, EngineError> {
        let analyzer = {
            let mut plans = self.plans.lock().unwrap_or_else(PoisonError::into_inner);
            build(&mut plans, cfg, self.format)?
        };
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        // A closed channel means the worker already stopped; nothing to reconfigure.
        let _ = self
            .control
            .send(Control::Swap(Box::new(analyzer), generation));
        Ok(generation)
    }

    /// Clears the filter, ballistics, AGC and feature state at the next hop boundary.
    pub fn reset(&self) {
        let _ = self.control.send(Control::Reset);
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Resolves `cfg` at the source rate (window in milliseconds, Nyquist-clamped axis) and builds
/// the analyzer with the configured layer runner.
fn build(
    plans: &mut PlanCache,
    cfg: SpectrumConfig,
    format: CaptureFormat,
) -> Result<Analyzer, DspError> {
    let cfg = cfg.at_rate(format.sample_rate);
    let threads = cfg.settings.layer_threads as usize;
    let plan = plans.get(cfg.fft_size);
    Analyzer::with_runner(cfg, plan, move |units| layer_runner(units, threads))
}

/// With `layerThreads` = 0 (auto), layers from this count up run one per thread: measured 1.85x
/// faster at the default (4 layers, N = 32,768) and 2.5x at 6 layers; with 2 layers the hand-off
/// costs what it saves (`examples/layer_timing.rs`, docs/performance.md).
pub const PARALLEL_FROM_LAYERS: usize = 3;

/// Threads used for `layers` layers and the `layerThreads` setting (0 = auto).
pub fn layer_threads(layers: usize, setting: usize) -> usize {
    match setting {
        0 if layers >= PARALLEL_FROM_LAYERS => layers,
        0 => 1,
        n => n.min(layers).max(1),
    }
}

fn layer_runner(units: Vec<av_dsp::LayerUnit>, setting: usize) -> Box<dyn av_dsp::LayerRunner> {
    let threads = layer_threads(units.len(), setting);
    if threads > 1 {
        return Box::new(crate::parallel::ParallelLayers::with_threads(
            units, threads,
        ));
    }
    Box::new(av_dsp::SequentialLayers::new(units))
}

struct Worker {
    pipeline: Pipeline,
    writer: crate::slot_ring::SlotWriter,
    source: AudioSource,
    rx: Receiver<Control>,
    status: Arc<ArcSwap<StatusSnapshot>>,
    stop: Arc<AtomicBool>,
    format: CaptureFormat,
}

impl Worker {
    fn run(mut self) {
        let ch = self.format.channels.max(1) as usize;
        // Big enough for several hops per wake-up; allocated once.
        let mut buf = vec![0.0f32; 8192 * ch];
        let counters = self.source.counters().clone();
        let mut seen_dropped = 0u64;
        let mut last_data = Instant::now();
        let mut last_status = Instant::now();
        let mut starved_hops = 0u64;
        let mut silent = false;

        while !self.stop.load(Ordering::Relaxed) {
            while let Ok(control) = self.rx.try_recv() {
                match control {
                    Control::Swap(analyzer, generation) => {
                        drop(self.pipeline.swap(*analyzer, generation));
                    }
                    Control::Reset => self.pipeline.reset(),
                }
            }

            let dropped = counters.dropped_samples.load(Ordering::Relaxed);
            if dropped != seen_dropped {
                self.pipeline.on_gap((dropped - seen_dropped) / ch as u64);
                seen_dropped = dropped;
            }

            let hop = self.pipeline.analyzer().config().hop_samples;
            let hop_time = Duration::from_secs_f64(hop as f64 / self.format.sample_rate as f64);
            let n = self.source.pop(&mut buf);
            if n > 0 {
                self.pipeline.feed(&buf[..n], &mut self.writer);
                last_data = Instant::now();
                silent = false;
            } else if last_data.elapsed() >= hop_time * STARVED_AFTER_HOPS {
                self.pipeline.feed_silence(&mut self.writer);
                last_data += hop_time;
                starved_hops += 1;
                silent = true;
            }

            if last_status.elapsed() >= STATUS_PERIOD {
                self.publish_status(starved_hops, silent, &counters);
                last_status = Instant::now();
            }
            if n < buf.len() {
                std::thread::sleep(hop_time / 4);
            }
        }
        let mut s = (**self.status.load()).clone();
        s.running = false;
        self.status.store(Arc::new(s));
    }

    fn publish_status(
        &mut self,
        starved_hops: u64,
        silent: bool,
        counters: &av_audio::RingCounters,
    ) {
        let stats = self.pipeline.stats();
        let cfg = self.pipeline.analyzer().config();
        let previous = self.status.load();
        let snapshot = StatusSnapshot {
            running: true,
            source_rate: self.format.sample_rate,
            channels: self.format.channels,
            fft_size: cfg.fft_size as u32,
            hop_samples: cfg.hop_samples as u32,
            bar_count: cfg.output_bins() as u32,
            layers: cfg.layers as u32,
            window_samples: cfg.window_samples as u32,
            threads: self.pipeline.analyzer().threads() as u32,
            peak_hz: stats.peak_hz,
            features: stats.features,
            generation: self.pipeline.generation(),
            clock: ClockSource::SampleCount,
            frames: previous.frames + stats.hops,
            dsp_us_avg: if stats.hops > 0 {
                (stats.total_us / stats.hops as f64) as f32
            } else {
                0.0
            },
            dsp_us_max: stats.max_us as f32,
            overruns: counters.overruns.load(Ordering::Relaxed),
            dropped_samples: counters.dropped_samples.load(Ordering::Relaxed),
            silent,
            starved_hops,
            input_peak_dbfs: 20.0 * stats.input_peak.max(1e-10).log10(),
            updated_us: monotonic_us(),
        };
        self.status.store(Arc::new(snapshot));
        self.pipeline.reset_stats();
    }
}
