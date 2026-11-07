//! Cross-language wire fixtures (Track A step A4, closes P0.5).
//!
//! Rust is the reference. `cargo run -p av-protocol --example dump_fixtures` writes each case as
//! `<name>.bin` plus `manifest.json` into `test/fixtures/wire/`; `test/wire.parity.test.ts`
//! decodes every file and re-encodes every valid case byte for byte. The integration test
//! `tests/fixtures_current.rs` fails when the committed files drift from this generator.
//!
//! The manifest is written by hand (no serde in this crate yet) and only contains values JSON
//! represents exactly: integers below 2^53 and f32 values printed with Rust's shortest
//! round-trip formatting, which `Math.fround` restores bit for bit.

use std::fmt::Write as _;

use crate::frame::{FEATURE_COUNT, FrameFlags, FrameHeader, HEADER_BYTES};
use crate::wire::{FrameParts, encode, encode_header_only};

/// One fixture: the bytes and what a decoder must report.
#[derive(Debug, Clone)]
pub struct Case {
    pub name: &'static str,
    pub bytes: Vec<u8>,
    pub expect: Expect,
}

#[derive(Debug, Clone)]
pub enum Expect {
    Frame {
        header: FrameHeader,
        bars: Vec<f32>,
        bins: Option<Vec<f32>>,
        features: Option<Vec<f32>>,
    },
    /// The `WireError` variant name a decoder must report.
    Error(&'static str),
}

fn header(sequence: u32, timestamp_us: u64, fft_size: u32, bar_count: u32) -> FrameHeader {
    FrameHeader {
        sequence,
        timestamp_us,
        sample_rate: 48_000,
        fft_size,
        bar_count,
        flags: FrameFlags::empty(),
        generation: 1,
    }
}

fn frame(
    name: &'static str,
    h: FrameHeader,
    bars: Vec<f32>,
    bins: Option<Vec<f32>>,
    features: Option<Vec<f32>>,
) -> Case {
    let parts = FrameParts {
        header: h,
        bars: &bars,
        bins: bins.as_deref(),
        features: features.as_deref(),
    };
    let mut bytes = vec![
        0u8;
        HEADER_BYTES
            + (bars.len()
                + bins.as_ref().map_or(0, Vec::len)
                + features.as_ref().map_or(0, Vec::len))
                * 4
    ];
    let n = encode(&parts, &mut bytes).expect("fixture encodes");
    bytes.truncate(n);
    let mut header = h;
    header.flags.set(FrameFlags::BINS_PRESENT, bins.is_some());
    header
        .flags
        .set(FrameFlags::FEATURES_PRESENT, features.is_some());
    Case {
        name,
        bytes,
        expect: Expect::Frame {
            header,
            bars,
            bins,
            features,
        },
    }
}

/// Every fixture, in a stable order.
pub fn cases() -> Vec<Case> {
    let ramp = |n: usize, scale: f32| (0..n).map(|i| i as f32 * scale).collect::<Vec<_>>();
    let mut out = vec![
        frame(
            "bars_basic",
            header(1, 1_000_000, 2048, 8),
            vec![0.0, 0.1, 0.25, 0.5, 0.75, 1.0, f32::MIN_POSITIVE, f32::MAX],
            None,
            None,
        ),
        frame(
            "bins_present",
            header(2, 2_000_000, 16, 4),
            ramp(4, 0.25),
            Some(ramp(9, -0.125)),
            None,
        ),
        frame(
            "features_present",
            header(3, 3_000_000, 2048, 2),
            vec![0.5, -0.0],
            None,
            Some(ramp(FEATURE_COUNT, 1.5)),
        ),
        frame(
            "sequence_max",
            header(u32::MAX, 4_000_000, 2048, 1),
            vec![0.5],
            None,
            None,
        ),
        frame(
            "sequence_wrapped",
            header(0, 4_010_667, 2048, 1),
            vec![0.5],
            None,
            None,
        ),
        frame(
            "timestamp_64bit",
            header(5, 0x0001_2345_6789_ABCD, 2048, 1),
            vec![1.0],
            None,
            None,
        ),
        {
            let mut h = header(6, 6_000_000, 2048, 3);
            h.flags = FrameFlags::SILENT | FrameFlags::CLIPPED | FrameFlags::STEREO;
            h.generation = 0xDEAD_BEEF;
            frame("flags_and_generation", h, vec![0.0; 3], None, None)
        },
        frame(
            "zero_bars",
            header(7, 7_000_000, 2048, 0),
            vec![],
            None,
            None,
        ),
    ];

    let ho = {
        let h = header(42, 42_000_000, 2048, 48);
        let mut bytes = vec![0u8; HEADER_BYTES];
        encode_header_only(&h, &mut bytes).expect("header-only encodes");
        Case {
            name: "header_only",
            bytes,
            expect: Expect::Frame {
                header: FrameHeader {
                    flags: FrameFlags::HEADER_ONLY,
                    ..h
                },
                bars: vec![],
                bins: None,
                features: None,
            },
        }
    };
    out.push(ho);

    let basic = out[0].bytes.clone();
    let mut unknown_version = basic.clone();
    unknown_version[27] = 2;
    out.push(Case {
        name: "reject_unknown_version",
        bytes: unknown_version,
        expect: Expect::Error("UnknownVersion"),
    });
    let mut reserved = basic.clone();
    reserved[24] |= 1 << 6;
    out.push(Case {
        name: "reject_reserved_bits",
        bytes: reserved,
        expect: Expect::Error("ReservedBitsSet"),
    });
    out.push(Case {
        name: "reject_truncated",
        bytes: basic[..basic.len() - 2].to_vec(),
        expect: Expect::Error("Truncated"),
    });
    let mut trailing = basic;
    trailing.extend_from_slice(&[0, 0, 0, 0]);
    out.push(Case {
        name: "reject_trailing_bytes",
        bytes: trailing,
        expect: Expect::Error("TrailingBytes"),
    });
    out
}

fn floats(out: &mut String, values: &[f32]) {
    out.push('[');
    for (i, v) in values.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        let _ = write!(out, "{v:?}");
    }
    out.push(']');
}

