//! `nude image ls` / `nude image inspect` — list images, or inspect one by
//! name/ID.
//!
//! Data source per output format:
//! - compact : `list_images` summaries, **exploded to one row per `repo:tag`**
//!   (a dangling/untagged image still gets one row) so every cell stays a
//!   primitive — matching `docker images`.
//! - wide    : `inspect_image` (`ImageInspect`, fanned out over the list, for
//!   `ls -o wide`), one row per image with list columns (tags, env, layers, …).
//! - full    : `inspect_image`, converted verbatim via `IntoValue`.
//!
//! `inspect` always goes through `inspect_image` (exact, one call).

use bollard::models::{ImageInspect, ImageSummary};
use bollard::query_parameters::ListImagesOptionsBuilder;
use futures::future::join_all;
use nu_plugin::{DynamicCompletionCall, EngineInterface, EvaluatedCall, SimplePluginCommand};
use nu_protocol::{DynamicSuggestion, LabeledError, Record, Signature, Span, Value, engine::ArgType};

use crate::completers;
use crate::decorators::Decorators;
use crate::helpers::{epoch_date, full_value, opt_filesize, opt_rfc3339, short_id, str_list, str_opt};
use crate::output::OutputFormat;
use crate::plugin::NudePlugin;
use crate::scaffold::{self, CommandSpec, FilterArg};

pub struct ImageLsCommand;
pub struct ImageInspectCommand;

const LS: &str = "nude image ls";
const INSPECT: &str = "nude image inspect";

/// Docker `/images/json` filters — the single source of truth for both the
/// signature and the request builder.
const FILTERS: &[FilterArg] = &[
    FilterArg::string("reference", "Reference pattern (name[:tag], e.g. `nginx` or `ngin*`)"),
    FilterArg::string("before", "Created before this image (name or id)"),
    FilterArg::string("since", "Created since this image (name or id)"),
    FilterArg::switch("dangling", "Only dangling (untagged) images"),
    FilterArg::labels(),
];

impl SimplePluginCommand for ImageLsCommand {
    type Plugin = NudePlugin;

    fn name(&self) -> &str {
        LS
    }

    fn description(&self) -> &str {
        "List images"
    }

    fn signature(&self) -> Signature {
        scaffold::list_signature(&CommandSpec {
            cmd: LS,
            noun: "image",
            all_help: Some("Include intermediate (layer) images"),
            show_labels: true,
            filters: FILTERS,
        })
    }

    fn run(
        &self,
        plugin: &Self::Plugin,
        _engine: &EngineInterface,
        call: &EvaluatedCall,
        _input: &Value,
    ) -> Result<Value, LabeledError> {
        plugin.block_on_labeled(run_list(plugin, call))
    }

    /// Dynamic argument completions for the filter flags. Same contract as
    /// `container.rs`: hand back the full candidate set and let Nushell's
    /// `NuMatcher` filter it.
    #[allow(deprecated, reason = "ExperimentalMarker gates an experimental API we opt into")]
    fn get_dynamic_completion(
        &self,
        plugin: &Self::Plugin,
        _engine: &EngineInterface,
        call: DynamicCompletionCall,
        arg_type: ArgType,
        _experimental: nu_protocol::engine::ExperimentalMarker,
    ) -> Option<Vec<DynamicSuggestion>> {
        match arg_type {
            ArgType::Flag(name) => match name.as_ref() {
                "output" | "o" => Some(completers::from_pairs(OutputFormat::ALL)),
                // All three reference an existing image.
                "reference" | "before" | "since" => complete_refs(plugin),
                "labels" => {
                    completers::complete_labels(&call, all_images(plugin)?.iter().map(|i| &i.labels))
                }
                // dangling has no useful completion.
                _ => None,
            },
            _ => None,
        }
    }
}

impl SimplePluginCommand for ImageInspectCommand {
    type Plugin = NudePlugin;

    fn name(&self) -> &str {
        INSPECT
    }

    fn description(&self) -> &str {
        "Inspect an image by name/ID"
    }

    fn signature(&self) -> Signature {
        scaffold::inspect_signature(INSPECT, "Image reference (repo:tag) or ID", Some("image"))
    }

