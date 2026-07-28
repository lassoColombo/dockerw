use bollard::models::{ContainerInspectResponse, ContainerSummary};
use bollard::query_parameters::ListContainersOptionsBuilder;
use futures::future::join_all;
use nu_plugin::{DynamicCompletionCall, EngineInterface, EvaluatedCall, SimplePluginCommand};
use nu_protocol::{
    engine::ArgType, DynamicSuggestion, LabeledError, Record, Signature, Span, Value,
};

use crate::completers;
use crate::decorators::Decorators;
use crate::helpers::{
    clean_name, enum_opt, full_value, opt_bool, opt_epoch, opt_int, opt_rfc3339, short_id,
    str_list, str_opt,
};
use crate::output::OutputFormat;
use crate::plugin::NudePlugin;
use crate::scaffold::{self, CommandSpec, FilterArg};

pub struct ContainerLsCommand;
pub struct ContainerInspectCommand;

const LS: &str = "nude container ls";
const INSPECT: &str = "nude container inspect";

const FILTERS: &[FilterArg] = &[
    FilterArg::string(
        "status",
        "State: created|restarting|running|removing|paused|exited|dead",
    ),
    FilterArg::string("health", "Health: starting|healthy|unhealthy|none"),
    FilterArg::int("exited", "Exit code (matches stopped containers)"),
    FilterArg::string(
        "ancestor",
        "Created from this image (name[:tag], id, or digest)",
    ),
    FilterArg::string("before", "Created before this container (name or id)"),
    FilterArg::string("since", "Created since this container (name or id)"),
    FilterArg::string("name", "Name substring (use `inspect` for an exact lookup)"),
    FilterArg::string("id", "Container id prefix"),
    FilterArg::string("network", "Attached to this network (name or id)"),
    FilterArg::string("volume", "Uses this volume (name or mount destination)"),
    FilterArg::string(
        "publish",
        "Publishes this port (`port[/proto]` or `start-end[/proto]`)",
    ),
    FilterArg::string(
        "expose",
        "Exposes this port (`port[/proto]` or `start-end[/proto]`)",
    ),
    FilterArg::labels(),
    FilterArg::switch("is-task", "Only swarm service tasks"),
];

impl SimplePluginCommand for ContainerLsCommand {
    type Plugin = NudePlugin;

    fn name(&self) -> &str {
        LS
    }

    fn description(&self) -> &str {
        "List containers"
    }

    fn signature(&self) -> Signature {
        scaffold::list_signature(&CommandSpec {
            cmd: LS,
            noun: "container",
            all_help: Some("Include stopped containers"),
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

    #[allow(
        deprecated,
        reason = "ExperimentalMarker gates an experimental API we opt into"
    )]
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
                // Closed enums → static lists.
                "output" | "o" => Some(completers::from_pairs(OutputFormat::ALL)),
                "status" => Some(completers::from_pairs(STATUS)),
                "health" => Some(completers::from_pairs(HEALTH)),
                // Container references → live names/ids.
                "name" | "before" | "since" => complete_names(plugin),
                "id" => complete_ids(plugin),
                // Cross-references → the owning resource's completer.
                "ancestor" => crate::commands::image::complete_refs(plugin),
                "network" => crate::commands::network::complete_names(plugin),
                "volume" => crate::commands::volume::complete_names(plugin),
                "labels" => completers::complete_labels(
                    &call,
                    all_containers(plugin)?
                        .iter()
                        .filter_map(|c| c.labels.as_ref()),
                ),
                _ => None,
            },
            _ => None,
        }
    }
}

impl SimplePluginCommand for ContainerInspectCommand {
    type Plugin = NudePlugin;

    fn name(&self) -> &str {
        INSPECT
    }

    fn description(&self) -> &str {
        "Inspect a container by name/ID"
    }

    fn signature(&self) -> Signature {
        scaffold::inspect_signature(INSPECT, "Container name or ID", Some("container"))
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

    #[allow(
        deprecated,
        reason = "ExperimentalMarker gates an experimental API we opt into"
    )]
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

const STATUS: &[(&str, &str)] = &[
    ("created", "Created but not started"),
    ("restarting", "Restarting"),
    ("running", "Running"),
    ("removing", "Being removed"),
    ("paused", "Paused"),
    ("exited", "Stopped after running"),
    ("dead", "Dead - the daemon could not remove it"),
];

const HEALTH: &[(&str, &str)] = &[
    ("starting", "Healthcheck grace period"),
    ("healthy", "Passing its healthcheck"),
    ("unhealthy", "Failing its healthcheck"),
    ("none", "No healthcheck configured"),
];

fn all_containers(plugin: &NudePlugin) -> Option<Vec<ContainerSummary>> {
    let docker = plugin.docker().ok()?;
    let opts = ListContainersOptionsBuilder::default().all(true).build();
    plugin.rt.block_on(docker.list_containers(Some(opts))).ok()
}

