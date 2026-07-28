//! Optional enrichment columns toggled by `--show-*` flags.
//!
//! Every resource command shares this so the flag surface stays uniform: parse
//! the flags once with [`Decorators::from_call`], then let each formatter hand
//! its own label map to [`Decorators::apply_labels`]. Labels are Docker's only
//! enrichment axis today; a new one gets a field here plus an `apply_*` method,
//! and every resource picks it up for free.

use std::collections::HashMap;

use nu_plugin::EvaluatedCall;
use nu_protocol::{Record, Span};

use crate::helpers::str_map;

/// Which enrichment columns the caller asked for.
#[derive(Debug, Clone, Copy, Default)]
pub struct Decorators {
    show_labels: bool,
}

impl Decorators {
    /// Parse the decorator flags off a command call.
    pub fn from_call(call: &EvaluatedCall) -> anyhow::Result<Self> {
        Ok(Self {
            show_labels: call.has_flag("show-labels")?,
        })
    }

    /// Append a `labels` column built from `labels` when `--show-labels` is set;
    /// a no-op otherwise, so formatters can call it unconditionally.
    pub fn apply_labels(
        &self,
        rec: &mut Record,
        labels: Option<&HashMap<String, String>>,
        span: Span,
    ) {
        if self.show_labels {
            rec.push("labels", str_map(labels, span));
        }
    }
}
