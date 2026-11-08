//! Config and scene pipeline: partial and corrupt stores, healing, clamps, normalization,
//! migrations and patches (pre-plan sections 5.6 and 7; AGENTS.md section 6).

use av_protocol::config::{
    AppConfig, BallisticsMode, DbReference, LIMITS, Scale, SourceKind, WindowShape,
};
use av_protocol::pipeline::{Limit, Migration, Versioned, load, patch, slugify, to_text};
use av_protocol::scene::{Scene, normalize_color};
use serde::{Deserialize, Serialize};
use serde_json::json;

fn cfg(text: &str) -> av_protocol::Loaded<AppConfig> {
    load::<AppConfig>(text)
}

#[test]
fn defaults_round_trip_clean() {
    let text = to_text(&AppConfig::default());
    let l = cfg(&text);
    assert_eq!(l.value, AppConfig::default());
    assert!(!l.report.changed(), "{:?}", l.report);
    assert_eq!(to_text(&l.value), text);
}

#[test]
fn every_default_is_inside_its_limit() {
    let d = AppConfig::default();
    for l in LIMITS.iter() {
        let v = (l.get)(&d);
        assert!(
            v >= l.min && v <= l.max,
            "{} default {v} outside [{}, {}]",
            l.path,
            l.min,
            l.max
        );
    }
}

#[test]
fn corrupt_text_gives_defaults_and_asks_for_a_backup() {
    for bad in ["", "{", "not json", "[1, 2]", "42", "null"] {
        let l = cfg(bad);
        assert!(l.report.corrupt, "{bad:?}");
        assert!(l.report.needs_backup());
        assert_eq!(l.value, AppConfig::default());
    }
}

