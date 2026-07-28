//! `nude volume ls` / `nude volume inspect` — list volumes, or inspect one by
//! name.
//!
//! Volumes are the first resource where `list_volumes` and `inspect_volume`
//! return the **same** `Volume` struct, so there is no richer inspect payload to
//! fetch: wide simply projects more columns of the summary and needs **no**
//! inspect fan-out. Data source per output format:
//! - compact : `list_volumes` (`Volume`)
//! - wide    : the same `Volume`, with mountpoint / options / usage columns
//! - full    : the `Volume`, converted verbatim via `IntoValue`
//!
//! `inspect` goes through `inspect_volume` (exact, one call); volumes are keyed
//! by name, so there is no id filter.

use std::collections::BTreeSet;

use bollard::models::Volume;
use bollard::query_parameters::ListVolumesOptionsBuilder;
use nu_plugin::{DynamicCompletionCall, EngineInterface, EvaluatedCall, SimplePluginCommand};
use nu_protocol::{DynamicSuggestion, LabeledError, Record, Signature, Span, Value, engine::ArgType};

use crate::completers;
use crate::decorators::Decorators;
use crate::helpers::{enum_opt, full_value, opt_filesize, opt_int, opt_rfc3339, str_map, str_opt};
use crate::output::OutputFormat;
use crate::plugin::NudePlugin;
use crate::scaffold::{self, CommandSpec, FilterArg};

pub struct VolumeLsCommand;
pub struct VolumeInspectCommand;

const LS: &str = "nude volume ls";
const INSPECT: &str = "nude volume inspect";

/// Docker `/volumes` filters — the single source of truth for both the
/// signature and the request builder.
const FILTERS: &[FilterArg] = &[
    FilterArg::string("driver", "Volume driver (e.g. `local`, or a volume plugin)"),
    FilterArg::string("name", "Name substring (use `inspect` for an exact lookup)"),
    FilterArg::switch("dangling", "Only volumes not used by any container"),
    FilterArg::labels(),
];

impl SimplePluginCommand for VolumeLsCommand {
    type Plugin = NudePlugin;

    fn name(&self) -> &str {
        LS
    }

    fn description(&self) -> &str {
        "List volumes"
    }

    fn signature(&self) -> Signature {
        scaffold::list_signature(&CommandSpec {
            cmd: LS,
            noun: "volume",
            all_help: None,
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
                // Drivers are pluggable, so there's no closed set — offer the
                // ones actually in use.
                "driver" => complete_drivers(plugin),
                "name" => complete_names(plugin),
                "labels" => {
                    completers::complete_labels(&call, all_volumes(plugin)?.iter().map(|v| &v.labels))
                }
                // dangling has no useful completion.
                _ => None,
            },
            _ => None,
        }
    }
}

impl SimplePluginCommand for VolumeInspectCommand {
    type Plugin = NudePlugin;

    fn name(&self) -> &str {
        INSPECT
    }

    fn description(&self) -> &str {
        "Inspect a volume by name"
    }

