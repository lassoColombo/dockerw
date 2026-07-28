//! `nude plugin ls` / `nude plugin inspect` — list managed Docker plugins, or
//! inspect one by name.
//!
//! Like volumes, `list_plugins` and `inspect_plugin` both return the same
//! `Plugin` struct (`same_type`), so wide just projects more columns of the
//! summary — no inspect fan-out. Data source per output format:
//! - compact : `Plugin` headline (name, id, enabled, reference, description)
//! - wide    : the same `Plugin`, with capabilities / socket / entrypoint / env
//! - full    : the `Plugin`, converted verbatim via `IntoValue`
//!
//! Plugins carry **no label map**, so this resource has neither a `--labels`
//! filter nor the `--show-labels` decorator (`CommandSpec.show_labels: false`).
//! Most daemons have zero plugins installed.

use bollard::models::Plugin;
use bollard::query_parameters::ListPluginsOptionsBuilder;
use nu_plugin::{DynamicCompletionCall, EngineInterface, EvaluatedCall, SimplePluginCommand};
use nu_protocol::{DynamicSuggestion, LabeledError, Record, Signature, Span, Value, engine::ArgType};

use crate::completers;
use crate::helpers::{full_value, short_id, str_list, str_opt};
use crate::output::OutputFormat;
use crate::plugin::NudePlugin;
use crate::scaffold::{self, CommandSpec, FilterArg};

pub struct PluginLsCommand;
pub struct PluginInspectCommand;

const LS: &str = "nude plugin ls";
const INSPECT: &str = "nude plugin inspect";

/// Docker `/plugins` filters — the single source of truth for both the signature
/// and the request builder. Note the API key is `enabled`, not `enable` (the
/// latter is rejected with 400 by the daemon).
const FILTERS: &[FilterArg] = &[
    FilterArg::string(
        "capability",
        "Capability: volumedriver|networkdriver|ipamdriver|authz|logdriver|metricscollector",
    ),
    // `enabled` is boolean; nushell lexes a bare `true`/`false` as a bool (not a
    // string), so a switch is the clean UX. Disabled-only is `| where not enabled`.
    FilterArg::switch("enabled", "Only enabled plugins"),
];

impl SimplePluginCommand for PluginLsCommand {
    type Plugin = NudePlugin;

    fn name(&self) -> &str {
        LS
    }

    fn description(&self) -> &str {
        "List managed plugins"
    }

