//! The load pipeline shared by `AppConfig` and `Scene` (pre-plan section 5.6):
//!
//! **parse -> migrate -> self-heal -> sanitize -> normalize**
//!
//! - *Migrate* runs first, on the raw JSON, so old values are moved before anything clamps them
//!   (otherwise they become dead settings).
//! - *Self-heal* starts from the defaults and takes every user value that still deserializes in
//!   its slot. Missing fields keep the default (`filled`); a value of the wrong type or an unknown
//!   enum variant keeps the default (`healed`); unknown keys are dropped (`unknown`, ADR-007:
//!   ignore and rewrite normalized). One bad field never resets its neighbours.
//! - *Sanitize* walks the numeric limit table: non-finite values return to the default, others
//!   are clamped (and rounded for integers).
//! - *Normalize* fixes cross-field invariants (`min < max`, slugs, list lengths).
//!
//! Everything here is pure: no I/O, no clocks. The store in `src-tauri` reads and writes files.

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

/// A rewrite of the raw JSON from one version to the next.
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    pub from: u32,
    pub apply: fn(&mut Map<String, Value>),
}

/// One numeric field: its JSON path, range, and typed accessors. The table is the single source
/// for sanitizing and for the settings parity fixture.
#[derive(Debug)]
pub struct Limit<T> {
    pub path: &'static str,
    pub min: f64,
    pub max: f64,
    pub integer: bool,
    pub get: fn(&T) -> f64,
    pub set: fn(&mut T, f64),
}

// Manual impls: a derive would demand `T: Copy` although only fn pointers are stored.
impl<T> Clone for Limit<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Limit<T> {}

/// Builds a [`Limit`] row: `limit!(Doc, "json.path", min, max, int|float, field.path)`.
macro_rules! limit {
    ($ty:ty, $path:literal, $min:expr, $max:expr, int, $($acc:tt)+) => {
        $crate::pipeline::Limit {
            path: $path,
            min: $min as f64,
            max: $max as f64,
            integer: true,
            get: |c: &$ty| c.$($acc)+ as f64,
            set: |c: &mut $ty, v: f64| c.$($acc)+ = v as u32,
        }
    };
    ($ty:ty, $path:literal, $min:expr, $max:expr, float, $($acc:tt)+) => {
        $crate::pipeline::Limit {
            path: $path,
            min: $min as f64,
            max: $max as f64,
            integer: false,
            get: |c: &$ty| c.$($acc)+ as f64,
            set: |c: &mut $ty, v: f64| c.$($acc)+ = v as f32,
        }
    };
}
pub(crate) use limit;

/// A persisted, versioned document.
pub trait Versioned: Default + Serialize + DeserializeOwned + 'static {
    /// JSON key holding the version (`schemaVersion` / `formatVersion`).
    const VERSION_KEY: &'static str;
    /// The version this build writes.
    const CURRENT: u32;
    fn migrations() -> &'static [Migration] {
        &[]
    }
    fn limits() -> &'static [Limit<Self>];
    /// Cross-field invariants. Push a description of every change into `notes`.
    fn normalize(&mut self, notes: &mut Vec<String>);
}

/// What the pipeline changed. Empty lists and no flags mean the input was already canonical.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// The input was not a JSON object; defaults were used.
    pub corrupt: bool,
    /// The input was written by an older version.
    pub migrated_from: Option<u32>,
    /// The input claims a newer version than this build; known fields were kept.
    pub newer_version: Option<u32>,
    pub filled: Vec<String>,
    pub healed: Vec<String>,
    pub unknown: Vec<String>,
    pub clamped: Vec<String>,
    pub normalized: Vec<String>,
}

impl Report {
    /// True when the stored text should be rewritten (anything but missing-field fills, which
    /// are also rewritten, so: anything at all).
    pub fn changed(&self) -> bool {
        *self != Self::default()
    }
    /// True when the original should be backed up before it is overwritten: data was lost,
    /// cannot be understood by this build, or was rewritten by a migration (an older build can
    /// still read the backup).
    pub fn needs_backup(&self) -> bool {
        self.corrupt
            || self.newer_version.is_some()
            || self.migrated_from.is_some()
            || !self.healed.is_empty()
    }
}

/// A loaded document and the report of what changed.
#[derive(Debug, Clone)]
pub struct Loaded<T> {
    pub value: T,
    pub report: Report,
}

/// Loads from text. Never fails: corrupt input yields defaults with `report.corrupt`.
pub fn load<T: Versioned>(text: &str) -> Loaded<T> {
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(map)) => load_object(map),
        _ => {
            let mut loaded = finish(T::default(), Report::default());
            loaded.report.corrupt = true;
            loaded
        }
    }
}

/// Applies a partial update (a JSON object with any subset of fields) to `current` and runs
/// heal/sanitize/normalize on the result. The answer is the *applied* value the Studio mirrors.
pub fn patch<T: Versioned>(current: &T, patch: &Value) -> Loaded<T> {
    let mut base = serde_json::to_value(current).expect("documents serialize");
    if let (Value::Object(base), Value::Object(p)) = (&mut base, patch) {
        merge_patch(base, p);
    }
    let Value::Object(map) = base else {
        unreachable!("documents serialize to objects")
    };
    load_object::<T>(map)
}

