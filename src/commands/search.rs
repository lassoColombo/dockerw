//! `nude image search` — search Docker Hub for images.
//!
//! The one **outward-facing** command: unlike every other `nude` command, this
//! doesn't introspect the local daemon — it queries **Docker Hub** (the daemon
//! proxies `GET /images/search` to the registry). Still strictly read-only.
//!
//! A different shape from list+inspect: the search term is a **required**
//! positional, there is no inspect (a hit is just registry metadata), and the
//! result data isn't reproducible (it changes with Hub, and needs network). The
//! output *shape* stays typed, though — which is the whole point: `star_count`
//! becomes an int and `is_official` a bool, so
//! `nude image search nginx | where official | sort-by stars -r` just works.
//!
//! Output per format (the response is flat — five primitive fields):
//! - compact / wide : `{name, description, stars, official}` (mirrors `docker search`)
//! - full           : the raw item, which additionally carries the now-dead
//!   `is_automated` (Docker Hub removed automated builds in 2023).

use bollard::models::ImageSearchResponseItem;
use bollard::query_parameters::SearchImagesOptionsBuilder;
use nu_plugin::{DynamicCompletionCall, EngineInterface, EvaluatedCall, SimplePluginCommand};
use nu_protocol::{DynamicSuggestion, LabeledError, Record, Signature, Span, Value, engine::ArgType};

use crate::completers;
use crate::helpers::{full_value, opt_bool, opt_int, str_opt};
use crate::output::OutputFormat;
use crate::plugin::NudePlugin;
use crate::scaffold::{self, FilterArg, Shape};

pub struct ImageSearchCommand;

const NAME: &str = "nude image search";

/// Docker `/images/search` filters. `official` maps to the API's `is-official`
/// key (like `labels` → `label`). `is-automated` is intentionally omitted: Docker
/// Hub removed automated builds in 2023, so the filter matches nothing useful.
const FILTERS: &[FilterArg] = &[
    FilterArg::int("stars", "Only images with at least this many stars"),
    FilterArg {
        flag: "official",
        api_key: "is-official",
        shape: Shape::Switch,
        help: "Only official images (disabled-only: `| where not official`)",
    },
];

impl SimplePluginCommand for ImageSearchCommand {
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

    /// Only `-o` is completable — the term is free text, and `--stars`/`--official`
    /// aren't enumerable.
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

    let mut builder = SearchImagesOptionsBuilder::new().term(&term).filters(&filters);
    // Omitted → the daemon's default of 25, matching `docker search`.
    if let Some(limit) = call.get_flag::<i64>("limit")? {
        builder = builder.limit(limit as i32);
    }
    // Results come back in Hub's relevance order; we keep it (the user sorts).
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

/// One search hit. Every field is a flat primitive, so compact captures the whole
/// useful row; `full` additionally carries the (now-dead) `is_automated`.
fn compact(r: &ImageSearchResponseItem, span: Span) -> Value {
    let mut rec = Record::new();
    rec.push("name", str_opt(r.name.as_deref(), span));
    rec.push("description", str_opt(r.description.as_deref(), span));
    rec.push("stars", opt_int(r.star_count, span));
    rec.push("official", opt_bool(r.is_official, span));
    Value::record(rec, span)
}
