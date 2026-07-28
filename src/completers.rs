//! Resource-agnostic completion building blocks shared across commands.
//!
//! Only genuinely reusable machinery lives here: the static-enum helper, the
//! argument readers, and the label-filter cycle. Completers that fetch a
//! specific Docker resource (container names, images, …) stay with the command
//! that owns them.
//!
//! Every completer returns the *full* candidate set — Nushell filters it against
//! the typed prefix.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use nu_plugin::DynamicCompletionCall;
use nu_protocol::{
    DynamicSuggestion,
    ast::{Call, Expr, Expression},
    engine::ArgType,
};

use crate::output::OutputFormat;
use crate::plugin::NudePlugin;

/// Turn a static `(value, description)` table into suggestions. For closed
/// enums like `--status` / `--health` / `--output`; an empty description is
/// omitted rather than shown blank.
pub fn from_pairs(pairs: &[(&str, &str)]) -> Vec<DynamicSuggestion> {
    pairs
        .iter()
        .map(|&(value, desc)| DynamicSuggestion {
            value: value.to_string(),
            description: (!desc.is_empty()).then(|| desc.to_string()),
            ..Default::default()
        })
        .collect()
}

/// Completion for the "single object-ref positional + `-o` output flag" shape
/// shared by every `inspect` command and the per-object detail sub-verbs
/// (`diff`/`top`/`history`): the positional delegates to the owning resource's
/// name/ref completer, and `-o` offers the output formats. Same "one resource
/// owns how to complete its refs" rule as the cross-reference filters — callers
/// pass that resource's `pub(crate)` completer.
pub fn ref_and_output(
    plugin: &NudePlugin,
    arg_type: ArgType,
    name_completer: fn(&NudePlugin) -> Option<Vec<DynamicSuggestion>>,
) -> Option<Vec<DynamicSuggestion>> {
    match arg_type {
        ArgType::Positional(0) => name_completer(plugin),
        ArgType::Flag(name) if matches!(name.as_ref(), "output" | "o") => {
            Some(from_pairs(OutputFormat::ALL))
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Reading already-typed arguments during completion
// ---------------------------------------------------------------------------

/// The string behind a literal expression, if any. Mirrors nuke so completers
/// can read the values other flags already hold.
pub fn expr_as_str(expr: &Expression) -> Option<&str> {
    match &expr.expr {
        Expr::String(s) | Expr::RawString(s) | Expr::GlobPattern(s, _) => Some(s.as_str()),
        _ => None,
    }
}

/// The string value a named flag currently holds in the parsed call.
pub fn flag_str<'a>(call: &'a Call, name: &str) -> Option<&'a str> {
    call.named_iter()
        .find(|(n, _, _)| n.item == name)
        .and_then(|(_, _, expr)| expr.as_ref())
        .and_then(expr_as_str)
}

/// The partial text of the flag *currently being completed*.
///
/// Nushell appends a one-character `a` placeholder to the buffer so it parses
/// while mid-word; `call.strip` says it's there, so drop the trailing char to
/// recover exactly what the user has typed.
pub fn flag_prefix<'a>(call: &'a DynamicCompletionCall, name: &str) -> &'a str {
    let raw = flag_str(&call.call, name).unwrap_or_default();
    if call.strip {
        &raw[..raw.len().saturating_sub(1)]
    } else {
        raw
    }
}

// ---------------------------------------------------------------------------
// Label filter (comma-separated `key=value` pairs)
// ---------------------------------------------------------------------------