    fn signature(&self) -> Signature {
        scaffold::list_signature(&CommandSpec {
            cmd: LS,
            noun: "plugin",
            all_help: None,
            show_labels: false,
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
        _plugin: &Self::Plugin,
        _engine: &EngineInterface,
        _call: DynamicCompletionCall,
        arg_type: ArgType,
        _experimental: nu_protocol::engine::ExperimentalMarker,
    ) -> Option<Vec<DynamicSuggestion>> {
        match arg_type {
            ArgType::Flag(name) => match name.as_ref() {
                "output" | "o" => Some(completers::from_pairs(OutputFormat::ALL)),
                // Closed enums → static lists.
                "capability" => Some(completers::from_pairs(CAPABILITIES)),
                _ => None,
            },
            _ => None,
        }
    }
}

impl SimplePluginCommand for PluginInspectCommand {
    type Plugin = NudePlugin;

    fn name(&self) -> &str {
        INSPECT
    }

    fn description(&self) -> &str {
        "Inspect a managed plugin by name"
    }

    fn signature(&self) -> Signature {
        scaffold::inspect_signature(INSPECT, "Plugin name", None)
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

    /// The plugin-name positional + `-o` — the shared inspect/detail shape.
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

/// `--capability` filter values (the built-in plugin capability kinds).
const CAPABILITIES: &[(&str, &str)] = &[
    ("volumedriver", "Provides a volume driver"),
    ("networkdriver", "Provides a network driver"),
    ("ipamdriver", "Provides an IPAM driver"),
    ("authz", "Authorization plugin"),
    ("logdriver", "Provides a logging driver"),
    ("metricscollector", "Exposes metrics to collect"),
];

/// Every plugin, or `None` if the daemon is unreachable. The shared source for
/// the name completer.
fn all_plugins(plugin: &NudePlugin) -> Option<Vec<Plugin>> {
    let docker = plugin.docker().ok()?;
    let opts = ListPluginsOptionsBuilder::new().build();
    plugin.rt.block_on(docker.list_plugins(Some(opts))).ok()
}

/// Plugin names as candidates, described by their config description. Feeds the
/// positional.
fn complete_names(plugin: &NudePlugin) -> Option<Vec<DynamicSuggestion>> {
    Some(
        all_plugins(plugin)?
            .iter()
            .map(|p| DynamicSuggestion {
                value: p.name.clone(),
                description: Some(p.config.description.clone()),
                ..Default::default()
            })
            .collect(),
    )
}

async fn run_list(plugin: &NudePlugin, call: &EvaluatedCall) -> anyhow::Result<Value> {
    let span = call.head;
    let fmt = scaffold::output_format(call, false)?;

    // Every format is built from the same `Plugin` — no inspect fan-out, like
    // volumes.
    let filters = scaffold::collect_filters(call, FILTERS)?;
    let opts = ListPluginsOptionsBuilder::new().filters(&filters).build();
    let plugins = plugin.docker()?.list_plugins(Some(opts)).await?;
    let rows = plugins
        .iter()
        .map(|p| match fmt {
            OutputFormat::Compact => compact(p, span),
            OutputFormat::Wide => wide(p, span),
            OutputFormat::Full => full_value(p, span),
        })
        .collect();
    Ok(Value::list(rows, span))
}

/// Inspect one plugin by exact name (one call). `inspect_plugin` returns the
/// same `Plugin` a list row would, so wide/full render it directly.
async fn run_inspect(plugin: &NudePlugin, call: &EvaluatedCall) -> anyhow::Result<Value> {
    let name: String = call.req(0)?;
    let span = call.head;
    let fmt = scaffold::output_format(call, true)?;

    let p = plugin.docker()?.inspect_plugin(&name).await?;
    Ok(match fmt {
        OutputFormat::Full => full_value(&p, span),
        _ => wide(&p, span),
    })
}

// ---------------------------------------------------------------------------
// Compact
// ---------------------------------------------------------------------------

fn compact(p: &Plugin, span: Span) -> Value {
    let mut rec = Record::new();
    rec.push("name", str_opt(Some(p.name.as_str()), span));
    rec.push("id", str_opt(p.id.as_deref().map(short_id).as_deref(), span));
    rec.push("enabled", Value::bool(p.enabled, span));
    rec.push("reference", str_opt(p.plugin_reference.as_deref(), span));
    rec.push("description", str_opt(Some(p.config.description.as_str()), span));
    Value::record(rec, span)
}

// ---------------------------------------------------------------------------
// Wide (same struct, more columns)
// ---------------------------------------------------------------------------

fn wide(p: &Plugin, span: Span) -> Value {
    let c = &p.config;
    let mut rec = Record::new();
    rec.push("name", str_opt(Some(p.name.as_str()), span));
    rec.push("id", str_opt(p.id.as_deref().map(short_id).as_deref(), span));
    rec.push("enabled", Value::bool(p.enabled, span));
    rec.push("reference", str_opt(p.plugin_reference.as_deref(), span));
    rec.push("description", str_opt(Some(c.description.as_str()), span));
    rec.push("documentation", str_opt(Some(c.documentation.as_str()), span));
    rec.push("capabilities", str_list(Some(&c.interface.types), span));
    rec.push("socket", str_opt(Some(c.interface.socket.as_str()), span));
    rec.push("entrypoint", str_list(Some(&c.entrypoint), span));
    rec.push("work_dir", str_opt(Some(c.work_dir.as_str()), span));
    // The active runtime settings (as applied), not the config's definitions.
    rec.push("env", str_list(Some(&p.settings.env), span));
    rec.push("args", str_list(Some(&p.settings.args), span));
    Value::record(rec, span)
}
