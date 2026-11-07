//! A7 gates: file -> analyzer -> slot ring golden frames, generation swaps, gaps, starvation
//! silence, the real-time engine end to end, and the preview encoder.

use std::time::{Duration, Instant};

use av_audio::file::{DecodedAudio, write_wav_i16};
use av_audio::{CaptureBackend, CaptureFormat, FileSource, decode_file, ring};
use av_dsp::{Analyzer, PlanCache, SpectrumConfig};
use av_engine::pipeline::Pipeline;
use av_engine::preview::PreviewEncoder;
use av_engine::{Engine, FrameCopy, slot_ring};
use av_protocol::frame::FrameFlags;
use av_protocol::wire;

const SR: u32 = 48_000;

fn tone(freq: f32, ch: u16, seconds: f32, amp: f32) -> Vec<f32> {
    let frames = (SR as f32 * seconds) as usize;
    (0..frames)
        .flat_map(|n| {
            let v = amp * (std::f32::consts::TAU * freq * n as f32 / SR as f32).sin();
            std::iter::repeat_n(v, ch as usize)
        })
        .collect()
}

/// The default analysis with `n` output bins and hop `hop`.
fn bins(n: u32, hop: u32) -> SpectrumConfig {
    let mut s = SpectrumConfig::default().settings;
    s.output_bins = n;
    s.hop_samples = hop;
    SpectrumConfig::new(s, SR)
}

fn analyzer(cfg: SpectrumConfig) -> Analyzer {
    Analyzer::new(cfg.clone(), PlanCache::default().get(cfg.fft_size)).unwrap()
}

/// The pipeline's frames equal a bare analyzer fed the same mono hops, bit for bit, with
/// sample-count timestamps and consecutive sequences.
#[test]
fn file_frames_match_the_analyzer_bit_for_bit() {
    let fmt = CaptureFormat {
        sample_rate: SR,
        channels: 2,
    };
    let path = std::env::temp_dir().join(format!("av-engine-golden-{}.wav", std::process::id()));
    write_wav_i16(&path, fmt, &tone(1000.0, 2, 0.5, 0.5)).unwrap();
    let audio = decode_file(&path).unwrap();
    let _ = std::fs::remove_file(&path);

    let cfg = SpectrumConfig::default();
    let hop = cfg.hop_samples;
    let (mut writer, reader) = slot_ring();
    let mut p = Pipeline::new(analyzer(cfg.clone()), 7, 2, 1_000_000);
    let published = p.feed(&audio.samples, &mut writer);
    assert_eq!(published, audio.frames() / hop);

    let mut reference = analyzer(cfg.clone());
    let (stereo, _) = audio.samples.as_chunks::<2>();
    let mono: Vec<f32> = stereo.iter().map(|f| (f[0] + f[1]) * 0.5).collect();
    let mut expected = Vec::new();
    for h in mono.chunks_exact(hop) {
        expected = reference.process(h, hop as f32 / SR as f32).bars.to_vec();
    }

    let mut f = FrameCopy::default();
    reader.read_latest(&mut f, false).unwrap();
    assert_eq!(f.header.sequence, published as u32);
    assert_eq!(f.header.generation, 7);
    assert_eq!(f.header.sample_rate, SR);
    assert_eq!(f.header.fft_size as usize, cfg.fft_size);
    let newest_sample = (published * hop) as u64;
    assert_eq!(
        f.header.timestamp_us,
        1_000_000 + newest_sample * 1_000_000 / SR as u64
    );
    let bits = |v: &[f32]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
    assert_eq!(bits(f.bars()), bits(&expected));
    assert!(!f.header.flags.contains(FrameFlags::SILENT));
}

#[test]
fn a_swap_changes_generation_and_shape_without_touching_the_stream() {
    let (mut writer, reader) = slot_ring();
    let mut p = Pipeline::new(analyzer(SpectrumConfig::default()), 1, 1, 0);
    p.feed(&tone(440.0, 1, 0.1, 0.5), &mut writer);
    let before = reader.latest_sequence().unwrap();
    let narrower = bins(16, 256);
    p.swap(analyzer(narrower), 2);
    p.feed(&tone(440.0, 1, 0.1, 0.5), &mut writer);
    let mut f = FrameCopy::default();
    reader.read_latest(&mut f, false).unwrap();
    assert!(
        f.header.sequence > before,
        "sequence keeps counting across the swap"
    );
    assert_eq!(f.header.generation, 2);
    assert_eq!(f.bars().len(), 16);
}

#[test]
fn gaps_advance_the_clock_and_starvation_publishes_silent_frames() {
    let (mut writer, reader) = slot_ring();
    let mut p = Pipeline::new(analyzer(SpectrumConfig::default()), 1, 1, 0);
    p.feed(&tone(440.0, 1, 0.1, 0.5), &mut writer);
    let mut f = FrameCopy::default();
    reader.read_latest(&mut f, false).unwrap();
    let t0 = f.header.timestamp_us;
    p.on_gap(4800); // 100 ms lost upstream
    for _ in 0..200 {
        p.feed_silence(&mut writer);
    }
    reader.read_latest(&mut f, false).unwrap();
    assert!(f.header.flags.contains(FrameFlags::SILENT));
    assert!(
        f.bars().iter().all(|&b| b < 1e-3),
        "silence decays the bars"
    );
    assert!(f.header.timestamp_us >= t0 + 100_000 + 200 * 512 * 1_000_000 / SR as u64);
}

