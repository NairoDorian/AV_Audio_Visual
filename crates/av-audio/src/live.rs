//! Live capture through `cpal`: system output (WASAPI shared-mode loopback on the default render
//! endpoint) and the default microphone (all OSes). Phase 1B.2, decided by spike P0.3
//! (`docs/spikes/p0.3-loopback.md`).
//!
//! The stream is created and owned on an `av-capture-<kind>` thread (a `cpal::Stream` is not
//! `Send` on every platform). The data callback is real-time: it converts to f32 into a scratch
//! buffer allocated once, sized in whole frames so a dropped chunk never splits a frame, pushes
//! into the ring and returns. No allocation, lock or log. Device loss is flagged through an
//! atomic and surfaced by `state()`.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};

use crate::capture::{CaptureBackend, CaptureError, CaptureFormat, CaptureState};
use crate::ring::AudioSink;

/// Which live source. System output is never replaced by the microphone silently (AGENTS.md
/// invariant 18): each is opened only when asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveKind {
    SystemOutput,
    Microphone,
}

impl LiveKind {
    fn tag(self) -> &'static str {
        match self {
            Self::SystemOutput => "system",
            Self::Microphone => "mic",
        }
    }
}

const STOPPED: u8 = 0;
const RUNNING: u8 = 1;
const FAILED: u8 = 3;

/// Frames converted per push; the scratch holds this many frames of every channel.
const CHUNK_FRAMES: usize = 2048;
const OPEN_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug)]
pub struct LiveSource {
    kind: LiveKind,
    format: CaptureFormat,
    sample_format: SampleFormat,
    device_label: String,
    state: Arc<AtomicU8>,
    stop: Option<mpsc::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

fn default_device(
    kind: LiveKind,
) -> Result<(cpal::Device, cpal::SupportedStreamConfig), CaptureError> {
    let host = cpal::default_host();
    let device = match kind {
        LiveKind::SystemOutput => host.default_output_device(),
        LiveKind::Microphone => host.default_input_device(),
    }
    .ok_or(CaptureError::DeviceNotFound)?;
    let config = match kind {
        LiveKind::SystemOutput => device.default_output_config(),
        LiveKind::Microphone => device.default_input_config(),
    }
    .map_err(map_error)?;
    Ok((device, config))
}

fn map_error(e: cpal::Error) -> CaptureError {
    use cpal::ErrorKind as K;
    match e.kind() {
        K::DeviceNotAvailable => CaptureError::DeviceNotFound,
        K::PermissionDenied => CaptureError::PermissionDenied,
        K::UnsupportedConfig | K::UnsupportedOperation => {
            CaptureError::UnsupportedFormat(e.to_string())
        }
        K::HostUnavailable => CaptureError::BackendUnavailable(e.to_string()),
        _ => CaptureError::BackendUnavailable(e.to_string()),
    }
}

impl LiveSource {
    /// Probes the default device and its mix format without starting anything.
    pub fn open(kind: LiveKind) -> Result<Self, CaptureError> {
        let (device, config) = default_device(kind)?;
        let format = CaptureFormat {
            sample_rate: config.sample_rate(),
            channels: config.channels(),
        };
        let sample_format = config.sample_format();
        if !matches!(
            sample_format,
            SampleFormat::F32 | SampleFormat::I16 | SampleFormat::I32 | SampleFormat::F64
        ) {
            return Err(CaptureError::UnsupportedFormat(format!(
                "{sample_format:?}"
            )));
        }
        let device_label = device
            .description()
            .map(|d| d.to_string())
            .unwrap_or_else(|_| "default device".into());
        Ok(Self {
            kind,
            format,
            sample_format,
            device_label,
            state: Arc::new(AtomicU8::new(STOPPED)),
            stop: None,
            thread: None,
        })
    }

    pub fn kind(&self) -> LiveKind {
        self.kind
    }

    /// Human-readable device name, for the Studio only (never written to release logs).
    pub fn device_label(&self) -> &str {
        &self.device_label
    }
}

/// Builds and plays the input stream; the returned stream must stay alive on this thread.
fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut sink: AudioSink,
    state: Arc<AtomicU8>,
) -> Result<cpal::Stream, CaptureError>
where
    T: SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    let channels = config.channels.max(1) as usize;
    let mut scratch = vec![0.0f32; CHUNK_FRAMES * channels];
    let error_state = state.clone();
    let stream = device
        .build_input_stream::<T, _, _>(
            *config,
            move |data: &[T], _info| {
                for chunk in data.chunks(scratch.len()) {
                    let out = &mut scratch[..chunk.len()];
                    for (d, s) in out.iter_mut().zip(chunk) {
                        *d = s.to_sample::<f32>();
                    }
                    sink.push(out);
                }
            },
            move |err| {
                // Not the data callback: may run on any cpal thread. A rerouted default device
                // keeps streaming; anything that ends the stream marks the source failed.
                if err.kind() != cpal::ErrorKind::DeviceChanged {
                    error_state.store(FAILED, Ordering::SeqCst);
                }
            },
            Some(OPEN_TIMEOUT),
        )
        .map_err(map_error)?;
    stream.play().map_err(map_error)?;
    Ok(stream)
}