    fn run(
        &self,
        plugin: &Self::Plugin,
        _engine: &EngineInterface,
        call: &EvaluatedCall,
        _input: &Value,
    ) -> Result<Value, LabeledError> {
        plugin.block_on_labeled(run_inspect(plugin, call))
    }

    /// The image-ref positional + `-o` — the shared inspect/detail shape.
    #[allow(deprecated, reason = "ExperimentalMarker gates an experimental API we opt into")]
    fn get_dynamic_completion(
        &self,
        plugin: &Self::Plugin,
        _engine: &EngineInterface,
        _call: DynamicCompletionCall,
        arg_type: ArgType,
        _experimental: nu_protocol::engine::ExperimentalMarker,
    ) -> Option<Vec<DynamicSuggestion>> {
        completers::ref_and_output(plugin, arg_type, complete_refs)
    }
}

/// Live image references (`repo:tag`), skipping the untagged `<none>:<none>`
/// placeholder. Owned by the image resource; also feeds container's
/// `--ancestor`, so it is `pub(crate)`.
pub(crate) fn complete_refs(plugin: &NudePlugin) -> Option<Vec<DynamicSuggestion>> {
    let docker = plugin.docker().ok()?;
    let opts = ListImagesOptionsBuilder::default().build();
    let images = plugin.rt.block_on(docker.list_images(Some(opts))).ok()?;
    Some(
        images
            .iter()
            .flat_map(|i| i.repo_tags.iter())
            .filter(|t| t.as_str() != "<none>:<none>")
            .map(|value| DynamicSuggestion {
                value: value.clone(),
                ..Default::default()
            })
            .collect(),
    )
}

/// Every image including intermediates, or `None` if the daemon is unreachable.
/// Source for the label-key completer.
fn all_images(plugin: &NudePlugin) -> Option<Vec<ImageSummary>> {
    let docker = plugin.docker().ok()?;
    let opts = ListImagesOptionsBuilder::default().all(true).build();
    plugin.rt.block_on(docker.list_images(Some(opts))).ok()
}

async fn run_list(plugin: &NudePlugin, call: &EvaluatedCall) -> anyhow::Result<Value> {
    let all: bool = call.has_flag("all")?;
    let decorators = Decorators::from_call(call)?;
    let span = call.head;
    let fmt = scaffold::output_format(call, false)?;

    let docker = plugin.docker()?;
    let filters = scaffold::collect_filters(call, FILTERS)?;
    let opts = ListImagesOptionsBuilder::default()
        .all(all)
        .filters(&filters)
        .build();
    let summaries = docker.list_images(Some(opts)).await?;
    match fmt {
        // Compact explodes each image into one row per repo:tag.
        OutputFormat::Compact => Ok(Value::list(
            summaries
                .iter()
                .flat_map(|i| compact_rows(i, span, decorators))
                .collect(),
            span,
        )),
        // wide/full come from inspect → fan out concurrently.
        _ => {
            let ids: Vec<String> = summaries.into_iter().map(|i| i.id).collect();
            let inspected = join_all(ids.iter().map(|id| docker.inspect_image(id))).await;
            let rows: Vec<Value> = inspected
                .into_iter()
                .filter_map(Result::ok)
                .map(|r| match fmt {
                    OutputFormat::Full => full_value(&r, span),
                    _ => wide(&r, span, decorators),
                })
                .collect();
            Ok(Value::list(rows, span))
        }
    }
}

/// Inspect one image by exact reference/ID (one call). Shown wide; `-o full`
/// returns the raw inspect payload.
async fn run_inspect(plugin: &NudePlugin, call: &EvaluatedCall) -> anyhow::Result<Value> {
    let name: String = call.req(0)?;
    let decorators = Decorators::from_call(call)?;
    let span = call.head;
    let fmt = scaffold::output_format(call, true)?;

    let resp = plugin.docker()?.inspect_image(&name).await?;
    Ok(match fmt {
        OutputFormat::Full => full_value(&resp, span),
        _ => wide(&resp, span, decorators),
    })
}

// ---------------------------------------------------------------------------
// Reference parsing
// ---------------------------------------------------------------------------

