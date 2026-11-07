//! File source: decode a file with symphonia, then play it into the ring in real time on an
//! `av-capture-file` thread, in 10 ms chunks like a shared-mode WASAPI period. It is the
//! deterministic source for tests, fixtures and the Studio preview before live capture exists
//! (Phase 1B.1).

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::capture::{CaptureBackend, CaptureError, CaptureFormat, CaptureState};
use crate::ring::AudioSink;

/// Fully decoded interleaved audio.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedAudio {
    pub format: CaptureFormat,
    pub samples: Vec<f32>,
}

impl DecodedAudio {
    pub fn frames(&self) -> usize {
        self.samples.len() / self.format.channels.max(1) as usize
    }
}

/// Decodes the first audio track of `path` (WAV, FLAC, MP3, AAC/M4A, Ogg Vorbis) to f32.
pub fn decode_file(path: &Path) -> Result<DecodedAudio, CaptureError> {
    use symphonia::core::codecs::audio::AudioDecoderOptions;
    use symphonia::core::errors::Error as SymError;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::formats::probe::Hint;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;

    let file = std::fs::File::open(path).map_err(|e| CaptureError::Decode(e.to_string()))?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| CaptureError::Decode(format!("unrecognised container: {e}")))?;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.as_ref().and_then(|p| p.audio()).is_some())
        .ok_or_else(|| CaptureError::Decode("no audio track".into()))?;
    let track_id = track.id;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or_else(|| CaptureError::Decode("no audio track".into()))?;
    let sample_rate = params
        .sample_rate
        .ok_or_else(|| CaptureError::Decode("no sample rate".into()))?;
    if !(8_000..=384_000).contains(&sample_rate) {
        return Err(CaptureError::UnsupportedFormat(format!("{sample_rate} Hz")));
    }
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default())
        .map_err(|e| CaptureError::UnsupportedFormat(e.to_string()))?;

    let mut samples = Vec::new();
    let mut packet_buf = Vec::new();
    let mut channels = 0u16;
    loop {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) | Err(SymError::ResetRequired) => break,
            Err(e) => return Err(CaptureError::Decode(e.to_string())),
        };
        if packet.track_id != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            // A damaged packet is skipped, not fatal.
            Err(SymError::DecodeError(_)) => continue,
            Err(e) => return Err(CaptureError::Decode(e.to_string())),
        };
        let ch = decoded.spec().channels().count().max(1) as u16;
        if channels == 0 {
            channels = ch;
        } else if ch != channels {
            return Err(CaptureError::UnsupportedFormat(
                "channel count changes mid-file".into(),
            ));
        }
        packet_buf.resize(decoded.samples_interleaved(), 0.0f32);
        decoded.copy_to_slice_interleaved(&mut packet_buf);
        samples.extend_from_slice(&packet_buf);
    }
    if channels == 0 {
        return Err(CaptureError::Decode("the file contains no audio".into()));
    }
    Ok(DecodedAudio {
        format: CaptureFormat {
            sample_rate,
            channels,
        },
        samples,
    })
}

/// Plays decoded audio into the ring at real-time pace.
#[derive(Debug)]
pub struct FileSource {
    audio: Arc<DecodedAudio>,
    looping: bool,
    stop: Arc<AtomicBool>,
    state: Arc<AtomicU8>,
    thread: Option<JoinHandle<()>>,
}

/// Chunk length, like a shared-mode WASAPI period.
pub const PERIOD: Duration = Duration::from_millis(10);

const fn encode(s: CaptureState) -> u8 {
    match s {
        CaptureState::Stopped => 0,
        CaptureState::Running => 1,
        CaptureState::Finished => 2,
        CaptureState::Failed => 3,
    }
}

const fn decode(v: u8) -> CaptureState {
    match v {
        1 => CaptureState::Running,
        2 => CaptureState::Finished,
        3 => CaptureState::Failed,
        _ => CaptureState::Stopped,
    }
}

impl FileSource {
    pub fn new(audio: DecodedAudio, looping: bool) -> Self {
        Self {
            audio: Arc::new(audio),
            looping,
            stop: Arc::new(AtomicBool::new(false)),
            state: Arc::new(AtomicU8::new(encode(CaptureState::Stopped))),
            thread: None,
        }
    }

    pub fn open(path: &Path, looping: bool) -> Result<Self, CaptureError> {
        Ok(Self::new(decode_file(path)?, looping))
    }
}

impl CaptureBackend for FileSource {
    fn format(&self) -> CaptureFormat {
        self.audio.format
    }