/// Context-sensitive completion for a `--labels a=b,c=d` filter, given the text
/// typed so far and the observed `key -> {values}` seen on the resource.
///
/// Candidates are whole new flag values (Nushell matches them against `typed`),
/// and never end in a space, so completing cycles within the one token:
/// - typing a key            → `key=`
/// - after `key=`            → each known value → `key=value`
/// - after a complete `key=value` → the separating comma → `key=value,`
/// - after that comma        → the next key, and so on.
pub fn label_filter(
    typed: &str,
    labels: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<DynamicSuggestion> {
    // Everything up to and including the last comma is committed; the tail is
    // the pair being edited now.
    let (committed, last) = match typed.rfind(',') {
        Some(i) => typed.split_at(i + 1),
        None => ("", typed),
    };

    match last.split_once('=') {
        Some((key, val)) => match labels.get(key) {
            // Value already matches a known one → offer the separating comma.
            Some(values) if !val.is_empty() && values.contains(val) => {
                vec![pair(format!("{typed},"))]
            }
            // Otherwise offer this key's known values (Nushell filters by `val`).
            Some(values) => values
                .iter()
                .map(|v| pair(format!("{committed}{key}={v}")))
                .collect(),
            None => Vec::new(),
        },
        // Editing a key → offer every known key, ready to take its value.
        None => labels
            .keys()
            .map(|k| pair(format!("{committed}{k}=")))
            .collect(),
    }
}

/// Pool `key -> {values}` from a set of label maps, then drive [`label_filter`].
/// This is the shared body of every resource's `--labels` completer: pass the
/// text typed so far and each resource object's label map.
pub fn label_filter_from<'a>(
    typed: &str,
    maps: impl IntoIterator<Item = &'a HashMap<String, String>>,
) -> Vec<DynamicSuggestion> {
    let mut labels: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for map in maps {
        for (k, v) in map {
            labels.entry(k.clone()).or_default().insert(v.clone());
        }
    }
    label_filter(typed, &labels)
}

/// A resource's `--labels` completer: read the in-progress `--labels` text and
/// pool `key -> {values}` from the resource's label maps. Every command's
/// `--labels` arm is a one-liner over this.
pub fn complete_labels<'a>(
    call: &DynamicCompletionCall,
    maps: impl IntoIterator<Item = &'a HashMap<String, String>>,
) -> Option<Vec<DynamicSuggestion>> {
    Some(label_filter_from(flag_prefix(call, "labels"), maps))
}

/// A suggestion that replaces the whole flag value and keeps the cursor inside
/// the token (no trailing space) so the comma-separated list can continue.
fn pair(value: String) -> DynamicSuggestion {
    DynamicSuggestion {
        value,
        append_whitespace: false,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels() -> BTreeMap<String, BTreeSet<String>> {
        BTreeMap::from([
            (
                "app".to_string(),
                BTreeSet::from(["web".to_string(), "db".to_string()]),
            ),
            ("env".to_string(), BTreeSet::from(["prod".to_string()])),
        ])
    }

    fn values(suggestions: &[DynamicSuggestion]) -> Vec<&str> {
        suggestions.iter().map(|s| s.value.as_str()).collect()
    }

    #[test]
    fn nothing_typed_offers_every_key() {
        assert_eq!(values(&label_filter("", &labels())), ["app=", "env="]);
    }

    #[test]
    fn after_key_eq_offers_that_key_values() {
        assert_eq!(values(&label_filter("app=", &labels())), ["app=db", "app=web"]);
    }

    #[test]
    fn partial_value_still_offers_values_for_nushell_to_filter() {
        assert_eq!(values(&label_filter("app=w", &labels())), ["app=db", "app=web"]);
    }

    #[test]
    fn exact_value_offers_the_separating_comma() {
        assert_eq!(values(&label_filter("app=web", &labels())), ["app=web,"]);
    }

    #[test]
    fn next_key_keeps_the_committed_pairs() {
        assert_eq!(
            values(&label_filter("env=prod,ap", &labels())),
            ["env=prod,app=", "env=prod,env="]
        );
    }

    #[test]
    fn comma_after_a_committed_pair_keeps_the_prefix() {
        assert_eq!(
            values(&label_filter("env=prod,app=web", &labels())),
            ["env=prod,app=web,"]
        );
    }

    #[test]
    fn unknown_key_offers_nothing() {
        assert!(label_filter("nope=", &labels()).is_empty());
    }

    #[test]
    fn candidates_never_end_the_token_with_a_space() {
        assert!(
            label_filter("app=", &labels())
                .iter()
                .all(|s| !s.append_whitespace)
        );
    }
}