#[test]
fn the_engine_runs_a_file_source_in_real_time() {
    let fmt = CaptureFormat {
        sample_rate: SR,
        channels: 2,
    };
    let audio = DecodedAudio {
        format: fmt,
        samples: tone(2000.0, 2, 1.0, 0.4),
    };
    let (sink, source) = ring(SR, 2, 2.0);
    let mut file = FileSource::new(audio, true);
    let engine = Engine::start("test", source, fmt, SpectrumConfig::default()).unwrap();
    file.start(sink).unwrap();

    let reader = engine.reader();
    let deadline = Instant::now() + Duration::from_secs(3);
    while reader.latest_sequence().unwrap_or(0) < 20 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut f = FrameCopy::default();
    reader.read_latest(&mut f, false).unwrap();
    assert!(
        f.header.sequence >= 20,
        "only {} frames in 3 s",
        f.header.sequence
    );
    assert!(
        f.bars().iter().any(|&b| b > 0.3),
        "the tone should light a bar"
    );

    // Reconfiguring bumps the generation without restarting anything.
    let generation = engine.reconfigure(bins(32, 512)).unwrap();
    assert_eq!(generation, 2);
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        reader.read_latest(&mut f, false).unwrap();
        if f.header.generation == 2 || Instant::now() > deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(f.header.generation, 2);
    assert_eq!(f.bars().len(), 32);

    std::thread::sleep(Duration::from_millis(300));
    let status = engine.status();
    assert!(status.running && status.frames > 0, "{status:?}");
    assert!(
        status.dsp_us_avg > 0.0 && status.dsp_us_avg < 2000.0,
        "{status:?}"
    );
    file.stop();
}

#[test]
fn a_starved_engine_publishes_silent_frames() {
    let fmt = CaptureFormat {
        sample_rate: SR,
        channels: 1,
    };
    let (_sink, source) = ring(SR, 1, 2.0);
    let engine = Engine::start("starved", source, fmt, SpectrumConfig::default()).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    let reader = engine.reader();
    let mut f = FrameCopy::default();
    reader.read_latest(&mut f, false).unwrap();
    assert!(f.header.flags.contains(FrameFlags::SILENT));
    assert!(f.header.sequence >= 5, "silence published at the hop rate");
}

#[test]
fn the_preview_encoder_answers_header_only_when_nothing_changed() {
    let (mut writer, reader) = slot_ring();
    let mut enc = PreviewEncoder::default();
    let empty = wire::decode(enc.encode(&reader, None, false)).unwrap();
    assert!(empty.is_header_only());

    let mut p = Pipeline::new(analyzer(SpectrumConfig::single(2048, 2048, 512)), 1, 1, 0);
    p.feed(&tone(440.0, 1, 0.05, 0.5), &mut writer);
    let seq = reader.latest_sequence().unwrap();

    let full = enc.encode(&reader, None, true).to_vec();
    let d = wire::decode(&full).unwrap();
    assert_eq!(d.header.sequence, seq);
    assert_eq!(d.bars().len(), 1024);
    assert!(
        d.bins().is_none(),
        "the analyzer publishes output bins only"
    );

    let same = wire::decode(enc.encode(&reader, Some(seq), true)).unwrap();
    assert!(same.is_header_only());
    assert_eq!(same.header.sequence, seq);

    let no_bins = wire::decode(enc.encode(&reader, None, false)).unwrap();
    assert!(no_bins.bins().is_none());
}

#[test]
fn frames_carry_the_tapered_raw_waveform() {
    let (mut writer, reader) = slot_ring();
    let mut p = Pipeline::new(analyzer(SpectrumConfig::default()), 1, 2, 0);
    // A full-scale stereo DC pair: the scope is the raw mono mix, before the DC blocker.
    let stereo = vec![0.5f32; 2 * 4800];
    p.feed(&stereo, &mut writer);
    let mut enc = PreviewEncoder::default();
    let d = wire::decode(enc.encode(&reader, None, false)).unwrap();
    let w: Vec<f32> = d.waveform().expect("waveform present").iter().collect();
    assert_eq!(w.len(), 800);
    assert_eq!(w[0], 0.0, "the taper starts at zero");
    assert!(
        (w[400] - 0.5).abs() < 1e-3,
        "the middle is the raw sample: {}",
        w[400]
    );
    assert!(w[799].abs() < 1e-6);
    // Off when the setting is 0.
    let mut s = SpectrumConfig::default().settings;
    s.waveform_samples = 0;
    p.swap(analyzer(SpectrumConfig::new(s, SR)), 2);
    p.feed(&stereo, &mut writer);
    let d = wire::decode(enc.encode(&reader, None, false)).unwrap();
    assert!(d.waveform().is_none());
}
