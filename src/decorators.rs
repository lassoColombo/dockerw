use std::collections::HashMap;

use nu_plugin::EvaluatedCall;
use nu_protocol::{Record, Span};

use crate::helpers::str_map;

#[derive(Debug, Clone, Copy, Default)]
pub struct Decorators {
    show_labels: bool,
}

impl Decorators {
    pub fn from_call(call: &EvaluatedCall) -> anyhow::Result<Self> {
        Ok(Self {
            show_labels: call.has_flag("show-labels")?,
        })
    }

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
