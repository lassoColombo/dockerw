//! Shared command scaffolding: the common signature, the typed filter table,
//! and output-format resolution.
//!
//! Each resource declares its Docker list filters **once**, as a `&[FilterArg]`
//! table. Both the signature (via [`signature`]) and the request builder (via
//! [`collect_filters`]) are driven from that same table, so a filter flag can
//! never exist in one place but be forgotten in the other.

use std::collections::HashMap;

use nu_plugin::EvaluatedCall;
use nu_protocol::{Category, Signature, SyntaxShape, Type};

use crate::helpers::split_csv;
use crate::output::OutputFormat;

/// How a filter flag is typed, and how its value maps into Docker's filter map.
pub enum Shape {
    /// A single string value (`--driver local`).
    Str,
    /// A single integer, stringified (`--exited 0`).
    Int,
    /// A boolean switch, sent as `"true"` when present (`--dangling`).
    Switch,
    /// nude's comma-separated `--labels a=b,c=d`, split into the API list.
    Labels,
}

/// One Docker list filter, exposed as one typed nude flag.
pub struct FilterArg {
    /// The nude flag name.
    pub flag: &'static str,
    /// The Docker API filter key — equals `flag` except where they differ
    /// (`labels` → `label`).
    pub api_key: &'static str,
    pub shape: Shape,
    pub help: &'static str,
}

impl FilterArg {
    pub const fn string(flag: &'static str, help: &'static str) -> Self {
        Self { flag, api_key: flag, shape: Shape::Str, help }
    }
    pub const fn int(flag: &'static str, help: &'static str) -> Self {
        Self { flag, api_key: flag, shape: Shape::Int, help }
    }
    pub const fn switch(flag: &'static str, help: &'static str) -> Self {
        Self { flag, api_key: flag, shape: Shape::Switch, help }
    }
    /// The standard `--labels a=b,c=d` filter (flag `labels` → API key `label`).
    pub const fn labels() -> Self {
        Self {
            flag: "labels",
            api_key: "label",
            shape: Shape::Labels,
            help: "Labels, comma-separated: `key` or `key=value` (e.g. `a=b,c=d`)",
        }
    }
}

/// Everything [`list_signature`] needs to build a `nude <resource> ls` command's
/// flags.
pub struct CommandSpec {
    /// Full command name, e.g. `"nude image ls"`.
    pub cmd: &'static str,
    /// Singular resource noun, woven into the `--show-labels` help.
    pub noun: &'static str,
    /// `Some(help)` adds an `--all`/`-a` switch; `None` omits it.
    pub all_help: Option<&'static str>,
    /// Whether to add the `--show-labels` decorator switch. `false` for resources
    /// with no label map (e.g. plugins), where the flag would be a no-op.
    pub show_labels: bool,
    /// The resource's filter table.
    pub filters: &'static [FilterArg],
}

/// The common signature every resource's `ls` shares: `--output`/`-o`, an
/// optional `--all`, `--show-labels`, the filter flags, and the standard
/// input/output types + `docker` category. Commands add only their
/// resource-specific extras on top of this. (The exact-lookup positional lives
/// on the sibling `inspect` command — see [`inspect_signature`].)
pub fn list_signature(spec: &CommandSpec) -> Signature {
    let mut sig = Signature::build(spec.cmd).named(
        "output",
        SyntaxShape::String,
        "Output format: compact | wide | full  (default: compact)",
        Some('o'),
    );
    if let Some(help) = spec.all_help {
        sig = sig.switch("all", help, Some('a'));
    }
    if spec.show_labels {
        sig = sig.switch(
            "show-labels",
            format!("Enrich each {} with a `labels` column (compact/wide; full always has them)", spec.noun),
            None,
        );
    }
    add_filters(sig, spec.filters)
        .input_output_types(vec![(Type::Nothing, Type::Any)])
        .category(Category::Custom("docker".to_string()))
}

/// Signature for a resource's `inspect` command (`nude container inspect <ref>`):
/// a **required** object-reference positional and `--output` (default `wide`),
/// plus `--show-labels` for resources that carry a label map. No filters or
/// `--all` — inspect targets exactly one object (to inspect many, use `ls -o
/// wide`). `show_labels_noun` is `Some(noun)` to add the decorator, `None` to
/// omit it (resources with no label map, e.g. plugins).
pub fn inspect_signature(cmd: &str, ref_help: &str, show_labels_noun: Option<&str>) -> Signature {
    let mut sig = Signature::build(cmd)
        .required("name", SyntaxShape::String, ref_help)
        .named(
            "output",
            SyntaxShape::String,
            "Output format: wide | full  (default: wide)",
            Some('o'),
        );
    if let Some(noun) = show_labels_noun {
        sig = sig.switch(
            "show-labels",
            format!("Enrich the {noun} with a `labels` column (wide; full always has them)"),
            None,
        );
    }
    sig.input_output_types(vec![(Type::Nothing, Type::Any)])
        .category(Category::Custom("docker".to_string()))
}