/// Serializes as pretty JSON with a trailing newline (the canonical stored form).
pub fn to_text<T: Versioned>(value: &T) -> String {
    let mut s = serde_json::to_string_pretty(value).expect("documents serialize");
    s.push('\n');
    s
}

fn load_object<T: Versioned>(mut map: Map<String, Value>) -> Loaded<T> {
    let mut report = Report::default();

    // Migrate.
    let version = map
        .get(T::VERSION_KEY)
        .and_then(Value::as_u64)
        .map(|v| v.min(u32::MAX as u64) as u32);
    match version {
        Some(v) if v > T::CURRENT => report.newer_version = Some(v),
        Some(v) if v < T::CURRENT => {
            report.migrated_from = Some(v);
            let mut at = v;
            for m in T::migrations().iter().filter(|m| m.from >= v) {
                if m.from == at {
                    (m.apply)(&mut map);
                    at += 1;
                }
            }
        }
        _ => {}
    }
    map.insert(T::VERSION_KEY.into(), Value::from(T::CURRENT));

    // Self-heal.
    let defaults = serde_json::to_value(T::default()).expect("defaults serialize");
    let mut merged = defaults.clone();
    heal_into::<T>(&mut merged, &defaults, &Value::Object(map), "", &mut report);
    let mut value: T = serde_json::from_value(merged).expect("healed value deserializes");

    // Sanitize, then normalize.
    sanitize(&mut value, &mut report);
    value.normalize(&mut report.normalized);
    finish(value, report)
}

fn finish<T: Versioned>(value: T, report: Report) -> Loaded<T> {
    Loaded { value, report }
}

/// Walks the defaults' shape. Objects recurse; everything else (numbers, strings, enums,
/// arrays, nulls) is a leaf that is accepted only if the whole document still deserializes.
fn heal_into<T: Versioned>(
    merged: &mut Value,
    defaults: &Value,
    user: &Value,
    prefix: &str,
    report: &mut Report,
) {
    let (Some(def_obj), Some(user_obj)) = (defaults.as_object(), user.as_object()) else {
        return;
    };
    for key in user_obj.keys() {
        if !def_obj.contains_key(key) {
            report.unknown.push(join(prefix, key));
        }
    }
    for (key, def_child) in def_obj {
        let path = join(prefix, key);
        let Some(user_child) = user_obj.get(key) else {
            report.filled.push(path);
            continue;
        };
        if def_child.is_object() {
            if user_child.is_object() {
                // Recurse with a pointer into `merged`; take it out and put it back to satisfy
                // the borrow checker without cloning the whole document per level.
                let mut slot = merged[key.as_str()].take();
                heal_into::<T>(&mut slot, def_child, user_child, &path, report);
                merged[key.as_str()] = slot;
                // A nested object that now fails to deserialize can only come from a leaf that
                // passed in isolation; the per-leaf trial below rules that out.
            } else {
                report.healed.push(path);
            }
            continue;
        }
        if user_child == def_child {
            continue;
        }
        let previous = std::mem::replace(&mut merged[key.as_str()], user_child.clone());
        if !leaf_is_valid::<T>(merged, prefix) {
            merged[key.as_str()] = previous;
            report.healed.push(path);
        }
    }
}

/// `merged` here is the object at `prefix`; validity is checked by deserializing a document
/// made of the defaults with this subtree substituted.
fn leaf_is_valid<T: Versioned>(subtree: &Value, prefix: &str) -> bool {
    let mut doc = serde_json::to_value(T::default()).expect("defaults serialize");
    let target = if prefix.is_empty() {
        &mut doc
    } else {
        match doc.pointer_mut(&format!("/{}", prefix.replace('.', "/"))) {
            Some(t) => t,
            None => return false,
        }
    };
    *target = subtree.clone();
    serde_json::from_value::<T>(doc).is_ok()
}

fn sanitize<T: Versioned>(value: &mut T, report: &mut Report) {
    let defaults = T::default();
    for limit in T::limits() {
        let v = (limit.get)(value);
        let fixed = if !v.is_finite() {
            (limit.get)(&defaults)
        } else {
            let c = v.clamp(limit.min, limit.max);
            if limit.integer { c.round() } else { c }
        };
        if fixed.to_bits() != v.to_bits() {
            (limit.set)(value, fixed);
            report.clamped.push(limit.path.to_string());
        }
    }
}

/// RFC 7396-style merge without null deletion (null is a real value for optional fields).
fn merge_patch(base: &mut Map<String, Value>, patch: &Map<String, Value>) {
    for (k, v) in patch {
        match (base.get_mut(k), v) {
            (Some(Value::Object(b)), Value::Object(p)) => merge_patch(b, p),
            _ => {
                base.insert(k.clone(), v.clone());
            }
        }
    }
}

fn join(prefix: &str, key: &str) -> String {
    if prefix.is_empty() {
        key.to_string()
    } else {
        format!("{prefix}.{key}")
    }
}

/// Lower-case `[a-z0-9-]` slug, collapsing other runs to one `-`; empty input gives `fallback`.
pub fn slugify(input: &str, fallback: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out.truncate(64);
    if out.is_empty() {
        fallback.to_string()
    } else {
        out
    }
}
