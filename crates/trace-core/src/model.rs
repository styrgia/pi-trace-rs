use crate::{bounded, views::*};
use serde_json::Value;

#[derive(Clone, Default)]
pub(crate) enum CompletionReason {
    #[default]
    Open,
    Recorded {
        failed: bool,
    },
    Stop(Option<String>),
}

#[derive(Clone, Default)]
pub(crate) struct CompletionEvidence {
    pub reason: CompletionReason,
    pub missing_prerequisite: bool,
}
impl CompletionEvidence {
    pub fn outcome(&self) -> Outcome {
        if self.missing_prerequisite {
            return Outcome::Incomplete;
        }
        match &self.reason {
            CompletionReason::Open => Outcome::Incomplete,
            CompletionReason::Recorded { failed: true } => Outcome::Error,
            CompletionReason::Recorded { failed: false } => Outcome::Success,
            CompletionReason::Stop(reason) => match reason.as_deref() {
                Some("stop" | "length" | "toolUse") => Outcome::Success,
                Some("error" | "aborted") => Outcome::Error,
                _ => Outcome::Unknown,
            },
        }
    }
}

#[derive(Clone)]
pub(crate) struct Node {
    pub id: String,
    pub span_id: String,
    pub span_usage: SpanUsage,
    pub contribution_id: Option<String>,
    pub kind: NodeKind,
    pub label: String,
    pub label_truncated: bool,
    pub parent: Option<usize>,
    pub relationship: RelationshipState,
    pub depth: usize,
    pub children: Vec<usize>,
    pub sources: Vec<SourceRef>,
    pub diagnostic_ids: Vec<String>,
    pub metadata: Metadata,
    pub start: Option<f64>,
    pub end: Option<f64>,
    pub ended: bool,
    pub completion: CompletionEvidence,
    pub duration: Measurement,
    pub headers: Measurement,
    pub outcome: Outcome,
    pub usage: Usage,
    pub summary_usage: Option<Usage>,
    pub generation_usage: Option<crate::accounting::RecordedUsage>,
    pub summary_evidence: Option<crate::accounting::RecordedUsage>,
    pub stop_reason: Option<String>,
    pub input: ContentField,
    pub output: ContentField,
    pub child_ids: Vec<String>,
    pub context: ContextState,
    pub tool_inspection: ToolState,
}
impl Node {
    pub fn new(
        index: usize,
        kind: NodeKind,
        parent: Option<usize>,
        source: SourceRef,
        event: &Value,
    ) -> Self {
        let generic = match kind {
            NodeKind::Session => "Session",
            NodeKind::Interaction => "Interaction",
            NodeKind::Turn => "Turn",
            NodeKind::Generation => "Generation",
            NodeKind::Tool => "Tool",
            NodeKind::Message => "Message",
        };
        let tool_name = event
            .get("toolName")
            .and_then(Value::as_str)
            .map(|s| bounded(s, DISPLAY_BYTES).0);
        let label = if kind == NodeKind::Tool {
            event
                .get("toolName")
                .and_then(Value::as_str)
                .unwrap_or(generic)
                .to_owned()
        } else {
            format!("{generic} {}", index + 1)
        };
        let (label, label_truncated) = bounded(&label, DISPLAY_BYTES);
        Self {
            id: format!("n{}", index + 1),
            span_id: String::new(),
            span_usage: crate::accounting::empty_span_usage(),
            contribution_id: None,
            kind,
            label,
            label_truncated,
            parent,
            relationship: if parent.is_some() {
                RelationshipState::Linked
            } else {
                RelationshipState::Root
            },
            depth: 0,
            children: Vec::new(),
            sources: vec![source],
            diagnostic_ids: Vec::new(),
            metadata: Metadata {
                session_id: string(event, "sessionId"),
                model: string(event, "model"),
                tool_name,
                turn_index: event.get("turnIndex").and_then(Value::as_u64),
                step_index: event.get("stepIndex").and_then(Value::as_u64),
            },
            start: number(event.get("ts")),
            end: None,
            ended: false,
            completion: CompletionEvidence::default(),
            duration: unknown("elapsed duration"),
            headers: unknown("request to HTTP headers"),
            outcome: Outcome::Incomplete,
            usage: empty_usage(0),
            summary_usage: None,
            generation_usage: None,
            summary_evidence: None,
            stop_reason: None,
            input: unavailable(),
            output: unavailable(),
            child_ids: Vec::new(),
            context: ContextState::Missing,
            tool_inspection: ToolState::Missing,
        }
    }
    pub fn row(
        &self,
        nodes: &[Node],
        timeline: &Option<Timeline>,
        matched: bool,
        context: bool,
    ) -> Observation {
        let bar = match (self.start, self.duration.value_ms, timeline) {
            (Some(start), Some(duration), Some(domain))
                if self.duration.state != MeasurementState::Invalid
                    && self.end.is_none_or(|end| end >= start)
                    && start >= domain.start_ms =>
            {
                Some(Bar {
                    offset_ms: start - domain.start_ms,
                    duration_ms: duration,
                })
            }
            _ => None,
        };
        Observation {
            id: self.id.clone(),
            span_id: self.span_id.clone(),
            span_usage: self.span_usage.clone(),
            kind: self.kind,
            label: self.label.clone(),
            label_truncated: self.label_truncated,
            parent_id: self.parent.map(|i| nodes[i].id.clone()),
            depth: self.depth,
            has_children: !self.children.is_empty(),
            r#match: matched,
            context,
            outcome: self.outcome,
            duration: self.duration.clone(),
            bar,
            source: self.sources[0].clone(),
        }
    }
}
pub(crate) fn string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(|s| bounded(s, DISPLAY_BYTES).0)
}
pub(crate) fn number(value: Option<&Value>) -> Option<f64> {
    value.and_then(Value::as_f64).filter(|n| n.is_finite())
}
pub(crate) fn unknown(meaning: &str) -> Measurement {
    Measurement {
        state: MeasurementState::Unknown,
        value_ms: None,
        meaning: meaning.into(),
    }
}
pub(crate) fn recorded(value: Option<&Value>, meaning: &str) -> Measurement {
    match number(value) {
        Some(n) if n >= 0.0 => Measurement {
            state: MeasurementState::Recorded,
            value_ms: Some(n),
            meaning: meaning.into(),
        },
        Some(_) => Measurement {
            state: MeasurementState::Invalid,
            value_ms: None,
            meaning: meaning.into(),
        },
        None => unknown(meaning),
    }
}
pub(crate) fn unavailable() -> ContentField {
    ContentField {
        state: ContentState::Unavailable,
        text: None,
        truncated: false,
        limit_bytes: CONTENT_BYTES,
    }
}
pub(crate) fn preview(value: Option<&Value>) -> ContentField {
    match value {
        Some(v) if !v.is_null() => {
            let raw = v
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| v.to_string());
            let (text, truncated) = bounded(&raw, CONTENT_BYTES);
            ContentField {
                state: ContentState::Available,
                text: Some(text),
                truncated,
                limit_bytes: CONTENT_BYTES,
            }
        }
        _ => unavailable(),
    }
}
pub(crate) fn empty_usage(total: usize) -> Usage {
    Usage {
        state: UsageState::Unknown,
        input_tokens: None,
        output_tokens: None,
        cache_read_tokens: None,
        cache_write_tokens: None,
        total_tokens: None,
        cost: None,
        observed_generations: 0,
        total_generations: total,
        fallback_source: None,
    }
}
pub(crate) fn parse_usage(value: Option<&Value>) -> Usage {
    let Some(value) = value.filter(|v| v.is_object()) else {
        return empty_usage(1);
    };
    let input = value
        .get("inputTokens")
        .or_else(|| value.get("input"))
        .and_then(Value::as_u64);
    let output = value
        .get("outputTokens")
        .or_else(|| value.get("output"))
        .and_then(Value::as_u64);
    let cache_read = value.get("cacheRead").and_then(Value::as_u64);
    let cache_write = value.get("cacheWrite").and_then(Value::as_u64);
    let total = value
        .get("totalTokens")
        .and_then(Value::as_u64)
        .or_else(|| {
            input
                .zip(output)
                .zip(cache_read)
                .zip(cache_write)
                .and_then(|(((i, o), r), w)| i.checked_add(o)?.checked_add(r)?.checked_add(w))
        });

    let cost = number(value.get("cost").and_then(|v| {
        if v.is_object() {
            v.get("total")
        } else {
            Some(v)
        }
    }))
    .filter(|n| *n >= 0.0);
    let observed = input.is_some()
        || output.is_some()
        || cache_read.is_some()
        || cache_write.is_some()
        || total.is_some()
        || cost.is_some();
    Usage {
        state: if !observed {
            UsageState::Unknown
        } else if input.is_some()
            && output.is_some()
            && cache_read.is_some()
            && cache_write.is_some()
            && total.is_some()
        {
            UsageState::Complete
        } else {
            UsageState::Partial
        },
        input_tokens: input,
        output_tokens: output,
        cache_read_tokens: cache_read,
        cache_write_tokens: cache_write,
        total_tokens: total,
        cost,
        observed_generations: usize::from(observed),
        total_generations: 1,
        fallback_source: None,
    }
}
pub(crate) fn aggregate(usages: &[&Usage]) -> Usage {
    let mut result = empty_usage(usages.iter().map(|u| u.total_generations).sum());
    result.observed_generations = usages.iter().map(|u| u.observed_generations).sum();
    fn sum(items: &[&Usage], field: impl Fn(&Usage) -> Option<u64>) -> Option<u64> {
        items
            .iter()
            .filter_map(|u| field(u))
            .try_fold(None, |a, n| a.unwrap_or(0_u64).checked_add(n).map(Some))
            .flatten()
    }
    result.input_tokens = sum(usages, |u| u.input_tokens);
    result.output_tokens = sum(usages, |u| u.output_tokens);
    result.cache_read_tokens = sum(usages, |u| u.cache_read_tokens);
    result.cache_write_tokens = sum(usages, |u| u.cache_write_tokens);
    result.total_tokens = sum(usages, |u| u.total_tokens);
    let costs: Vec<_> = usages.iter().filter_map(|u| u.cost).collect();
    result.cost = if !usages.is_empty() && costs.len() == usages.len() {
        Some(costs.iter().sum::<f64>()).filter(|n| n.is_finite())
    } else {
        None
    };
    let known: Vec<_> = usages
        .iter()
        .filter(|u| u.state != UsageState::Unknown)
        .collect();
    result.state = if known.is_empty() {
        UsageState::Unknown
    } else if known.len() < usages.len() || known.iter().any(|u| u.state == UsageState::Partial) {
        UsageState::Partial
    } else if known.iter().any(|u| u.state == UsageState::Fallback) {
        UsageState::Fallback
    } else {
        UsageState::Complete
    };
    if usages.iter().any(|u| u.fallback_source.is_some()) {
        result.fallback_source = Some(FallbackSource::TurnSummary);
    }
    result
}