    fn start(&mut self, mut sink: AudioSink) -> Result<(), CaptureError> {
        self.stop();
        self.stop.store(false, Ordering::SeqCst);
        let audio = self.audio.clone();
        let stop = self.stop.clone();
        let state = self.state.clone();
        let looping = self.looping;
        let ch = audio.format.channels as usize;
        let chunk = (audio.format.sample_rate as usize * PERIOD.as_millis() as usize / 1000) * ch;
        state.store(encode(CaptureState::Running), Ordering::SeqCst);
        let thread = std::thread::Builder::new()
            .name("av-capture-file".into())
            .spawn(move || {
                let started = Instant::now();
                let mut period = 0u32;
                let mut at = 0usize;
                while !stop.load(Ordering::Relaxed) && !sink.is_abandoned() {
                    if at >= audio.samples.len() {
                        if !looping {
                            state.store(encode(CaptureState::Finished), Ordering::SeqCst);
                            return;
                        }
                        at = 0;
                    }
                    let end = (at + chunk).min(audio.samples.len());
                    sink.push(&audio.samples[at..end]);
                    at = end;
                    // Pace against the start time so sleep jitter does not accumulate.
                    period += 1;
                    let due = started + PERIOD * period;
                    if let Some(wait) = due.checked_duration_since(Instant::now()) {
                        std::thread::sleep(wait);
                    }
                }
                state.store(encode(CaptureState::Stopped), Ordering::SeqCst);
            })
            .map_err(|e| CaptureError::BackendUnavailable(e.to_string()))?;
        self.thread = Some(thread);
        Ok(())
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }

    fn state(&self) -> CaptureState {
        decode(self.state.load(Ordering::SeqCst))
    }
}

impl Drop for FileSource {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Writes a 16-bit PCM WAV (test fixtures; no dependency on an encoder crate).
pub fn write_wav_i16(path: &Path, format: CaptureFormat, samples: &[f32]) -> std::io::Result<()> {
    let ch = format.channels as u32;
    let data_len = samples.len() as u32 * 2;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(ch as u16).to_le_bytes());
    out.extend_from_slice(&format.sample_rate.to_le_bytes());
    out.extend_from_slice(&(format.sample_rate * ch * 2).to_le_bytes());
    out.extend_from_slice(&((ch * 2) as u16).to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for &s in samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes());
    }
    std::fs::write(path, out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ring::ring;

    fn tone(rate: u32, ch: u16, seconds: f32) -> Vec<f32> {
        let frames = (rate as f32 * seconds) as usize;
        (0..frames)
            .flat_map(|n| {
                let v = 0.5 * (std::f32::consts::TAU * 440.0 * n as f32 / rate as f32).sin();
                std::iter::repeat_n(v, ch as usize)
            })
            .collect()
    }

    fn temp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("av-audio-{}-{name}", std::process::id()))
    }

    #[test]
    fn a_wav_round_trips_through_symphonia() {
        let fmt = CaptureFormat {
            sample_rate: 44_100,
            channels: 2,
        };
        let src = tone(44_100, 2, 0.25);
        let path = temp("rt.wav");
        write_wav_i16(&path, fmt, &src).unwrap();
        let d = decode_file(&path).unwrap();
        assert_eq!(d.format, fmt);
        assert_eq!(d.samples.len(), src.len());
        let worst = d
            .samples
            .iter()
            .zip(&src)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max);
        assert!(worst < 1.0 / 16_000.0, "16-bit round trip error {worst}");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn missing_and_garbage_files_are_typed_errors() {
        assert!(matches!(
            decode_file(Path::new("does/not/exist.wav")),
            Err(CaptureError::Decode(_))
        ));
        let path = temp("junk.wav");
        std::fs::write(&path, b"definitely not audio").unwrap();
        assert!(decode_file(&path).is_err());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn playback_is_paced_and_finishes() {
        let fmt = CaptureFormat {
            sample_rate: 48_000,
            channels: 1,
        };
        let audio = DecodedAudio {
            format: fmt,
            samples: tone(48_000, 1, 0.1),
        };
        let (sink, mut src) = ring(48_000, 1, 2.0);
        let mut fs = FileSource::new(audio, false);
        let t = Instant::now();
        fs.start(sink).unwrap();
        while fs.state() == CaptureState::Running && t.elapsed() < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(5));
        }
        let elapsed = t.elapsed();
        assert_eq!(fs.state(), CaptureState::Finished);
        assert!(
            elapsed >= Duration::from_millis(80),
            "played 100 ms in {elapsed:?}"
        );
        let mut out = vec![0.0; 10_000];
        assert_eq!(src.pop(&mut out), 4_800);
    }
}
