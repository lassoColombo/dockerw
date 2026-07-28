//! Typed field → Nushell `Value` helpers.
//!
//! Docker quantities are mapped to precise Nushell types, mirroring nuke:
//! - Unix epoch / RFC 3339 timestamp → `Value::date`
//! - byte counts                     → `Value::filesize`
//! - absent (`None`) / empty         → `Value::nothing`

use chrono::{TimeZone, Utc};
use nu_protocol::{IntoValue, Record, Span, Value};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Dates
// ---------------------------------------------------------------------------

/// Unix epoch seconds → `Value::date` (`Value::nothing` if out of range).
pub fn epoch_date(secs: i64, span: Span) -> Value {
    match Utc.timestamp_opt(secs, 0) {
        chrono::LocalResult::Single(dt) => Value::date(dt.fixed_offset(), span),
        _ => Value::nothing(span),
    }
}

/// `Option<i64>` epoch seconds → `Value::date` or `Value::nothing`.
pub fn opt_epoch(secs: Option<i64>, span: Span) -> Value {
    match secs {
        Some(s) => epoch_date(s, span),
        None => Value::nothing(span),
    }
}

/// RFC 3339 timestamp string → `Value::date` (`Value::nothing` on failure).
pub fn rfc3339_date(s: &str, span: Span) -> Value {
    match chrono::DateTime::parse_from_rfc3339(s) {
        Ok(dt) => Value::date(dt, span),
        Err(_) => Value::nothing(span),
    }
}

/// `Option<&str>` RFC 3339 → `Value::date`. Docker's "zero" timestamp
/// (`0001-01-01T00:00:00Z`, used for never-started/finished) maps to nothing.
pub fn opt_rfc3339(s: Option<&str>, span: Span) -> Value {
    match s {
        Some(s) if !s.is_empty() && !s.starts_with("0001-01-01") => rfc3339_date(s, span),
        _ => Value::nothing(span),
    }
}

// ---------------------------------------------------------------------------
// Strings / enums / maps
// ---------------------------------------------------------------------------

/// `Option<&str>` → `Value::string` (nothing when absent or empty).
///
/// Docker uses the empty string for absent scalar fields, so `Some("")` maps to
/// `nothing`. Callers holding an owned `Option<String>` pass `.as_deref()`.
pub fn str_opt(s: Option<&str>, span: Span) -> Value {
    match s {
        Some(s) if !s.is_empty() => Value::string(s, span),
        _ => Value::nothing(span),
    }
}

/// `Option<i64>` → `Value::int` or `Value::nothing`.
pub fn opt_int(n: Option<i64>, span: Span) -> Value {
    match n {
        Some(n) => Value::int(n, span),
        None => Value::nothing(span),
    }
}

/// `Option<bool>` → `Value::bool` or `Value::nothing`.
pub fn opt_bool(b: Option<bool>, span: Span) -> Value {
    match b {
        Some(b) => Value::bool(b, span),
        None => Value::nothing(span),
    }
}

/// `Option<i64>` byte count → `Value::filesize` or `Value::nothing`.
pub fn opt_filesize(bytes: Option<i64>, span: Span) -> Value {
    match bytes {
        Some(n) => Value::filesize(n, span),
        None => Value::nothing(span),
    }
}

/// A serde-serializable enum → its string form, e.g. bollard's
/// `ContainerStateStatusEnum` → `"running"`. Nothing when absent or when the
/// value does not serialize to a plain JSON string. Avoids depending on a
/// `Display`/`as_ref` impl we haven't verified.
pub fn enum_opt<T: serde::Serialize>(v: Option<&T>, span: Span) -> Value {
    match v.map(serde_json::to_value) {
        // An enum that serializes to "" (e.g. `VolumeScopeEnum::EMPTY`) means
        // "unset" — treat it as absent, like `str_opt` does for empty strings.
        Some(Ok(serde_json::Value::String(s))) if !s.is_empty() => Value::string(s, span),
        _ => Value::nothing(span),
    }
}

/// A string→string map → `Value::record` (empty record when absent). Backs
/// labels, network `options`, and any other Docker string map.
pub fn str_map(map: Option<&HashMap<String, String>>, span: Span) -> Value {
    let mut rec = Record::new();
    if let Some(map) = map {
        for (k, v) in map {
            rec.push(k.clone(), Value::string(v.clone(), span));
        }
    }
    Value::record(rec, span)
}

/// `Option<Vec<String>>` → `Value::list` of strings, always a list (empty when
/// absent) so a list column is never `nothing` — keeps list columns predictable.
pub fn str_list(items: Option<&Vec<String>>, span: Span) -> Value {
    let rows = items
        .map(|v| v.iter().map(|s| Value::string(s, span)).collect())
        .unwrap_or_default();
    Value::list(rows, span)
}

/// The first of Docker's container names, stripped of its leading `/`.
pub fn clean_name(names: Option<&Vec<String>>) -> Option<String> {
    names?.first().map(|n| n.trim_start_matches('/').to_string())
}

/// Serialize a daemon struct to a Nushell value verbatim — the `full` output
/// format. Falls back to `nothing` on the (practically impossible) serialization
/// failure, so callers never need to thread an error through the list path.
pub fn full_value<T: serde::Serialize>(value: &T, span: Span) -> Value {
    match serde_json::to_value(value) {
        Ok(json) => json.into_value(span),
        Err(_) => Value::nothing(span),
    }
}

/// The first 12 characters of an id — Docker's short form. Callers strip any
/// registry prefix (e.g. `sha256:`) beforehand.
pub fn short_id(id: &str) -> String {
    id.chars().take(12).collect()
}

/// Split a comma-separated flag value (`a=b,c=d`) into trimmed, non-empty parts.
/// Turns nude's `--labels` string into Docker's `label` filter list.
pub fn split_csv(s: &str) -> Vec<String> {
    s.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}