/// Append one flag per filter to a signature — shared by the list [`signature`]
/// and the [`search_signature`] shapes.
fn add_filters(mut sig: Signature, filters: &[FilterArg]) -> Signature {
    for f in filters {
        sig = match f.shape {
            Shape::Switch => sig.switch(f.flag, f.help, None),
            Shape::Int => sig.named(f.flag, SyntaxShape::Int, f.help, None),
            Shape::Str | Shape::Labels => sig.named(f.flag, SyntaxShape::String, f.help, None),
        };
    }
    sig
}

/// Signature for the registry-search command (`nude image search`): a **required**
/// search term, `--output`, `--limit`, and the filter flags. Unlike [`signature`]
/// the positional is required, and there's no `--all` / `--show-labels` / optional
/// name — search returns a flat list from one remote call, with nothing to inspect.
pub fn search_signature(cmd: &str, term_help: &str, filters: &[FilterArg]) -> Signature {
    let sig = Signature::build(cmd)
        .required("term", SyntaxShape::String, term_help)
        .named(
            "output",
            SyntaxShape::String,
            "Output format: compact | wide | full  (default: compact)",
            Some('o'),
        )
        .named(
            "limit",
            SyntaxShape::Int,
            "Maximum number of results (Docker Hub caps at 100; default 25)",
            None,
        );
    add_filters(sig, filters)
        .input_output_types(vec![(Type::Nothing, Type::Any)])
        .category(Category::Custom("docker".to_string()))
}

/// Collect the present filter flags into Docker's `map[string][]string` shape,
/// driven by the same table [`signature`] used.
pub fn collect_filters(
    call: &EvaluatedCall,
    filters: &[FilterArg],
) -> anyhow::Result<HashMap<String, Vec<String>>> {
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    for f in filters {
        match f.shape {
            Shape::Str => {
                if let Some(v) = call.get_flag::<String>(f.flag)? {
                    out.insert(f.api_key.to_string(), vec![v]);
                }
            }
            Shape::Int => {
                if let Some(n) = call.get_flag::<i64>(f.flag)? {
                    out.insert(f.api_key.to_string(), vec![n.to_string()]);
                }
            }
            Shape::Switch => {
                if call.has_flag(f.flag)? {
                    out.insert(f.api_key.to_string(), vec!["true".to_string()]);
                }
            }
            Shape::Labels => {
                if let Some(s) = call.get_flag::<String>(f.flag)? {
                    let values = split_csv(&s);
                    if !values.is_empty() {
                        out.insert(f.api_key.to_string(), values);
                    }
                }
            }
        }
    }
    Ok(out)
}

/// The signature for a singleton `nude system <sub>` command: only `--output`/`-o`
/// plus the shared io types and `docker` category — no name, filters, or labels.
/// The singleton commands (`df`/`info`/`version`) don't fit [`signature`]'s
/// list+inspect shape, but still share the output flag and category.
pub fn singleton_signature(cmd: &str) -> Signature {
    Signature::build(cmd)
        .named(
            "output",
            SyntaxShape::String,
            "Output format: compact | wide | full  (default: compact)",
            Some('o'),
        )
        .input_output_types(vec![(Type::Nothing, Type::Any)])
        .category(Category::Custom("docker".to_string()))
}

/// Signature for a per-object detail sub-verb (`nude container diff <name>`,
/// `nude image history <ref>`, …): a **required** object-reference positional and
/// `--output`. No filters, `--all`, `--show-labels`, or optional name — each
/// returns one focused view of a single object.
pub fn subcommand_signature(cmd: &str, ref_help: &str) -> Signature {
    Signature::build(cmd)
        .required("name", SyntaxShape::String, ref_help)
        .named(
            "output",
            SyntaxShape::String,
            "Output format: compact | wide | full  (default: compact)",
            Some('o'),
        )
        .input_output_types(vec![(Type::Nothing, Type::Any)])
        .category(Category::Custom("docker".to_string()))
}

/// Resolve `--output`, defaulting to `wide` for a named lookup and `compact` for
/// a list. An unrecognized value falls back to the same default.
pub fn output_format(call: &EvaluatedCall, name_present: bool) -> anyhow::Result<OutputFormat> {
    Ok(call
        .get_flag::<String>("output")?
        .and_then(|s| s.parse::<OutputFormat>().ok())
        .unwrap_or(if name_present {
            OutputFormat::Wide
        } else {
            OutputFormat::Compact
        }))
}
