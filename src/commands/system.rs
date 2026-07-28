//! `nude system` — singleton daemon introspection: `info` / `version`.
//!
//! A different shape from the list+inspect resources: no name positional, no
//! filters, no inspect fan-out, no labels. Each command fetches one singleton
//! from the daemon and projects it by output format; only `-o compact|wide|full`
//! is shared with the resource commands (via [`scaffold::singleton_signature`]).
//! Data source per command:
//! - `info`    → `SystemInfo`: compact is the `docker info` headline, wide adds
//!   drivers / flags / security options / warnings, full is the raw payload.
//! - `version` → `SystemVersion`: compact is the server-version summary, wide
//!   adds the component breakdown, full is raw.
//!
//! `df` is intentionally absent: bollard 0.21's `SystemDataUsageResponse` models
//! a *future* API shape (`ImageUsage`/… computed server-side) that real daemons
//! don't return — they still send the classic `{LayersSize, Images[], …}` — and
//! bollard exposes no raw path to that payload. Deferred; see list.yaml.

use bollard::models::{SystemInfo, SystemVersion};
use nu_plugin::{DynamicCompletionCall, EngineInterface, EvaluatedCall, SimplePluginCommand};
use nu_protocol::{DynamicSuggestion, LabeledError, Record, Signature, Span, Value, engine::ArgType};

use crate::completers;
use crate::helpers::{
    enum_opt, full_value, opt_bool, opt_filesize, opt_int, opt_rfc3339, str_list, str_opt,
};
use crate::output::OutputFormat;
use crate::plugin::NudePlugin;
use crate::scaffold;

/// `-o`/`--output` completion — the only completable flag the singleton system
/// commands have. Shared by all three.
fn output_completion(arg_type: ArgType) -> Option<Vec<DynamicSuggestion>> {
    match arg_type {
        ArgType::Flag(name) if matches!(name.as_ref(), "output" | "o") => {
            Some(completers::from_pairs(OutputFormat::ALL))
        }
        _ => None,
    }
}

// ===========================================================================
// info
// ===========================================================================

pub struct SystemInfoCommand;

const INFO_NAME: &str = "nude system info";

impl SimplePluginCommand for SystemInfoCommand {
    type Plugin = NudePlugin;

    fn name(&self) -> &str {
        INFO_NAME
    }

    fn description(&self) -> &str {
        "Daemon-wide info: versions, counts, driver, host resources"
    }

    fn signature(&self) -> Signature {
        scaffold::singleton_signature(INFO_NAME)
    }

    fn run(
        &self,
        plugin: &Self::Plugin,
        _engine: &EngineInterface,
        call: &EvaluatedCall,
        _input: &Value,
    ) -> Result<Value, LabeledError> {
        plugin.block_on_labeled(run_info(plugin, call))
    }

    #[allow(deprecated, reason = "ExperimentalMarker gates an experimental API we opt into")]
    fn get_dynamic_completion(
        &self,
        _plugin: &Self::Plugin,
        _engine: &EngineInterface,
        _call: DynamicCompletionCall,
        arg_type: ArgType,
        _experimental: nu_protocol::engine::ExperimentalMarker,
    ) -> Option<Vec<DynamicSuggestion>> {
        output_completion(arg_type)
    }
}

async fn run_info(plugin: &NudePlugin, call: &EvaluatedCall) -> anyhow::Result<Value> {
    let span = call.head;
    let fmt = scaffold::output_format(call, false)?;
    let info = plugin.docker()?.info().await?;
    Ok(match fmt {
        OutputFormat::Full => full_value(&info, span),
        OutputFormat::Compact => info_record(&info, span, false),
        OutputFormat::Wide => info_record(&info, span, true),
    })
}