impl CaptureBackend for LiveSource {
    fn format(&self) -> CaptureFormat {
        self.format
    }

    fn start(&mut self, sink: AudioSink) -> Result<(), CaptureError> {
        self.stop();
        let (kind, expected, sample_format) = (self.kind, self.format, self.sample_format);
        let state = self.state.clone();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), CaptureError>>();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let thread = std::thread::Builder::new()
            .name(format!("av-capture-{}", kind.tag()))
            .spawn(move || {
                let built = default_device(kind).and_then(|(device, config)| {
                    let format = CaptureFormat {
                        sample_rate: config.sample_rate(),
                        channels: config.channels(),
                    };
                    if format != expected || config.sample_format() != sample_format {
                        return Err(CaptureError::UnsupportedFormat(
                            "the default device changed between probe and start".into(),
                        ));
                    }
                    let cfg: cpal::StreamConfig = config.into();
                    match sample_format {
                        SampleFormat::F32 => {
                            build_stream::<f32>(&device, &cfg, sink, state.clone())
                        }
                        SampleFormat::I16 => {
                            build_stream::<i16>(&device, &cfg, sink, state.clone())
                        }
                        SampleFormat::I32 => {
                            build_stream::<i32>(&device, &cfg, sink, state.clone())
                        }
                        SampleFormat::F64 => {
                            build_stream::<f64>(&device, &cfg, sink, state.clone())
                        }
                        other => Err(CaptureError::UnsupportedFormat(format!("{other:?}"))),
                    }
                });
                match built {
                    Ok(stream) => {
                        state.store(RUNNING, Ordering::SeqCst);
                        let _ = ready_tx.send(Ok(()));
                        // Park until stop (or the source is dropped); the stream lives here.
                        let _ = stop_rx.recv();
                        drop(stream);
                        if state.load(Ordering::SeqCst) == RUNNING {
                            state.store(STOPPED, Ordering::SeqCst);
                        }
                    }
                    Err(e) => {
                        state.store(FAILED, Ordering::SeqCst);
                        let _ = ready_tx.send(Err(e));
                    }
                }
            })
            .map_err(|e| CaptureError::BackendUnavailable(e.to_string()))?;
        self.thread = Some(thread);
        self.stop = Some(stop_tx);
        match ready_rx.recv_timeout(OPEN_TIMEOUT + Duration::from_secs(1)) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => {
                self.stop();
                Err(e)
            }
            Err(_) => {
                self.stop();
                Err(CaptureError::Timeout)
            }
        }
    }

    fn stop(&mut self) {
        if let Some(tx) = self.stop.take() {
            let _ = tx.send(());
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }

    fn state(&self) -> CaptureState {
        match self.state.load(Ordering::SeqCst) {
            RUNNING => CaptureState::Running,
            FAILED => CaptureState::Failed,
            _ => CaptureState::Stopped,
        }
    }
}

impl Drop for LiveSource {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ring::ring;

    /// Needs a real output device; run with `cargo test -p av-audio -- --ignored` on a desktop.
    #[test]
    #[ignore = "needs an audio device"]
    fn system_output_loopback_delivers_samples() {
        let mut src = LiveSource::open(LiveKind::SystemOutput).expect("default output device");
        let fmt = src.format();
        let (sink, mut out) = ring(fmt.sample_rate, fmt.channels, 2.0);
        src.start(sink).expect("loopback starts");
        std::thread::sleep(Duration::from_millis(500));
        let mut buf = vec![0.0f32; 96_000];
        let n = out.pop(&mut buf);
        assert_eq!(src.state(), CaptureState::Running);
        // Shared-mode loopback delivers packets only while something renders; either way the
        // stream must be alive and the samples finite.
        assert!(buf[..n].iter().all(|s| s.is_finite()));
        src.stop();
        assert_eq!(src.state(), CaptureState::Stopped);
    }
}