    fn signature(&self) -> Signature {
        scaffold::inspect_signature(INSPECT, "Volume name", Some("volume"))
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

    /// The volume-name positional + `-o` — the shared inspect/detail shape.
    #[allow(deprecated, reason = "ExperimentalMarker gates an experimental API we opt into")]
    fn get_dynamic_completion(
        &self,
        plugin: &Self::Plugin,
        _engine: &EngineInterface,
        _call: DynamicCompletionCall,
        arg_type: ArgType,
        _experimental: nu_protocol::engine::ExperimentalMarker,
    ) -> Option<Vec<DynamicSuggestion>> {
        completers::ref_and_output(plugin, arg_type, complete_names)
    }
}

/// Every volume, or `None` if the daemon is unreachable. The shared source for
/// the name/driver/label-key completers.
fn all_volumes(plugin: &NudePlugin) -> Option<Vec<Volume>> {
    let docker = plugin.docker().ok()?;
    let opts = ListVolumesOptionsBuilder::default().build();
    let resp = plugin.rt.block_on(docker.list_volumes(Some(opts))).ok()?;
    Some(resp.volumes.unwrap_or_default())
}

/// Volume names as candidates, described by their driver. Feeds the positional
/// and the `--name` filter.
pub(crate) fn complete_names(plugin: &NudePlugin) -> Option<Vec<DynamicSuggestion>> {
    Some(
        all_volumes(plugin)?
            .iter()
            .map(|v| DynamicSuggestion {
                value: v.name.clone(),
                description: Some(v.driver.clone()),
                ..Default::default()
            })
            .collect(),
    )
}

/// The distinct drivers actually in use. Feeds `--driver`.
fn complete_drivers(plugin: &NudePlugin) -> Option<Vec<DynamicSuggestion>> {
    let drivers: BTreeSet<String> = all_volumes(plugin)?.into_iter().map(|v| v.driver).collect();
    Some(
        drivers
            .into_iter()
            .map(|value| DynamicSuggestion {
                value,
                ..Default::default()
            })
            .collect(),
    )
}

async fn run_list(plugin: &NudePlugin, call: &EvaluatedCall) -> anyhow::Result<Value> {
    let decorators = Decorators::from_call(call)?;
    let span = call.head;
    let fmt = scaffold::output_format(call, false)?;

    // Every format is built from the same `Volume` — no inspect fan-out, unlike
    // containers/networks/images.
    let filters = scaffold::collect_filters(call, FILTERS)?;
    let opts = ListVolumesOptionsBuilder::default().filters(&filters).build();
    let volumes = plugin
        .docker()?
        .list_volumes(Some(opts))
        .await?
        .volumes
        .unwrap_or_default();
    let rows = volumes
        .iter()
        .map(|v| match fmt {
            OutputFormat::Compact => compact(v, span, decorators),
            OutputFormat::Wide => wide(v, span, decorators),
            OutputFormat::Full => full_value(v, span),
        })
        .collect();
    Ok(Value::list(rows, span))
}

/// Inspect one volume by exact name (one call). `inspect_volume` returns the
/// same `Volume` a list row would, so wide/full render it directly.
async fn run_inspect(plugin: &NudePlugin, call: &EvaluatedCall) -> anyhow::Result<Value> {
    let name: String = call.req(0)?;
    let decorators = Decorators::from_call(call)?;
    let span = call.head;
    let fmt = scaffold::output_format(call, true)?;

    let v = plugin.docker()?.inspect_volume(&name).await?;
    Ok(match fmt {
        OutputFormat::Full => full_value(&v, span),
        _ => wide(&v, span, decorators),
    })
}

// ---------------------------------------------------------------------------
// Compact
// ---------------------------------------------------------------------------

fn compact(v: &Volume, span: Span, decorators: Decorators) -> Value {
    let mut rec = Record::new();
    rec.push("name", str_opt(Some(v.name.as_str()), span));
    rec.push("driver", str_opt(Some(v.driver.as_str()), span));
    rec.push("scope", enum_opt(v.scope.as_ref(), span));
    rec.push("created", opt_rfc3339(v.created_at.as_deref(), span));
    decorators.apply_labels(&mut rec, Some(&v.labels), span);
    Value::record(rec, span)
}

// ---------------------------------------------------------------------------
// Wide (same struct, more columns)
// ---------------------------------------------------------------------------

fn wide(v: &Volume, span: Span, decorators: Decorators) -> Value {
    let mut rec = Record::new();
    rec.push("name", str_opt(Some(v.name.as_str()), span));
    rec.push("driver", str_opt(Some(v.driver.as_str()), span));
    rec.push("scope", enum_opt(v.scope.as_ref(), span));
    rec.push("mountpoint", str_opt(Some(v.mountpoint.as_str()), span));
    rec.push("created", opt_rfc3339(v.created_at.as_deref(), span));
    rec.push("options", str_map(Some(&v.options), span));
    // Usage is only present when the daemon computed it (e.g. `docker system
    // df`); otherwise nothing, rather than a misleading zero.
    rec.push(
        "size",
        opt_filesize(v.usage_data.as_ref().map(|u| u.size), span),
    );
    rec.push(
        "refs",
        opt_int(v.usage_data.as_ref().map(|u| u.ref_count), span),
    );
    decorators.apply_labels(&mut rec, Some(&v.labels), span);
    Value::record(rec, span)
}