/// The `docker info` projection. `wide` appends drivers, host flags, security
/// options and warnings; deeply nested sub-structs (plugins, runtimes map,
/// registry config, swarm) are left to `full`.
fn info_record(i: &SystemInfo, span: Span, wide: bool) -> Value {
    let mut rec = Record::new();
    rec.push("name", str_opt(i.name.as_deref(), span));
    rec.push("server_version", str_opt(i.server_version.as_deref(), span));
    rec.push("containers", opt_int(i.containers, span));
    rec.push("containers_running", opt_int(i.containers_running, span));
    rec.push("containers_paused", opt_int(i.containers_paused, span));
    rec.push("containers_stopped", opt_int(i.containers_stopped, span));
    rec.push("images", opt_int(i.images, span));
    rec.push("driver", str_opt(i.driver.as_deref(), span));
    rec.push("operating_system", str_opt(i.operating_system.as_deref(), span));
    rec.push("os_type", str_opt(i.os_type.as_deref(), span));
    rec.push("architecture", str_opt(i.architecture.as_deref(), span));
    rec.push("kernel_version", str_opt(i.kernel_version.as_deref(), span));
    rec.push("ncpu", opt_int(i.ncpu, span));
    rec.push("mem_total", opt_filesize(i.mem_total, span));
    if wide {
        rec.push("docker_root_dir", str_opt(i.docker_root_dir.as_deref(), span));
        rec.push("logging_driver", str_opt(i.logging_driver.as_deref(), span));
        rec.push("cgroup_driver", enum_opt(i.cgroup_driver.as_ref(), span));
        rec.push("cgroup_version", enum_opt(i.cgroup_version.as_ref(), span));
        rec.push("default_runtime", str_opt(i.default_runtime.as_deref(), span));
        rec.push("live_restore", opt_bool(i.live_restore_enabled, span));
        rec.push("debug", opt_bool(i.debug, span));
        rec.push("experimental", opt_bool(i.experimental_build, span));
        rec.push("index_server", str_opt(i.index_server_address.as_deref(), span));
        rec.push("security_options", str_list(i.security_options.as_ref(), span));
        rec.push("warnings", str_list(i.warnings.as_ref(), span));
        rec.push("labels", str_list(i.labels.as_ref(), span));
    }
    Value::record(rec, span)
}

// ===========================================================================
// version
// ===========================================================================

pub struct SystemVersionCommand;

const VERSION_NAME: &str = "nude system version";

impl SimplePluginCommand for SystemVersionCommand {
    type Plugin = NudePlugin;

    fn name(&self) -> &str {
        VERSION_NAME
    }

    fn description(&self) -> &str {
        "Daemon version: engine, API, Go, OS/arch, and components"
    }

    fn signature(&self) -> Signature {
        scaffold::singleton_signature(VERSION_NAME)
    }

    fn run(
        &self,
        plugin: &Self::Plugin,
        _engine: &EngineInterface,
        call: &EvaluatedCall,
        _input: &Value,
    ) -> Result<Value, LabeledError> {
        plugin.block_on_labeled(run_version(plugin, call))
    }

    #[allow(deprecated, reason = "ExperimentalMarker gates an experimental API we opt into")]
    fn get_dynamic_completion(
        &self,
        _plugin: &Self::Plugin,
        _engine: &EngineInterface,
        _call: DynamicCompletionCall,
        arg_type: ArgType,
        _experimental: nu_protocol::engine::ExperimentalMarker,
    ) -> Option<Vec<DynamicSuggestion>> {
        output_completion(arg_type)
    }
}

async fn run_version(plugin: &NudePlugin, call: &EvaluatedCall) -> anyhow::Result<Value> {
    let span = call.head;
    let fmt = scaffold::output_format(call, false)?;
    let v = plugin.docker()?.version().await?;
    Ok(match fmt {
        OutputFormat::Full => full_value(&v, span),
        OutputFormat::Compact => version_record(&v, span, false),
        OutputFormat::Wide => version_record(&v, span, true),
    })
}

/// The `docker version` (server) projection. `wide` adds `min_api_version` and
/// the component breakdown (Engine, containerd, runc, docker-init).
fn version_record(v: &SystemVersion, span: Span, wide: bool) -> Value {
    let mut rec = Record::new();
    rec.push("version", str_opt(v.version.as_deref(), span));
    rec.push("api_version", str_opt(v.api_version.as_deref(), span));
    if wide {
        rec.push("min_api_version", str_opt(v.min_api_version.as_deref(), span));
    }
    rec.push("os", str_opt(v.os.as_deref(), span));
    rec.push("arch", str_opt(v.arch.as_deref(), span));
    rec.push("kernel_version", str_opt(v.kernel_version.as_deref(), span));
    rec.push("go_version", str_opt(v.go_version.as_deref(), span));
    rec.push("git_commit", str_opt(v.git_commit.as_deref(), span));
    rec.push("build_time", opt_rfc3339(v.build_time.as_deref(), span));
    rec.push("experimental", opt_bool(v.experimental, span));
    if wide {
        rec.push("components", components_value(v, span));
    }
    Value::record(rec, span)
}

/// The version components as a list of `{name, version}` records.
fn components_value(v: &SystemVersion, span: Span) -> Value {
    let rows = v
        .components
        .as_ref()
        .map(|cs| {
            cs.iter()
                .map(|c| {
                    let mut r = Record::new();
                    r.push("name", Value::string(&c.name, span));
                    r.push("version", Value::string(&c.version, span));
                    Value::record(r, span)
                })
                .collect()
        })
        .unwrap_or_default();
    Value::list(rows, span)
}