#[test]
fn partial_store_keeps_what_it_has_and_fills_the_rest() {
    let l = cfg(r#"{ "schemaVersion": 3, "analysis": { "outputBins": 64 } }"#);
    assert_eq!(l.value.analysis.output_bins, 64);
    assert_eq!(l.value.analysis.db_range, 80.0);
    assert_eq!(l.value.audio, AppConfig::default().audio);
    assert!(l.report.filled.contains(&"audio".to_string()));
    assert!(l.report.filled.contains(&"analysis.dbRange".to_string()));
    assert!(!l.report.needs_backup(), "fills alone lose nothing");
}

#[test]
fn one_bad_field_never_resets_its_neighbours() {
    let l = cfg(r#"{ "analysis": { "outputBins": "lots", "dbRange": 60, "window": "hamming" } }"#);
    assert_eq!(l.value.analysis.output_bins, 1024);
    assert_eq!(l.value.analysis.db_range, 60.0);
    assert_eq!(l.value.analysis.window, WindowShape::Hamming);
    assert_eq!(l.report.healed, vec!["analysis.outputBins".to_string()]);
    assert!(l.report.needs_backup());
}

#[test]
fn unknown_enum_variants_and_wrong_shapes_heal_to_defaults() {
    let l = cfg(r#"{ "audio": { "source": "telepathy", "deviceId": "dev-1" }, "overlay": 5 }"#);
    assert_eq!(l.value.audio.source, SourceKind::SystemOutput);
    assert_eq!(l.value.audio.device_id.as_deref(), Some("dev-1"));
    assert!(l.report.healed.contains(&"audio.source".to_string()));
    assert!(l.report.healed.contains(&"overlay".to_string()));
}

#[test]
fn unknown_keys_are_dropped_and_reported() {
    let l = cfg(r#"{ "legacyThing": 1, "analysis": { "foo": 2 } }"#);
    assert_eq!(
        l.report.unknown,
        vec!["legacyThing".to_string(), "analysis.foo".to_string()]
    );
    assert!(!to_text(&l.value).contains("legacyThing"));
}

#[test]
fn out_of_range_values_are_clamped_and_integers_rounded() {
    let l = cfg(
        r#"{ "analysis": { "outputBins": 999999, "dbRange": 500, "attackMs": -3, "release": 1.5 },
             "performance": { "fpsCap": 1 } }"#,
    );
    assert_eq!(l.value.analysis.output_bins, 65_537);
    assert_eq!(l.value.analysis.db_range, 160.0);
    assert_eq!(l.value.analysis.attack_ms, 0.0);
    assert_eq!(l.value.analysis.release, 0.99);
    assert_eq!(l.value.performance.fps_cap, 15);
    for p in [
        "analysis.outputBins",
        "analysis.dbRange",
        "analysis.attackMs",
        "analysis.release",
        "performance.fpsCap",
    ] {
        assert!(l.report.clamped.contains(&p.to_string()), "{p}");
    }
}

#[test]
fn huge_floats_never_become_infinite() {
    let l = cfg(r#"{ "analysis": { "releaseMs": 1e300 } }"#);
    assert!(l.value.analysis.release_ms.is_finite());
    assert!(l.value.analysis.release_ms <= 5_000.0);
}

#[test]
fn cross_field_invariants_are_normalized() {
    let l = cfg(
        r#"{ "analysis": { "logFloorHz": 2000, "displayMaxHz": 500, "fftSize": 3000,
                           "windowSamples": 30000, "layers": 6 },
             "scene": "My Scene!", "audio": { "deviceId": "  " } }"#,
    );
    let a = &l.value.analysis;
    assert!(a.display_max_hz >= a.log_floor_hz * 2.0, "{a:?}");
    assert!(a.largest_window() <= 65_536, "{a:?}");
    assert_eq!(a.layers, 2);
    assert_eq!(
        a.fft_size, 4096,
        "raised to a power of two, never to the window"
    );
    assert_eq!(l.value.scene, "my-scene");
    assert_eq!(l.value.audio.device_id, None);
    assert!(l.report.normalized.len() >= 4, "{:?}", l.report.normalized);
}

#[test]
fn a_newer_schema_keeps_known_fields_and_asks_for_a_backup() {
    let l = cfg(r#"{ "schemaVersion": 99, "analysis": { "outputBins": 32 }, "future": true }"#);
    assert_eq!(l.report.newer_version, Some(99));
    assert_eq!(l.value.analysis.output_bins, 32);
    assert_eq!(l.value.schema_version, 3);
    assert!(l.report.needs_backup());
}

#[test]
fn patches_are_sanitized_into_the_applied_value() {
    let current = AppConfig::default();
    let applied = patch(
        &current,
        &json!({ "analysis": { "kaiserBeta": 500, "dbReference": "framePeak" } }),
    );
    assert_eq!(applied.value.analysis.kaiser_beta, 100.0);
    assert_eq!(applied.value.analysis.db_reference, DbReference::FramePeak);
    assert_eq!(applied.value.audio, current.audio);
    assert_eq!(
        applied.report.clamped,
        vec!["analysis.kaiserBeta".to_string()]
    );
    // Null is a real value for optional fields.
    let mut with_device = current.clone();
    with_device.audio.device_id = Some("x".into());
    let cleared = patch(&with_device, &json!({ "audio": { "deviceId": null } }));
    assert_eq!(cleared.value.audio.device_id, None);
}

/// A real schema v1 store (as written by the previous build) migrates to v2 with every value
/// moved before clamping.
#[test]
fn schema_v1_migrates_presets_bands_and_db_bounds() {
    let v1 = r#"{
      "schemaVersion": 1,
      "audio": { "source": "systemOutput", "deviceId": null, "avOffsetMs": 0.0 },
      "analysis": {
        "preset": "default", "layers": 4, "fftSize": 32768, "window": "hann",
        "kaiserBeta": 15.0, "barCount": 48, "layout": "mel", "warpBlend": 0.5,
        "minHz": 40.0, "maxHz": 16000.0, "dbFloor": -72.0, "dbCeiling": 0.0,
        "attackMs": 12.0, "releaseMs": 220.0, "autoGain": false,
        "eq": { "enabled": true, "highShelf": true, "highGainDb": 24.0, "highCutoffHz": 1000.0,
                "lowShelf": false, "lowGainDb": 0.0, "lowCutoffHz": 200.0, "q": 0.707,
                "amount": 1.0 }
      },
      "scene": "bars"
    }"#;
    let l = cfg(v1);
    let a = &l.value.analysis;
    assert_eq!(l.report.migrated_from, Some(1));
    assert!(
        l.report.needs_backup(),
        "a migrated store keeps its v1 original"
    );
    assert_eq!(l.value.schema_version, 3);
    assert_eq!((a.window_samples, a.hop_samples, a.layers), (2048, 512, 1));
    assert_eq!(a.scale, Scale::Mel);
    assert_eq!((a.log_floor_hz, a.display_max_hz), (40.0, 16_000.0));
    assert_eq!(a.db_range, 72.0);
    assert_eq!(a.db_reference, DbReference::Dbfs);
    assert!(a.ballistics);
    assert_eq!(a.ballistics_mode, BallisticsMode::Milliseconds);
    assert_eq!((a.attack_ms, a.release_ms), (12.0, 220.0));
    assert_eq!(a.window, WindowShape::Hann);
    assert_eq!(a.warp_blend, 0.5);
    assert!(l.report.unknown.is_empty(), "{:?}", l.report.unknown);
    assert!(l.report.healed.is_empty(), "{:?}", l.report.healed);
}

#[test]
fn schema_v2_parallel_layers_become_layer_threads() {
    for (old, threads) in [("auto", 0), ("always", 6), ("never", 1)] {
        let l = cfg(&format!(
            r#"{{ "schemaVersion": 2, "analysis": {{ "parallelLayers": "{old}", "layers": 3 }} }}"#
        ));
        assert_eq!(l.report.migrated_from, Some(2));
        assert_eq!(l.value.analysis.layer_threads, threads, "{old}");
        assert_eq!(l.value.analysis.layers, 3);
        assert!(l.report.unknown.is_empty(), "{:?}", l.report.unknown);
    }
}

#[test]
fn schema_v1_layered_preset_keeps_its_layers() {
    let l = cfg(r#"{ "schemaVersion": 1, "analysis": { "preset": "layered", "layers": 5 } }"#);
    let a = &l.value.analysis;
    assert_eq!((a.window_samples, a.hop_samples, a.layers), (1024, 512, 5));
}

// --- Migration mechanism, exercised on a test-only document with a real v1 -> v2 move. ---

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Doc {
    version: u32,
    /// v1 called this `size`; v2 renamed it and changed the unit from px to DIPs (/2).
    width_dip: f32,
}

impl Default for Doc {
    fn default() -> Self {
        Self {
            version: 2,
            width_dip: 100.0,
        }
    }
}

static DOC_LIMITS: &[Limit<Doc>] = &[Limit {
    path: "widthDip",
    min: 10.0,
    max: 1000.0,
    integer: false,
    get: |d| d.width_dip as f64,
    set: |d, v| d.width_dip = v as f32,
}];

static DOC_MIGRATIONS: &[Migration] = &[Migration {
    from: 1,
    apply: |m| {
        if let Some(px) = m.remove("size").and_then(|v| v.as_f64()) {
            m.insert("widthDip".into(), json!(px / 2.0));
        }
    },
}];

impl Versioned for Doc {
    const VERSION_KEY: &'static str = "version";
    const CURRENT: u32 = 2;
    fn migrations() -> &'static [Migration] {
        DOC_MIGRATIONS
    }
    fn limits() -> &'static [Limit<Self>] {
        DOC_LIMITS
    }
    fn normalize(&mut self, _notes: &mut Vec<String>) {}
}

#[test]
fn migrations_run_before_clamping() {
    // 4000 px would clamp to 1000 if it were read as DIPs before migrating; 4000 / 2 = 2000
    // still clamps, but 1800 px -> 900 DIPs must survive.
    let l = load::<Doc>(r#"{ "version": 1, "size": 1800 }"#);
    assert_eq!(l.report.migrated_from, Some(1));
    assert_eq!(l.value.width_dip, 900.0);
    assert!(
        l.report.unknown.is_empty(),
        "migrated key must not be reported unknown"
    );
    assert!(l.report.clamped.is_empty());
}

#[test]
fn a_missing_version_is_treated_as_current() {
    let l = load::<Doc>(r#"{ "widthDip": 50 }"#);
    assert_eq!(l.report.migrated_from, None);
    assert_eq!(l.value.width_dip, 50.0);
}

// --- Scenes. ---

#[test]
fn scenes_normalize_ids_names_and_palettes() {
    let l = load::<Scene>(
        r##"{ "formatVersion": 1, "id": "Neon Bars!!", "name": "  ",
              "palette": ["#ABC", "nope", "#11223344", "#zzzzzz"], "opacity": 7 }"##,
    );
    assert_eq!(l.value.id, "neon-bars");
    assert_eq!(l.value.name, "neon-bars");
    assert_eq!(
        l.value.palette,
        vec!["#aabbcc".to_string(), "#11223344".to_string()]
    );
    assert_eq!(l.value.opacity, 1.0);
}

#[test]
fn a_scene_without_valid_colours_gets_the_default_palette() {
    let l = load::<Scene>(r#"{ "palette": ["red"] }"#);
    assert_eq!(l.value.palette, Scene::default().palette);
}

#[test]
fn scenes_carry_no_machine_specific_settings() {
    let text = to_text(&Scene::default()).to_lowercase();
    for forbidden in [
        "device", "monitor", "path", "hotkey", "gpu", "offset", "anchor",
    ] {
        assert!(!text.contains(forbidden), "scene JSON mentions {forbidden}");
    }
}

#[test]
fn colours_and_slugs() {
    assert_eq!(normalize_color("#FFF").as_deref(), Some("#ffffff"));
    assert_eq!(normalize_color("#12345").as_deref(), None);
    assert_eq!(slugify("  Hello,  World ", "x"), "hello-world");
    assert_eq!(slugify("!!!", "fallback"), "fallback");
}

#[test]
fn scene_v1_ring_booleans_migrate_to_components() {
    use av_protocol::scene::Ring;
    let ring = |bars: bool, inner: bool| -> Ring {
        let text = format!(
            r#"{{ "formatVersion": 1, "id": "x", "name": "X", "style": "concentric",
                 "ring": {{ "bins": 120, "floor": 0.3, "gain": 1.5, "radius": 0.4,
                            "bars": {bars}, "inner": {inner} }} }}"#
        );
        let l = load::<Scene>(&text);
        assert_eq!(l.report.migrated_from, Some(1));
        assert!(l.report.unknown.is_empty(), "{:?}", l.report.unknown);
        assert_eq!(l.value.ring.bins, 120);
        l.value.ring
    };
    let both = ring(true, true);
    assert!(both.span_bars && !both.outer_bars && !both.outer_line && both.bar_scale == 1.0);
    let out = ring(true, false);
    assert!(out.outer_bars && !out.span_bars);
    let loops = ring(false, true);
    assert!(loops.outer_line && loops.inner_line && !loops.span_bars);
    let outer_only = ring(false, false);
    assert!(outer_only.outer_line && !outer_only.inner_line);
}

#[test]
fn component_colours_are_normalized_or_dropped() {
    let l = load::<Scene>(
        r##"{ "formatVersion": 2, "colors": { "outerLine": "#ABC", "scope": "#ff00ff80",
             "spanBars": "red", "bars": null } }"##,
    );
    let c = &l.value.colors;
    assert_eq!(c.outer_line.as_deref(), Some("#aabbcc"));
    assert_eq!(c.scope.as_deref(), Some("#ff00ff80"));
    assert_eq!(c.span_bars, None);
    assert_eq!(c.bars, None);
    assert!(l.report.normalized.iter().any(|n| n.contains("spanBars")));
}