#[derive(Clone)]
pub(crate) enum ContextState {
    Missing,
    Retained(Box<ContextInspection>),
    Limited {
        coverage: Coverage,
        shape: RequestShape,
        field: Option<RequestField>,
        source_index: usize,
    },
}
impl ContextState {
    pub fn retained_bytes(&self) -> usize {
        match self {
            Self::Retained(value) => crate::inspection::bytes(value),
            _ => 0,
        }
    }
    pub fn view(&self, sources: &[SourceRef], usage: &Usage) -> ContextInspection {
        let mut context = match self {
            Self::Missing => crate::inspection::empty_context(),
            Self::Retained(value) => *value.clone(),
            Self::Limited {
                coverage,
                shape,
                field,
                source_index,
            } => crate::inspection::limited_context(
                *coverage,
                *shape,
                *field,
                sources.get(*source_index).cloned(),
            ),
        };
        if usage.fallback_source.is_none() {
            context.context_input_tokens = usage
                .input_tokens
                .zip(usage.cache_read_tokens)
                .zip(usage.cache_write_tokens)
                .and_then(|((input, read), write)| input.checked_add(read)?.checked_add(write));
        }
        context
    }
}
#[derive(Clone)]
pub(crate) enum ToolState {
    Missing,
    Retained(Box<ToolInspection>),
    Limited {
        argument_source: usize,
        result_source: Option<usize>,
        argument_local: bool,
        result_local: bool,
    },
}
impl ToolState {
    pub fn retained_bytes(&self) -> usize {
        match self {
            Self::Retained(value) => crate::inspection::bytes(value),
            _ => 0,
        }
    }
    pub fn view(&self, node: &Node) -> ToolInspection {
        match self {
            Self::Retained(value) => *value.clone(),
            _ => {
                let (start, end, arg_local, result_local) = match self {
                    Self::Limited {
                        argument_source,
                        result_source,
                        argument_local,
                        result_local,
                    } => (
                        *argument_source,
                        *result_source,
                        *argument_local,
                        *result_local,
                    ),
                    _ => (0, None, false, false),
                };
                let evidence =
                    |content: &ContentField, source: Option<SourceRef>, local| PreviewEvidence {
                        availability: content.state,
                        selected_size: None,
                        preview_utf8_bytes: content.text.as_ref().map(String::len),
                        source,
                        local_truncated: local,
                        source_truncation: SourceTruncation::Unknown,
                    };
                ToolInspection {
                    retention: Retention::RetentionLimited,
                    call_id: None,
                    call_id_truncated: false,
                    call_id_source: node.sources.get(start).cloned(),
                    arguments: evidence(&node.input, node.sources.get(start).cloned(), arg_local),
                    result: ResultEvidence {
                        preview: evidence(
                            &node.output,
                            end.and_then(|i| node.sources.get(i)).cloned(),
                            result_local,
                        ),
                        recorded_field: None,
                        original_length: None,
                    },
                }
            }
        }
    }
}
