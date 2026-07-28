use chrono::{TimeZone, Utc};
use nu_protocol::{IntoValue, Record, Span, Value};
use std::collections::HashMap;

pub fn epoch_date(secs: i64, span: Span) -> Value {
    match Utc.timestamp_opt(secs, 0) {
        chrono::LocalResult::Single(dt) => Value::date(dt.fixed_offset(), span),
        _ => Value::nothing(span),
    }
}

pub fn opt_epoch(secs: Option<i64>, span: Span) -> Value {
    match secs {
        Some(s) => epoch_date(s, span),
        None => Value::nothing(span),
    }
}

pub fn rfc3339_date(s: &str, span: Span) -> Value {
    match chrono::DateTime::parse_from_rfc3339(s) {
        Ok(dt) => Value::date(dt, span),
        Err(_) => Value::nothing(span),
    }
}

pub fn opt_rfc3339(s: Option<&str>, span: Span) -> Value {
    match s {
        Some(s) if !s.is_empty() && !s.starts_with("0001-01-01") => rfc3339_date(s, span),
        _ => Value::nothing(span),
    }
}

pub fn str_opt(s: Option<&str>, span: Span) -> Value {
    match s {
        Some(s) if !s.is_empty() => Value::string(s, span),
        _ => Value::nothing(span),
    }
}

pub fn opt_int(n: Option<i64>, span: Span) -> Value {
    match n {
        Some(n) => Value::int(n, span),
        None => Value::nothing(span),
    }
}

pub fn opt_bool(b: Option<bool>, span: Span) -> Value {
    match b {
        Some(b) => Value::bool(b, span),
        None => Value::nothing(span),
    }
}

pub fn opt_filesize(bytes: Option<i64>, span: Span) -> Value {
    match bytes {
        Some(n) => Value::filesize(n, span),
        None => Value::nothing(span),
    }
}

pub fn enum_opt<T: serde::Serialize>(v: Option<&T>, span: Span) -> Value {
    match v.map(serde_json::to_value) {
        Some(Ok(serde_json::Value::String(s))) if !s.is_empty() => Value::string(s, span),
        _ => Value::nothing(span),
    }
}

pub fn str_map(map: Option<&HashMap<String, String>>, span: Span) -> Value {
    let mut rec = Record::new();
    if let Some(map) = map {
        for (k, v) in map {
            rec.push(k.clone(), Value::string(v.clone(), span));
        }
    }
    Value::record(rec, span)
}

pub fn str_list(items: Option<&Vec<String>>, span: Span) -> Value {
    let rows = items
        .map(|v| v.iter().map(|s| Value::string(s, span)).collect())
        .unwrap_or_default();
    Value::list(rows, span)
}

pub fn clean_name(names: Option<&Vec<String>>) -> Option<String> {
    names?
        .first()
        .map(|n| n.trim_start_matches('/').to_string())
}

pub fn full_value<T: serde::Serialize>(value: &T, span: Span) -> Value {
    match serde_json::to_value(value) {
        Ok(json) => json.into_value(span),
        Err(_) => Value::nothing(span),
    }
}

pub fn short_id(id: &str) -> String {
    id.chars().take(12).collect()
}

pub fn split_csv(s: &str) -> Vec<String> {
    s.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}