/// The manifest describing every case, as pretty JSON.
pub fn manifest(cases: &[Case]) -> String {
    let mut s = String::from("{\n  \"wireVersion\": 1,\n  \"cases\": [\n");
    for (i, case) in cases.iter().enumerate() {
        let _ = write!(
            s,
            "    {{ \"name\": \"{}\", \"file\": \"{}.bin\", ",
            case.name, case.name
        );
        match &case.expect {
            Expect::Error(kind) => {
                let _ = write!(s, "\"error\": \"{kind}\" }}");
            }
            Expect::Frame {
                header: h,
                bars,
                bins,
                features,
            } => {
                let _ = write!(
                    s,
                    "\"header\": {{ \"sequence\": {}, \"timestampLo\": {}, \"timestampHi\": {}, \"sampleRate\": {}, \"fftSize\": {}, \"barCount\": {}, \"flags\": {}, \"generation\": {} }},\n      \"bars\": ",
                    h.sequence,
                    h.timestamp_us as u32,
                    (h.timestamp_us >> 32) as u32,
                    h.sample_rate,
                    h.fft_size,
                    h.bar_count,
                    h.flags.bits(),
                    h.generation
                );
                floats(&mut s, bars);
                for (key, v) in [("bins", bins), ("features", features)] {
                    let _ = write!(s, ", \"{key}\": ");
                    match v {
                        Some(v) => floats(&mut s, v),
                        None => s.push_str("null"),
                    }
                }
                s.push_str(" }");
            }
        }
        s.push_str(if i + 1 < cases.len() { ",\n" } else { "\n" });
    }
    s.push_str("  ]\n}\n");
    s
}

/// Settings parity fixtures: defaults and the numeric limit tables for `AppConfig` and `Scene`,
/// written to `test/fixtures/settings/`. `test/settings.parity.test.ts` checks that every limit
/// names a real field of the defaults and that every default is inside its range; the Studio's
/// settings store reads the same shapes (blueprint section 7).
pub fn settings_files() -> Vec<(&'static str, String)> {
    use crate::pipeline::{Limit, Versioned, to_text};

    fn limits<T: Versioned>(rows: &[Limit<T>]) -> String {
        let d = T::default();
        let rows: Vec<serde_json::Value> = rows
            .iter()
            .map(|l| {
                serde_json::json!({
                    "path": l.path,
                    "min": l.min,
                    "max": l.max,
                    "integer": l.integer,
                    "default": (l.get)(&d),
                })
            })
            .collect();
        let mut s = serde_json::to_string_pretty(&rows).expect("limits serialize");
        s.push('\n');
        s
    }

    vec![
        (
            "config.defaults.json",
            to_text(&crate::config::AppConfig::default()),
        ),
        ("config.limits.json", limits(&crate::config::LIMITS)),
        (
            "scene.defaults.json",
            to_text(&crate::scene::Scene::default()),
        ),
        ("scene.limits.json", limits(crate::scene::LIMITS)),
        (
            "preset.defaults.json",
            to_text(&crate::preset::Preset::default()),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::decode;

    /// Every case decodes in Rust exactly as its manifest entry says.
    #[test]
    fn rust_decodes_every_case_as_described() {
        for case in cases() {
            match (&case.expect, decode(&case.bytes)) {
                (Expect::Error(kind), Err(e)) => {
                    assert!(format!("{e:?}").starts_with(kind), "{}: {e:?}", case.name)
                }
                (
                    Expect::Frame {
                        header,
                        bars,
                        bins,
                        features,
                    },
                    Ok(d),
                ) => {
                    assert_eq!(&d.header, header, "{}", case.name);
                    assert_eq!(
                        d.bars().iter().map(f32::to_bits).collect::<Vec<_>>(),
                        bars.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
                    );
                    assert_eq!(d.bins().map(|b| b.iter().collect::<Vec<_>>()), *bins);
                    assert_eq!(
                        d.features().map(|b| b.iter().collect::<Vec<_>>()),
                        *features
                    );
                }
                (_, r) => panic!("{}: unexpected {r:?}", case.name),
            }
        }
    }

    #[test]
    fn names_are_unique() {
        let c = cases();
        let mut names: Vec<_> = c.iter().map(|c| c.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), c.len());
    }
}
