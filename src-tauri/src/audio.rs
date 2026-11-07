//! The active audio source and its analysis engine, owned by the composition root.
//!
//! Sources start only when asked (the Studio's Audio section), so an idle app runs no capture
//! or analysis thread. Until live capture lands (spike P0.3 -> Phase 1B.2) the sources are the
//! built-in demo track and audio files. Analysis settings changes reconfigure the engine in
//! place (new generation); they never restart the source (AGENTS.md invariant 6).

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};

use av_audio::synth::demo_track;
use av_audio::{CaptureBackend, CaptureState, FileSource, LiveKind, LiveSource, ring};
use av_dsp::SpectrumConfig;
use av_engine::Engine;
use av_engine::preview::PreviewEncoder;
use av_protocol::config::AnalysisConfig;
use serde::{Deserialize, Serialize};

/// What the Studio asks to play.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SourceRequest {
    /// What the computer is playing (WASAPI loopback on Windows).
    System,
    /// The default microphone; only ever opened when asked for.
    Microphone,
    Demo,
    File {
        path: String,
    },
}

/// What is playing, for the Studio.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    pub kind: &'static str,
    /// File name only; full paths stay out of the UI and logs (AGENTS.md section 9).
    pub label: String,
    pub sample_rate: u32,
    pub channels: u16,
}

/// Serializable view of the engine status snapshot.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AudioStatus {
    pub source: Option<SourceInfo>,
    pub running: bool,
    pub fft_size: u32,
    pub hop_samples: u32,
    /// Output bins per frame.
    pub output_bins: u32,
    pub layers: u32,
    pub window_samples: u32,
    /// Layers run one per core.
    pub parallel: bool,
    /// Loudest output bin of the newest frame.
    pub peak_hz: f32,
    /// Spectral features of the newest frame, when enabled.
    pub features: Option<Features>,
    pub generation: u32,
    pub frames: u64,
    pub dsp_us_avg: f32,
    pub dsp_us_max: f32,
    pub overruns: u64,
    pub dropped_samples: u64,
    pub silent: bool,
    pub starved_hops: u64,
    /// Peak raw input over the last status window, dBFS (-200 for digital silence).
    pub input_peak_dbfs: f32,
    pub clock: &'static str,
    /// `running`, `finished`, `failed` (device lost) or `stopped`.
    pub capture: &'static str,
}

/// Spectral features (FFT CHOP "Spectral Features").
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Features {
    pub centroid_hz: f32,
    pub rolloff_hz: f32,
    pub flatness: f32,
    pub flux: f32,
    pub rms_db: f32,
    pub bass_db: f32,
    pub mid_db: f32,
    pub high_db: f32,
}

impl From<[f32; 8]> for Features {
    fn from(f: [f32; 8]) -> Self {
        Self {
            centroid_hz: f[0],
            rolloff_hz: f[1],
            flatness: f[2],
            flux: f[3],
            rms_db: f[4],
            bass_db: f[5],
            mid_db: f[6],
            high_db: f[7],
        }
    }
}

struct Active {
    // Field order is drop order: stop the source before the engine it feeds.
    source: Box<dyn CaptureBackend>,
    engine: Engine,
    info: SourceInfo,
}

#[derive(Default)]
pub struct AudioService {
    active: Mutex<Option<Active>>,
    encoder: Mutex<PreviewEncoder>,
}

impl std::fmt::Debug for AudioService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioService").finish_non_exhaustive()
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

const RING_SECONDS: f32 = 2.0;
const DEMO_RATE: u32 = 48_000;