/// Split a Docker `repository:tag` reference. The tag is after the **last**
/// colon — a registry `host:port` colon is always followed by a `/path`, so it
/// is never the last one. Docker's `<none>` placeholder maps to `None`.
fn split_ref(reference: &str) -> (Option<&str>, Option<&str>) {
    let (repo, tag) = reference.rsplit_once(':').unwrap_or((reference, ""));
    (clean(repo), clean(tag))
}

/// A reference part, or `None` if empty or Docker's `<none>` placeholder.
fn clean(part: &str) -> Option<&str> {
    (!part.is_empty() && part != "<none>").then_some(part)
}

/// The short (12-char) form of an image id, dropping the `sha256:` algorithm
/// prefix Docker puts on summary/inspect ids.
fn short_image_id(id: &str) -> String {
    short_id(id.strip_prefix("sha256:").unwrap_or(id))
}

// ---------------------------------------------------------------------------
// Compact (from a list summary, exploded per repo:tag)
// ---------------------------------------------------------------------------

fn compact_rows(img: &ImageSummary, span: Span, decorators: Decorators) -> Vec<Value> {
    // One row per tag; an untagged image still gets a single (dangling) row.
    let refs: Vec<&str> = if img.repo_tags.is_empty() {
        vec!["<none>:<none>"]
    } else {
        img.repo_tags.iter().map(String::as_str).collect()
    };
    let id = short_image_id(&img.id);
    refs.iter()
        .map(|r| {
            let (repository, tag) = split_ref(r);
            let mut rec = Record::new();
            rec.push("repository", str_opt(repository, span));
            rec.push("tag", str_opt(tag, span));
            rec.push("id", str_opt(Some(id.as_str()), span));
            rec.push("created", epoch_date(img.created, span));
            rec.push("size", Value::filesize(img.size, span));
            decorators.apply_labels(&mut rec, Some(&img.labels), span);
            Value::record(rec, span)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Wide (from an inspect response, one row per image)
// ---------------------------------------------------------------------------

fn wide(img: &ImageInspect, span: Span, decorators: Decorators) -> Value {
    let config = img.config.as_ref();
    let id = img.id.as_deref().map(short_image_id);

    let mut rec = Record::new();
    rec.push("id", str_opt(id.as_deref(), span));
    rec.push("repo_tags", str_list(img.repo_tags.as_ref(), span));
    rec.push("repo_digests", str_list(img.repo_digests.as_ref(), span));
    rec.push("size", opt_filesize(img.size, span));
    rec.push("created", opt_rfc3339(img.created.as_deref(), span));
    rec.push("architecture", str_opt(img.architecture.as_deref(), span));
    rec.push("os", str_opt(img.os.as_deref(), span));
    rec.push("author", str_opt(img.author.as_deref(), span));
    rec.push("command", str_list(config.and_then(|c| c.cmd.as_ref()), span));
    rec.push(
        "entrypoint",
        str_list(config.and_then(|c| c.entrypoint.as_ref()), span),
    );
    rec.push("env", str_list(config.and_then(|c| c.env.as_ref()), span));
    rec.push(
        "exposed_ports",
        str_list(config.and_then(|c| c.exposed_ports.as_ref()), span),
    );
    rec.push(
        "working_dir",
        str_opt(config.and_then(|c| c.working_dir.as_deref()), span),
    );
    rec.push(
        "layers",
        str_list(img.root_fs.as_ref().and_then(|r| r.layers.as_ref()), span),
    );
    decorators.apply_labels(&mut rec, config.and_then(|c| c.labels.as_ref()), span);
    Value::record(rec, span)
}

#[cfg(test)]
mod tests {
    use super::split_ref;

    #[test]
    fn splits_repository_and_tag_at_the_last_colon() {
        assert_eq!(split_ref("nginx:latest"), (Some("nginx"), Some("latest")));
        assert_eq!(
            split_ref("localhost:5000/team/img:v1.2"),
            (Some("localhost:5000/team/img"), Some("v1.2"))
        );
    }

    #[test]
    fn none_placeholder_and_empties_become_none() {
        assert_eq!(split_ref("<none>:<none>"), (None, None));
        assert_eq!(split_ref("repo:<none>"), (Some("repo"), None));
        assert_eq!(split_ref("<none>:latest"), (None, Some("latest")));
    }
}
