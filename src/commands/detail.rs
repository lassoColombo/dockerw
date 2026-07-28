use bollard::models::{
    ChangeType, ContainerTopResponse, FilesystemChange, ImageHistoryResponseItem,
};
use nu_plugin::{DynamicCompletionCall, EngineInterface, EvaluatedCall, SimplePluginCommand};
use nu_protocol::{
    engine::ArgType, DynamicSuggestion, LabeledError, Record, Signature, Span, Value,
};

use crate::commands::{container, image};
use crate::completers;
use crate::helpers::{epoch_date, full_value, short_id, str_list, str_opt};
use crate::output::OutputFormat;
use crate::plugin::NudePlugin;
use crate::scaffold;

pub struct ContainerDiffCommand;

const DIFF_NAME: &str = "nude container diff";

impl SimplePluginCommand for ContainerDiffCommand {
    type Plugin = NudePlugin;

    fn name(&self) -> &str {
        DIFF_NAME
    }

    fn description(&self) -> &str {
        "Filesystem changes to a container since it started"
    }

    fn signature(&self) -> Signature {
        scaffold::subcommand_signature(DIFF_NAME, "Container name or ID")
    }

    fn run(
        &self,
        plugin: &Self::Plugin,
        _engine: &EngineInterface,
        call: &EvaluatedCall,
        _input: &Value,
    ) -> Result<Value, LabeledError> {
        plugin.block_on_labeled(run_diff(plugin, call))
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
        completers::ref_and_output(plugin, arg_type, container::complete_names)
    }
}

async fn run_diff(plugin: &NudePlugin, call: &EvaluatedCall) -> anyhow::Result<Value> {
    let name: String = call.req(0)?;
    let span = call.head;
    let fmt = scaffold::output_format(call, false)?;
    // `None` = no changes → an empty list, not an error.
    let changes = plugin
        .docker()?
        .container_changes(&name)
        .await?
        .unwrap_or_default();
    let rows = changes
        .iter()
        .map(|c| match fmt {
            OutputFormat::Full => full_value(c, span),
            _ => diff_row(c, span),
        })
        .collect();
    Ok(Value::list(rows, span))
}

fn diff_row(c: &FilesystemChange, span: Span) -> Value {
    let mut rec = Record::new();
    rec.push("path", str_opt(Some(c.path.as_str()), span));
    rec.push("kind", change_kind(c.kind, span));
    Value::record(rec, span)
}

fn change_kind(kind: ChangeType, span: Span) -> Value {
    let s = match kind {
        ChangeType::_0 => "modified",
        ChangeType::_1 => "added",
        ChangeType::_2 => "deleted",
    };
    Value::string(s, span)
}

pub struct ContainerTopCommand;

const TOP_NAME: &str = "nude container top";

impl SimplePluginCommand for ContainerTopCommand {
    type Plugin = NudePlugin;

    fn name(&self) -> &str {
        TOP_NAME
    }

    fn description(&self) -> &str {
        "Running processes in a container (like `docker top`)"
    }

    fn signature(&self) -> Signature {
        scaffold::subcommand_signature(TOP_NAME, "Container name or ID (must be running)")
    }

    fn run(
        &self,
        plugin: &Self::Plugin,
        _engine: &EngineInterface,
        call: &EvaluatedCall,
        _input: &Value,
    ) -> Result<Value, LabeledError> {
        plugin.block_on_labeled(run_top(plugin, call))
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
        completers::ref_and_output(plugin, arg_type, container::complete_names)
    }
}

async fn run_top(plugin: &NudePlugin, call: &EvaluatedCall) -> anyhow::Result<Value> {
    let name: String = call.req(0)?;
    let span = call.head;
    let fmt = scaffold::output_format(call, false)?;
    let top = plugin.docker()?.top_processes(&name, None).await?;
    Ok(match fmt {
        OutputFormat::Full => full_value(&top, span),
        _ => top_table(&top, span),
    })
}

fn top_table(top: &ContainerTopResponse, span: Span) -> Value {
    let titles: Vec<String> = top
        .titles
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|t| t.to_lowercase())
        .collect();
    let rows = top
        .processes
        .as_ref()
        .map(|procs| {
            procs
                .iter()
                .map(|process| {
                    let mut rec = Record::new();
                    for (i, title) in titles.iter().enumerate() {
                        rec.push(
                            title.clone(),
                            str_opt(process.get(i).map(String::as_str), span),
                        );
                    }
                    Value::record(rec, span)
                })
                .collect()
        })
        .unwrap_or_default();
    Value::list(rows, span)
}

pub struct ImageHistoryCommand;

const HISTORY_NAME: &str = "nude image history";

impl SimplePluginCommand for ImageHistoryCommand {
    type Plugin = NudePlugin;

    fn name(&self) -> &str {
        HISTORY_NAME
    }

    fn description(&self) -> &str {
        "Layer history of an image (like `docker history`)"
    }

    fn signature(&self) -> Signature {
        scaffold::subcommand_signature(HISTORY_NAME, "Image reference (repo:tag) or ID")
    }

    fn run(
        &self,
        plugin: &Self::Plugin,
        _engine: &EngineInterface,
        call: &EvaluatedCall,
        _input: &Value,
    ) -> Result<Value, LabeledError> {
        plugin.block_on_labeled(run_history(plugin, call))
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
        completers::ref_and_output(plugin, arg_type, image::complete_refs)
    }
}

async fn run_history(plugin: &NudePlugin, call: &EvaluatedCall) -> anyhow::Result<Value> {
    let name: String = call.req(0)?;
    let span = call.head;
    let fmt = scaffold::output_format(call, false)?;
    let history = plugin.docker()?.image_history(&name).await?;
    let rows = history
        .iter()
        .map(|h| match fmt {
            OutputFormat::Full => full_value(h, span),
            OutputFormat::Wide => history_row(h, span, true),
            OutputFormat::Compact => history_row(h, span, false),
        })
        .collect();
    Ok(Value::list(rows, span))
}

fn history_row(h: &ImageHistoryResponseItem, span: Span, wide: bool) -> Value {
    let mut rec = Record::new();
    rec.push("id", history_id(&h.id, span));
    rec.push("created", epoch_date(h.created, span));
    rec.push("created_by", str_opt(Some(h.created_by.as_str()), span));
    rec.push("size", Value::filesize(h.size, span));
    rec.push("comment", str_opt(Some(h.comment.as_str()), span));
    if wide {
        rec.push("tags", str_list(Some(&h.tags), span));
    }
    Value::record(rec, span)
}

fn history_id(id: &str, span: Span) -> Value {
    let id = id.strip_prefix("sha256:").unwrap_or(id);
    if id.is_empty() || id == "<missing>" {
        Value::nothing(span)
    } else {
        Value::string(short_id(id), span)
    }
}