impl AudioService {
    /// Replaces whatever is playing. Decoding happens here, on the command's thread.
    pub fn start(
        &self,
        request: SourceRequest,
        analysis: &AnalysisConfig,
    ) -> Result<SourceInfo, String> {
        self.stop();
        let (mut source, kind, label): (Box<dyn CaptureBackend>, _, _) = match request {
            SourceRequest::System | SourceRequest::Microphone => {
                let (live_kind, kind) = if matches!(request, SourceRequest::System) {
                    (LiveKind::SystemOutput, "system")
                } else {
                    (LiveKind::Microphone, "microphone")
                };
                let live = LiveSource::open(live_kind).map_err(|e| e.to_string())?;
                let label = live.device_label().to_string();
                (Box::new(live), kind, label)
            }
            SourceRequest::Demo => (
                Box::new(FileSource::new(demo_track(DEMO_RATE), true)),
                "demo",
                "Demo track".to_string(),
            ),
            SourceRequest::File { path } => {
                let path = PathBuf::from(path);
                let label = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "file".into());
                let source = FileSource::open(&path, true).map_err(|e| e.to_string())?;
                (Box::new(source), "file", label)
            }
        };
        let format = source.format();
        let (sink, ring_source) = ring(format.sample_rate, format.channels, RING_SECONDS);
        let cfg = SpectrumConfig::new(analysis.clone(), format.sample_rate);
        let engine = Engine::start(kind, ring_source, format, cfg).map_err(|e| e.to_string())?;
        source.start(sink).map_err(|e| e.to_string())?;
        let info = SourceInfo {
            kind,
            label,
            sample_rate: format.sample_rate,
            channels: format.channels,
        };
        tracing::info!(target: "av_app::audio", kind, rate = format.sample_rate, channels = format.channels, "source started");
        *lock(&self.active) = Some(Active {
            source,
            engine,
            info: info.clone(),
        });
        Ok(info)
    }

    pub fn stop(&self) {
        if let Some(mut active) = lock(&self.active).take() {
            active.source.stop();
            active.engine.stop();
            tracing::info!(target: "av_app::audio", "source stopped");
        }
    }

    /// Applies new analysis settings to the running engine, if any.
    pub fn reconfigure(&self, analysis: &AnalysisConfig) {
        if let Some(active) = lock(&self.active).as_ref() {
            let cfg = SpectrumConfig::new(analysis.clone(), active.info.sample_rate);
            match active.engine.reconfigure(cfg) {
                Ok(generation) => {
                    tracing::debug!(target: "av_app::audio", generation, "analysis reconfigured")
                }
                Err(e) => tracing::warn!(target: "av_app::audio", "reconfigure rejected: {e}"),
            }
        }
    }

    /// Wire bytes for the preview: the newest frame, or a header-only reply.
    pub fn frame(&self, known: Option<u32>, want_bins: bool) -> Vec<u8> {
        let reader = lock(&self.active).as_ref().map(|a| a.engine.reader());
        let mut encoder = lock(&self.encoder);
        match reader {
            Some(r) => encoder.encode(&r, known, want_bins).to_vec(),
            None => {
                // Nothing playing: an all-zero header-only reply (valid wire v1).
                let mut buf = vec![0u8; av_protocol::frame::HEADER_BYTES];
                let header = av_protocol::FrameHeader::default();
                let _ = av_protocol::wire::encode_header_only(&header, &mut buf);
                buf
            }
        }
    }

    pub fn status(&self) -> AudioStatus {
        let active = lock(&self.active);
        let Some(a) = active.as_ref() else {
            return AudioStatus::default();
        };
        let s = a.engine.status();
        AudioStatus {
            source: Some(a.info.clone()),
            running: s.running,
            fft_size: s.fft_size,
            hop_samples: s.hop_samples,
            output_bins: s.bar_count,
            layers: s.layers,
            window_samples: s.window_samples,
            parallel: s.parallel,
            peak_hz: s.peak_hz,
            features: s.features.map(Features::from),
            generation: s.generation,
            frames: s.frames,
            dsp_us_avg: s.dsp_us_avg,
            dsp_us_max: s.dsp_us_max,
            overruns: s.overruns,
            dropped_samples: s.dropped_samples,
            silent: s.silent,
            starved_hops: s.starved_hops,
            input_peak_dbfs: s.input_peak_dbfs,
            capture: match a.source.state() {
                CaptureState::Running => "running",
                CaptureState::Finished => "finished",
                CaptureState::Failed => "failed",
                CaptureState::Stopped => "stopped",
            },
            clock: match s.clock {
                av_engine::status::ClockSource::SampleCount => "sampleCount",
                av_engine::status::ClockSource::Device => "device",
            },
        }
    }
}
