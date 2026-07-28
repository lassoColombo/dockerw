use bollard::models::ImageSearchResponseItem;
use bollard::query_parameters::SearchImagesOptionsBuilder;
use nu_plugin::{DynamicCompletionCall, EngineInterface, EvaluatedCall, SimplePluginCommand};
use nu_protocol::{
    engine::ArgType, DynamicSuggestion, LabeledError, Record, Signature, Span, Value,
};

use crate::completers;
use crate::helpers::{full_value, opt_bool, opt_int, str_opt};
use crate::output::OutputFormat;
use crate::plugin::NudePlugin;
use crate::scaffold::{self, FilterArg, Shape};

pub struct SearchCommand;

const NAME: &str = "nude search";

const FILTERS: &[FilterArg] = &[
    FilterArg::int("stars", "Only images with at least this many stars"),
    FilterArg {
        flag: "official",
        api_key: "is-official",
        shape: Shape::Switch,
        help: "Only official images (disabled-only: `| where not official`)",
    },
];

impl SimplePluginCommand for SearchCommand {
    type Plugin = NudePlugin;

    fn name(&self) -> &str {
        NAME
    }

    fn description(&self) -> &str {
        "Search Docker Hub for images"
    }

    fn signature(&self) -> Signature {
        scaffold::search_signature(
            NAME,
            "Search term (matched against Docker Hub image names & descriptions)",
            FILTERS,
        )
    }

    fn run(
        &self,
        plugin: &Self::Plugin,
        _engine: &EngineInterface,
        call: &EvaluatedCall,
        _input: &Value,
    ) -> Result<Value, LabeledError> {
        plugin.block_on_labeled(run(plugin, call))
    }

    #[allow(
        deprecated,
        reason = "ExperimentalMarker gates an experimental API we opt into"
    )]
    fn get_dynamic_completion(
        &self,
        _plugin: &Self::Plugin,
        _engine: &EngineInterface,
        _call: DynamicCompletionCall,
        arg_type: ArgType,
        _experimental: nu_protocol::engine::ExperimentalMarker,
    ) -> Option<Vec<DynamicSuggestion>> {
        match arg_type {
            ArgType::Flag(name) if matches!(name.as_ref(), "output" | "o") => {
                Some(completers::from_pairs(OutputFormat::ALL))
            }
            _ => None,
        }
    }
}

async fn run(plugin: &NudePlugin, call: &EvaluatedCall) -> anyhow::Result<Value> {
    let term: String = call.req(0)?;
    let span = call.head;
    let fmt = scaffold::output_format(call, false)?;
    let filters = scaffold::collect_filters(call, FILTERS)?;

    let mut builder = SearchImagesOptionsBuilder::new()
        .term(&term)
        .filters(&filters);
    if let Some(limit) = call.get_flag::<i64>("limit")? {
        builder = builder.limit(limit as i32);
    }
    let results = plugin.docker()?.search_images(builder.build()).await?;

    let rows = results
        .iter()
        .map(|r| match fmt {
            OutputFormat::Full => full_value(r, span),
            _ => compact(r, span),
        })
        .collect();
    Ok(Value::list(rows, span))
}

fn compact(r: &ImageSearchResponseItem, span: Span) -> Value {
    let mut rec = Record::new();
    rec.push("name", str_opt(r.name.as_deref(), span));
    rec.push("description", str_opt(r.description.as_deref(), span));
    rec.push("stars", opt_int(r.star_count, span));
    rec.push("official", opt_bool(r.is_official, span));
    Value::record(rec, span)
}