pub(crate) fn complete_names(plugin: &NudePlugin) -> Option<Vec<DynamicSuggestion>> {
    Some(
        all_containers(plugin)?
            .iter()
            .filter_map(|c| {
                Some(DynamicSuggestion {
                    value: clean_name(c.names.as_ref())?,
                    description: c.status.clone(),
                    ..Default::default()
                })
            })
            .collect(),
    )
}

fn complete_ids(plugin: &NudePlugin) -> Option<Vec<DynamicSuggestion>> {
    Some(
        all_containers(plugin)?
            .iter()
            .filter_map(|c| {
                Some(DynamicSuggestion {
                    value: c.id.clone()?,
                    description: clean_name(c.names.as_ref()),
                    ..Default::default()
                })
            })
            .collect(),
    )
}

async fn run_list(plugin: &NudePlugin, call: &EvaluatedCall) -> anyhow::Result<Value> {
    let all: bool = call.has_flag("all")?;
    let decorators = Decorators::from_call(call)?;
    let span = call.head;
    let fmt = scaffold::output_format(call, false)?;

    let docker = plugin.docker()?;
    let filters = scaffold::collect_filters(call, FILTERS)?;
    let opts = ListContainersOptionsBuilder::default()
        .all(all)
        .filters(&filters)
        .build();
    let summaries = docker.list_containers(Some(opts)).await?;
    match fmt {
        OutputFormat::Compact => Ok(Value::list(
            summaries
                .iter()
                .map(|c| compact(c, span, decorators))
                .collect(),
            span,
        )),
        _ => {
            let ids: Vec<String> = summaries.into_iter().filter_map(|c| c.id).collect();
            let inspected = join_all(ids.iter().map(|id| docker.inspect_container(id, None))).await;
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

async fn run_inspect(plugin: &NudePlugin, call: &EvaluatedCall) -> anyhow::Result<Value> {
    let name: String = call.req(0)?;
    let decorators = Decorators::from_call(call)?;
    let span = call.head;
    let fmt = scaffold::output_format(call, true)?;

    let resp = plugin.docker()?.inspect_container(&name, None).await?;
    Ok(match fmt {
        OutputFormat::Full => full_value(&resp, span),
        _ => wide(&resp, span, decorators),
    })
}

fn compact(c: &ContainerSummary, span: Span, decorators: Decorators) -> Value {
    let status = c.status.as_deref();
    let mut rec = Record::new();
    rec.push(
        "name",
        str_opt(clean_name(c.names.as_ref()).as_deref(), span),
    );
    rec.push("image", str_opt(c.image.as_deref(), span));
    rec.push("state", enum_opt(c.state.as_ref(), span));
    rec.push("health", str_opt(status_health(status), span));
    rec.push("exit_code", opt_int(status_exit_code(status), span));
    rec.push("created", opt_epoch(c.created, span));
    decorators.apply_labels(&mut rec, c.labels.as_ref(), span);
    Value::record(rec, span)
}

fn status_health(status: Option<&str>) -> Option<&'static str> {
    let s = status?;
    if s.contains("(healthy)") {
        Some("healthy")
    } else if s.contains("(unhealthy)") {
        Some("unhealthy")
    } else if s.contains("(health: starting)") {
        Some("starting")
    } else {
        None
    }
}

fn status_exit_code(status: Option<&str>) -> Option<i64> {
    let s = status?;
    let rest = s
        .strip_prefix("Exited (")
        .or_else(|| s.strip_prefix("Restarting ("))?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

fn wide(r: &ContainerInspectResponse, span: Span, decorators: Decorators) -> Value {
    let config = r.config.as_ref();
    let state = r.state.as_ref();

    let mut rec = Record::new();
    rec.push(
        "name",
        str_opt(r.name.as_deref().map(|n| n.trim_start_matches('/')), span),
    );
    rec.push(
        "id",
        str_opt(r.id.as_deref().map(short_id).as_deref(), span),
    );
    rec.push(
        "image",
        str_opt(config.and_then(|c| c.image.as_deref()), span),
    );
    rec.push(
        "state",
        enum_opt(state.and_then(|s| s.status.as_ref()), span),
    );
    rec.push(
        "health",
        enum_opt(
            state
                .and_then(|s| s.health.as_ref())
                .and_then(|h| h.status.as_ref()),
            span,
        ),
    );
    let running = state.and_then(|s| s.running).unwrap_or(false);
    rec.push(
        "exit_code",
        opt_int(
            (!running)
                .then(|| state.and_then(|s| s.exit_code))
                .flatten(),
            span,
        ),
    );
    rec.push(
        "command",
        str_list(config.and_then(|c| c.cmd.as_ref()), span),
    );
    rec.push("created", opt_rfc3339(r.created.as_deref(), span));
    rec.push(
        "started",
        opt_rfc3339(state.and_then(|s| s.started_at.as_deref()), span),
    );
    rec.push(
        "finished",
        opt_rfc3339(state.and_then(|s| s.finished_at.as_deref()), span),
    );
    rec.push("restarts", opt_int(r.restart_count, span));
    rec.push("ports", ports_value(r, span));
    rec.push("ip", ip_value(r, span));
    rec.push("networks", networks_value(r, span));
    rec.push("mounts", mounts_value(r, span));
    decorators.apply_labels(&mut rec, config.and_then(|c| c.labels.as_ref()), span);
    Value::record(rec, span)
}

fn port_int(s: &str, span: Span) -> Value {
    match s.parse::<i64>() {
        Ok(n) => Value::int(n, span),
        Err(_) => Value::string(s, span),
    }
}

fn ports_value(r: &ContainerInspectResponse, span: Span) -> Value {
    let Some(ports) = r.network_settings.as_ref().and_then(|ns| ns.ports.as_ref()) else {
        return Value::list(vec![], span);
    };
    let mut rows = Vec::new();
    for (key, bindings) in ports {
        let (port, proto) = key.split_once('/').unwrap_or((key.as_str(), ""));
        match bindings {
            Some(bs) if !bs.is_empty() => {
                for b in bs {
                    let mut rec = Record::new();
                    rec.push("container_port", port_int(port, span));
                    rec.push("proto", Value::string(proto, span));
                    rec.push("host_ip", str_opt(b.host_ip.as_deref(), span));
                    rec.push(
                        "host_port",
                        match b.host_port.as_deref() {
                            Some(p) if !p.is_empty() => port_int(p, span),
                            _ => Value::nothing(span),
                        },
                    );
                    rows.push(Value::record(rec, span));
                }
            }
            _ => {
                let mut rec = Record::new();
                rec.push("container_port", port_int(port, span));
                rec.push("proto", Value::string(proto, span));
                rec.push("host_ip", Value::nothing(span));
                rec.push("host_port", Value::nothing(span));
                rows.push(Value::record(rec, span));
            }
        }
    }
    Value::list(rows, span)
}

fn ip_value(r: &ContainerInspectResponse, span: Span) -> Value {
    let ip = r
        .network_settings
        .as_ref()
        .and_then(|ns| ns.networks.as_ref())
        .and_then(|nets| {
            nets.values()
                .filter_map(|e| e.ip_address.as_deref())
                .find(|s| !s.is_empty())
        });
    str_opt(ip, span)
}

fn networks_value(r: &ContainerInspectResponse, span: Span) -> Value {
    let names = r
        .network_settings
        .as_ref()
        .and_then(|ns| ns.networks.as_ref())
        .map(|nets| {
            nets.keys()
                .map(|k| Value::string(k, span))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Value::list(names, span)
}

fn mounts_value(r: &ContainerInspectResponse, span: Span) -> Value {
    let Some(mounts) = r.mounts.as_ref() else {
        return Value::list(vec![], span);
    };
    let rows = mounts
        .iter()
        .map(|m| {
            let mut rec = Record::new();
            rec.push("type", enum_opt(m.typ.as_ref(), span));
            rec.push("source", str_opt(m.source.as_deref(), span));
            rec.push("destination", str_opt(m.destination.as_deref(), span));
            rec.push("mode", str_opt(m.mode.as_deref(), span));
            rec.push("rw", opt_bool(m.rw, span));
            Value::record(rec, span)
        })
        .collect();
    Value::list(rows, span)
}

#[cfg(test)]
mod tests {
    use super::{status_exit_code, status_health};

    #[test]
    fn health_is_parsed_from_the_status_line() {
        assert_eq!(
            status_health(Some("Up 12 hours (healthy)")),
            Some("healthy")
        );
        assert_eq!(
            status_health(Some("Up 2 minutes (unhealthy)")),
            Some("unhealthy")
        );
        assert_eq!(
            status_health(Some("Up Less than a second (health: starting)")),
            Some("starting")
        );
        assert_eq!(status_health(Some("Up 12 hours")), None);
        assert_eq!(status_health(Some("Exited (0) 47 hours ago")), None);
        assert_eq!(status_health(None), None);
    }

    #[test]
    fn exit_code_is_parsed_from_the_status_line() {
        assert_eq!(status_exit_code(Some("Exited (0) 47 hours ago")), Some(0));
        assert_eq!(status_exit_code(Some("Exited (137) 3 days ago")), Some(137));
        assert_eq!(
            status_exit_code(Some("Restarting (1) 5 seconds ago")),
            Some(1)
        );
        assert_eq!(status_exit_code(Some("Up 12 hours (healthy)")), None);
        assert_eq!(status_exit_code(Some("Created")), None);
        assert_eq!(status_exit_code(None), None);
    }
}
