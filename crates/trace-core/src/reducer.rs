use crate::{model::*, views::*};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Default)]
pub(crate) struct Occurrence {
    session: Option<usize>,
    interaction: Option<usize>,
    turn: Option<usize>,
    generation: Option<usize>,
    tools: HashMap<String, Vec<usize>>,
    generations: Vec<usize>,
}
#[derive(Clone)]
pub(crate) struct Builder {
    pub(crate) nodes: Vec<Node>,
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) sessions: Vec<(usize, Option<String>)>,
    pub(crate) inspection_bytes: usize,
    run_id: String,
    source_anchors: HashMap<String, (usize, usize)>,
    projecting_live: bool,
    pub(crate) token_accounting: TokenAccounting,
}
impl Builder {
    pub(crate) fn new() -> Self {
        Self::with_run(crate::RunId::new())
    }
    pub(crate) fn with_run(run_id: crate::RunId) -> Self {
        Self {
            nodes: Vec::new(),
            diagnostics: Vec::new(),
            sessions: Vec::new(),
            inspection_bytes: 0,
            run_id: run_id.as_str().to_owned(),
            source_anchors: HashMap::new(),
            projecting_live: false,
            token_accounting: crate::accounting::empty_accounting(),
        }
    }
    pub(crate) fn run_id(&self) -> &str {
        &self.run_id
    }
    pub(crate) fn record(
        &mut self,
        occurrence: &mut Occurrence,
        record: &[u8],
        terminated: bool,
        location: SourceRef,
        child_id: Option<&str>,
    ) {
        match serde_json::from_slice::<Value>(record) {
            Ok(value) if value.is_object() => self.event(occurrence, value, location, child_id),
            Ok(_) => self.diagnostic(
                "invalid_record",
                "The record is not a JSON object.",
                location,
                None,
            ),
            Err(e) => {
                let tail = !terminated && e.is_eof();
                self.diagnostic(
                    if tail {
                        "incomplete_tail"
                    } else {
                        "malformed_record"
                    },
                    if tail {
                        "The final physical record is incomplete."
                    } else {
                        "The physical record is not valid JSON."
                    },
                    location,
                    None,
                );
            }
        }
    }
    pub(crate) fn project(&self, closed: bool) -> Self {
        let mut result = self.clone();
        result.projecting_live = !closed;
        result.finalize(closed);
        let accounting = crate::accounting::apply(&mut result.nodes);
        result.token_accounting = accounting.overview;
        for index in accounting.overflow_nodes {
            result.diagnostic(
                "aggregate_usage_overflow",
                "Accepted usage exceeds the supported unsigned integer range when combined.",
                result.nodes[index].sources[0].clone(),
                Some(index),
            );
        }
        result
    }
    fn usage_evidence(
        &mut self,
        index: usize,
        event: &Value,
        source: &SourceRef,
    ) -> crate::accounting::RecordedUsage {
        let evidence = crate::accounting::RecordedUsage::parse(event.get("usage"), source.clone());
        for code in &evidence.problems {
            let message = match *code {
                "invalid_usage_value" => {
                    "A recorded token counter is not a supported unsigned integer."
                }
                "usage_overflow" => {
                    "Recorded token components exceed the supported unsigned integer range."
                }
                _ => "The explicit token total conflicts with the complete component sum.",
            };
            self.diagnostic(code, message, source.clone(), Some(index));
        }
        evidence
    }
    fn retain_tool(&mut self, i: usize, inspection: ToolInspection) {
        self.inspection_bytes -= self.nodes[i].tool_inspection.retained_bytes();
        let retained = crate::inspection::bytes(&inspection);
        let node = &mut self.nodes[i];
        node.tool_inspection = if inspection.retention != Retention::RetentionLimited
            && retained <= crate::inspection::NODE_BYTES
            && retained <= crate::inspection::TOTAL_BYTES.saturating_sub(self.inspection_bytes)
        {
            self.inspection_bytes += retained;
            ToolState::Retained(Box::new(inspection))
        } else {
            ToolState::Limited {
                argument_source: 0,
                result_source: inspection
                    .result
                    .preview
                    .source
                    .as_ref()
                    .and_then(|source| node.sources.iter().position(|s| s == source)),
                argument_local: inspection.arguments.local_truncated,
                result_local: inspection.result.preview.local_truncated,
            }
        };
    }
    fn diagnostic(&mut self, code: &str, message: &str, source: SourceRef, node: Option<usize>) {
        let id = if self.projecting_live {
            match node {
                Some(index) => format!("p-{}-{}", code, self.nodes[index].span_id),
                None => format!("d{}", self.diagnostics.len() + 1),
            }
        } else {
            format!("d{}", self.diagnostics.len() + 1)
        };
        if let Some(index) = node {
            self.nodes[index].diagnostic_ids.push(id.clone());
        }
        self.diagnostics.push(Diagnostic {
            id,
            code: code.into(),
            message: message.into(),
            source,
            node_id: node.map(|i| self.nodes[i].id.clone()),
        });
    }
    fn add(
        &mut self,
        kind: NodeKind,
        parent: Option<usize>,
        source: SourceRef,
        event: &Value,
    ) -> usize {
        let index = self.nodes.len();
        let next_source = self.source_anchors.len();
        let anchor = self
            .source_anchors
            .entry(source.file.clone())
            .or_insert((next_source, 0));
        anchor.1 += 1;
        let span_id = format!("{}-{:x}-{:x}", self.run_id, anchor.0, anchor.1);
        let mut node = Node::new(index, kind, parent, source, event);
        node.span_id = span_id;
        self.nodes.push(node);
        if let Some(parent) = parent {
            self.nodes[parent].children.push(index);
        }
        index
    }
    fn ensure(
        &mut self,
        o: &mut Occurrence,
        kind: NodeKind,
        event: &Value,
        source: &SourceRef,
        child_id: Option<&str>,
    ) -> usize {
        if o.session.is_none() {
            let i = self.add(NodeKind::Session, None, source.clone(), event);
            o.session = Some(i);
            self.sessions.push((i, child_id.map(str::to_owned)));
            self.nodes[i].start = None;
            self.diagnostic(
                "missing_start",
                "Session start was not recorded.",
                source.clone(),
                Some(i),
            );
        }
        if kind == NodeKind::Session {
            return o.session.unwrap();
        }
        if o.interaction.is_none() {
            let i = self.add(NodeKind::Interaction, o.session, source.clone(), event);
            o.interaction = Some(i);
            self.nodes[i].start = None;
            self.diagnostic(
                "missing_start",
                "Interaction start was not recorded.",
                source.clone(),
                Some(i),
            );
        }
        if kind == NodeKind::Interaction {
            return o.interaction.unwrap();
        }
        if o.turn.is_none() {
            let i = self.add(NodeKind::Turn, o.interaction, source.clone(), event);
            o.turn = Some(i);
            self.nodes[i].start = None;
            self.diagnostic(
                "missing_start",
                "Turn start was not recorded.",
                source.clone(),
                Some(i),
            );
        }
        if kind == NodeKind::Turn {
            return o.turn.unwrap();
        }
        if o.generation.is_none() {
            let i = self.add(NodeKind::Generation, o.turn, source.clone(), event);
            o.generation = Some(i);
            o.generations.push(i);
            self.nodes[i].start = None;
            self.diagnostic(
                "missing_start",
                "Generation start was not recorded.",
                source.clone(),
                Some(i),
            );
        }
        o.generation.unwrap()
    }
    fn attach(&mut self, i: usize, source: &SourceRef, event: &Value) {
        if self.nodes[i].sources.last() != Some(source) {
            self.nodes[i].sources.push(source.clone());
        }
        if let Some(model) = string(event, "model") {
            self.nodes[i].metadata.model = Some(model);
        }
    }
    fn finish(&mut self, i: usize, source: &SourceRef, event: &Value, duration: bool) {
        self.attach(i, source, event);
        let node = &mut self.nodes[i];
        node.ended = true;
        node.end = number(event.get("ts"));
        node.completion = CompletionEvidence {
            reason: CompletionReason::Recorded {
                failed: event.get("isError").and_then(Value::as_bool) == Some(true)
                    || event.get("outcome").and_then(Value::as_str) == Some("error"),
            },
            missing_prerequisite: false,
        };
        if duration {
            node.duration = recorded(
                event.get("durationMs"),
                if node.kind == NodeKind::Generation {
                    "completed generation"
                } else {
                    "tool duration"
                },
            );
        }
    }
    fn event(
        &mut self,
        o: &mut Occurrence,
        event: Value,
        source: SourceRef,
        child_id: Option<&str>,
    ) {
        let kind = event.get("type").and_then(Value::as_str).unwrap_or("");
        match kind {
            "session_start" => {
                *o = Occurrence::default();
                let i = self.add(NodeKind::Session, None, source, &event);
                o.session = Some(i);
                self.sessions.push((i, child_id.map(str::to_owned)));
            }
            "interaction_start" => {
                let parent = self.ensure(o, NodeKind::Session, &event, &source, child_id);
                let i = self.add(NodeKind::Interaction, Some(parent), source, &event);
                o.interaction = Some(i);
                o.turn = None;
                o.generation = None;
                o.tools.clear();
                o.generations.clear();
                self.nodes[i].input = preview(event.get("prompt").or_else(|| event.get("input")));
            }
            "turn_start" => {
                let parent = self.ensure(o, NodeKind::Interaction, &event, &source, child_id);
                let i = self.add(NodeKind::Turn, Some(parent), source, &event);
                o.turn = Some(i);
                o.generation = None;
                o.tools.clear();
                o.generations.clear();
                self.nodes[parent].ended = false;
                self.nodes[parent].end = None;
                self.nodes[parent].completion = CompletionEvidence::default();
            }
            "step_start" => {
                let parent = self.ensure(o, NodeKind::Turn, &event, &source, child_id);
                let generation = self.add(NodeKind::Generation, Some(parent), source, &event);
                o.generation = Some(generation);
                o.generations.push(generation);
            }
            "llm_request" | "llm_response" | "step_end" => {
                let i = self.ensure(o, NodeKind::Generation, &event, &source, child_id);
                self.attach(i, &source, &event);
                if kind == "llm_request" {
                    if !matches!(self.nodes[i].context, ContextState::Missing) {
                        self.diagnostic("duplicate_request", "A generation has more than one request record; the last request summary and preview are retained together.", source.clone(), Some(i));
                    }
                    self.nodes[i].input = preview(crate::inspection::selected(&event).1);
                    let (mut inspection, malformed) = crate::inspection::context(&event, &source);
                    if malformed {
                        self.diagnostic("context_sections_malformed", "Request sections have malformed or unsupported nesting; section and skill coverage is partial.", source.clone(), Some(i));
                    }
                    if ["messages", "prompt", "input"]
                        .iter()
                        .filter(|key| event.get(**key).is_some())
                        .count()
                        > 1
                    {
                        self.diagnostic("multiple_request_fields", "Multiple request fields are recorded; messages, prompt, input presence precedence selects the inspected value, including null.", source.clone(), Some(i));
                    }
                    self.inspection_bytes -= self.nodes[i].context.retained_bytes();
                    let available = crate::inspection::TOTAL_BYTES
                        .saturating_sub(self.inspection_bytes)
                        .min(crate::inspection::NODE_BYTES.saturating_sub(64));
                    crate::inspection::trim_context(&mut inspection, available);
                    let retained = crate::inspection::bytes(&inspection);
                    self.nodes[i].context = if retained <= available {
                        self.inspection_bytes += retained;
                        ContextState::Retained(Box::new(inspection))
                    } else {
                        ContextState::Limited {
                            coverage: inspection.coverage,
                            shape: inspection.shape,
                            field: inspection.request_field,
                            source_index: self.nodes[i].sources.len() - 1,
                        }
                    };
                } else if kind == "llm_response" {
                    self.nodes[i].headers =
                        recorded(event.get("durationMs"), "request to HTTP headers");
                } else {
                    if self.nodes[i].ended {
                        self.diagnostic("duplicate_end", "A generation has more than one completion record; the first is retained.", source, Some(i));
                        return;
                    }
                    self.finish(i, &source, &event, true);
                    self.nodes[i].usage = parse_usage(event.get("usage"));
                    self.nodes[i].generation_usage = Some(self.usage_evidence(i, &event, &source));
                    if self.nodes[i]
                        .generation_usage
                        .as_ref()
                        .is_some_and(|usage| usage.observed)
                        && self.nodes[i].usage.observed_generations == 0
                    {
                        self.nodes[i].usage.observed_generations = 1;
                        self.nodes[i].usage.state = UsageState::Partial;
                    }
                    self.nodes[i].stop_reason = string(&event, "stopReason");
                    self.nodes[i].completion = CompletionEvidence {
                        reason: CompletionReason::Stop(self.nodes[i].stop_reason.clone()),
                        missing_prerequisite: false,
                    };
                    self.nodes[i].output = preview(
                        event
                            .get("text")
                            .or_else(|| event.get("message"))
                            .or_else(|| event.get("output")),
                    );
                }
            }
            "tool_start" => {
                let current = self.ensure(o, NodeKind::Generation, &event, &source, child_id);
                let step_index = event.get("stepIndex").and_then(Value::as_u64);
                let generations: Vec<_> = o
                    .generations
                    .iter()
                    .copied()
                    .filter(|i| self.nodes[*i].metadata.step_index == step_index)
                    .collect();
                let parent = if step_index.is_none() {
                    Some(current)
                } else if generations.len() == 1 {
                    Some(generations[0])
                } else {
                    o.turn
                };
                let i = self.add(NodeKind::Tool, parent, source.clone(), &event);
                if step_index.is_some() && generations.len() != 1 {
                    self.nodes[i].relationship = if generations.is_empty() {
                        RelationshipState::Unresolved
                    } else {
                        RelationshipState::Ambiguous
                    };
                    self.diagnostic(
                        "unresolved_tool_generation",
                        "The tool has no unique generation occurrence for its step index.",
                        source.clone(),
                        Some(i),
                    );
                }
                self.nodes[i].input = preview(
                    event
                        .get("arguments")
                        .or_else(|| event.get("args"))
                        .or_else(|| event.get("input")),
                );
                self.retain_tool(i, crate::inspection::tool(&event, &source));
                if let Some(id) = event.get("toolCallId").and_then(Value::as_str) {
                    o.tools.entry(id.to_owned()).or_default().push(i);
                } else {
                    self.diagnostic(
                        "missing_tool_id",
                        "A tool start has no tool call identifier.",
                        source.clone(),
                        Some(i),
                    );
                }
                self.child_ids(i, &event);
            }
            "tool_end" => {
                let matches = event
                    .get("toolCallId")
                    .and_then(Value::as_str)
                    .and_then(|id| o.tools.get(id))
                    .map(|items| {
                        items
                            .iter()
                            .copied()
                            .filter(|i| {
                                !self.nodes[*i].ended
                                    && event.get("stepIndex").and_then(Value::as_u64).is_none_or(
                                        |step| self.nodes[*i].metadata.step_index == Some(step),
                                    )
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                if matches.len() != 1 {
                    self.diagnostic(
                        if matches.is_empty() {
                            "unmatched_tool_end"
                        } else {
                            "ambiguous_tool_end"
                        },
                        "A tool completion has no unique start in this generation occurrence.",
                        source,
                        None,
                    );
                    return;
                }
                let i = matches[0];
                self.finish(i, &source, &event, true);
                self.nodes[i].output = preview(
                    event
                        .get("resultPreview")
                        .or_else(|| event.get("result"))
                        .or_else(|| event.get("output")),
                );
                let mut inspection = self.nodes[i].tool_inspection.view(&self.nodes[i]);
                inspection.result = crate::inspection::tool_result(&event, &source);
                if inspection.result.preview.source_truncation == SourceTruncation::KnownTruncated {
                    self.nodes[i].output.truncated = true;
                }
                self.retain_tool(i, inspection);
                self.child_ids(i, &event);
            }
            "turn_summary" => {
                let i = self.ensure(o, NodeKind::Turn, &event, &source, child_id);
                self.attach(i, &source, &event);
                if self.nodes[i].summary_usage.is_none() {
                    self.nodes[i].summary_usage = Some(parse_usage(event.get("usage")));
                    self.nodes[i].summary_evidence = Some(self.usage_evidence(i, &event, &source));
                    self.nodes[i].stop_reason = string(&event, "stopReason");
                    self.nodes[i].input = preview(event.get("userPrompt"));
                    self.nodes[i].output = preview(event.get("finalText"));
                }
            }
            "turn_end" => {
                let i = self.ensure(o, NodeKind::Turn, &event, &source, child_id);
                self.finish(i, &source, &event, false);
                let final_generation_missing = matches!(
                    self.nodes[i].stop_reason.as_deref(),
                    Some("stop" | "length")
                ) && o.generations.last().is_none_or(|generation| {
                    !matches!(
                        self.nodes[*generation].stop_reason.as_deref(),
                        Some("stop" | "length")
                    )
                });
                self.nodes[i].completion = CompletionEvidence {
                    reason: CompletionReason::Stop(self.nodes[i].stop_reason.clone()),
                    missing_prerequisite: final_generation_missing
                        || o.generations.is_empty()
                        || o.generations
                            .iter()
                            .any(|generation| !self.nodes[*generation].ended),
                };
                if let Some(parent) = o.interaction {
                    self.finish(parent, &source, &event, false);
                    self.nodes[parent].completion = self.nodes[i].completion.clone();
                    if self.nodes[i].stop_reason.as_deref() == Some("toolUse") {
                        self.nodes[parent].ended = false;
                        self.nodes[parent].end = None;
                        self.nodes[parent].completion.missing_prerequisite = true;
                    }
                }
                o.tools.clear();
                o.generations.clear();
            }
            "session_shutdown" => {
                let i = self.ensure(o, NodeKind::Session, &event, &source, child_id);
                self.finish(i, &source, &event, false);
                if self.nodes[i]
                    .children
                    .iter()
                    .any(|child| !self.nodes[*child].ended)
                {
                    self.nodes[i].completion.missing_prerequisite = true;
                }
            }
            "file_change" => {
                if let Some(i) = o.turn {
                    self.attach(i, &source, &event);
                }
            }
            _ => self.diagnostic(
                "unknown_event",
                "The event type is unsupported.",
                source,
                None,
            ),
        }
    }
    fn child_ids(&mut self, i: usize, event: &Value) {
        for v in [
            event.get("childTraceId"),
            event.pointer("/result/childTraceId"),
            event.pointer("/result/details/childTraceId"),
        ] {
            if let Some(id) = v.and_then(Value::as_str)
                && !self.nodes[i]
                    .child_ids
                    .iter()
                    .any(|existing| existing == id)
            {
                self.nodes[i].child_ids.push(id.to_owned());
            }
        }
        if let Some(results) = event.pointer("/subagent/results").and_then(Value::as_array) {
            for result in results {
                if let Some(id) = result.get("childTraceId").and_then(Value::as_str)
                    && !self.nodes[i]
                        .child_ids
                        .iter()
                        .any(|existing| existing == id)
                {
                    self.nodes[i].child_ids.push(id.to_owned());
                }
            }
        }
    }
    fn finalize(&mut self, closed: bool) {
        for i in 0..self.nodes.len() {
            self.nodes[i].outcome = self.nodes[i].completion.outcome();
            if closed && !self.nodes[i].ended {
                self.diagnostic(
                    "missing_end",
                    "The observation has no completion record.",
                    self.nodes[i].sources[0].clone(),
                    Some(i),
                );
            }
            let node = &mut self.nodes[i];
            if let (Some(start), Some(end)) = (node.start, node.end) {
                if end < start || !(end - start).is_finite() {
                    if node.duration.state != MeasurementState::Recorded {
                        node.duration = Measurement {
                            state: MeasurementState::Invalid,
                            value_ms: None,
                            meaning: "timestamp interval".into(),
                        };
                    }
                    self.diagnostic(
                        "invalid_interval",
                        "The recorded wall-clock interval is invalid.",
                        self.nodes[i].sources.last().unwrap().clone(),
                        Some(i),
                    );
                } else if node.duration.state == MeasurementState::Unknown {
                    node.duration = Measurement {
                        state: MeasurementState::Derived,
                        value_ms: Some(end - start),
                        meaning: "timestamp interval".into(),
                    };
                }
            }
            if self.nodes[i].kind == NodeKind::Generation
                && self.nodes[i].usage.total_generations == 0
            {
                self.nodes[i].usage = empty_usage(1);
            }
        }
        let mut direct = vec![Vec::new(); self.nodes.len()];
        for (i, node) in self.nodes.iter().enumerate() {
            if let Some(parent) = node.parent {
                direct[parent].push(i);
            }
        }
        for i in (0..self.nodes.len()).rev() {
            let kind = self.nodes[i].kind;
            if matches!(
                kind,
                NodeKind::Turn | NodeKind::Interaction | NodeKind::Session
            ) {
                let aggregate_usage = aggregate(
                    &direct[i]
                        .iter()
                        .map(|child| &self.nodes[*child].usage)
                        .collect::<Vec<_>>(),
                );
                self.nodes[i].usage = aggregate_usage;
                if kind == NodeKind::Turn
                    && self.nodes[i].usage.observed_generations == 0
                    && let Some(mut summary) = self.nodes[i]
                        .summary_usage
                        .clone()
                        .filter(|u| u.state != UsageState::Unknown)
                {
                    if summary.state == UsageState::Complete {
                        summary.state = UsageState::Fallback;
                    }
                    summary.fallback_source = Some(FallbackSource::TurnSummary);
                    summary.observed_generations = 0;
                    summary.total_generations = self.nodes[i].usage.total_generations;
                    self.nodes[i].usage = summary;
                }
            }
        }
        let mut candidates: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, node) in self.nodes.iter().enumerate() {
            for id in &node.child_ids {
                let prefix = node.sources[0]
                    .file
                    .strip_suffix("events.jsonl")
                    .unwrap_or("");
                candidates
                    .entry(format!("{prefix}subagents/{id}/events.jsonl"))
                    .or_default()
                    .push(i);
            }
        }
        for (session, child_id) in self.sessions.clone() {
            let Some(child_id) = child_id else {
                continue;
            };
            let parents = candidates.get(&child_id).cloned().unwrap_or_default();
            let mut cycle = false;
            if parents.len() == 1 {
                let mut cursor = Some(parents[0]);
                while let Some(i) = cursor {
                    if i == session {
                        cycle = true;
                        break;
                    }
                    cursor = self.nodes[i].parent;
                }
            }
            if parents.len() == 1 && !cycle {
                self.nodes[session].parent = Some(parents[0]);
                self.nodes[session].relationship = RelationshipState::Linked;
            } else {
                self.nodes[session].relationship = if parents.len() > 1 || cycle {
                    RelationshipState::Ambiguous
                } else {
                    RelationshipState::Unresolved
                };
                self.diagnostic(
                    if parents.len() > 1 || cycle {
                        "ambiguous_child"
                    } else {
                        "unresolved_child"
                    },
                    "The child recording has no unique supported execution parent.",
                    self.nodes[session].sources[0].clone(),
                    Some(session),
                );
            }
        }
        for node in &mut self.nodes {
            node.children.clear();
        }
        for i in 0..self.nodes.len() {
            if let Some(parent) = self.nodes[i].parent {
                self.nodes[parent].children.push(i);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source_ref(file: &str, line: usize) -> SourceRef {
        SourceRef {
            file: file.into(),
            line,
        }
    }
    #[test]
    fn projections_preserve_evidence_and_only_closed_input_reports_missing_ends() {
        let mut builder = Builder {
            nodes: vec![],
            diagnostics: vec![],
            sessions: vec![],
            inspection_bytes: 0,
            ..Builder::new()
        };
        let mut occurrence = Occurrence::default();
        builder.record(
            &mut occurrence,
            br#"{"type":"step_start"}"#,
            true,
            source_ref("events.jsonl", 1),
            None,
        );
        let original_diagnostics = builder.diagnostics.len();
        let original_parents: Vec<_> = builder.nodes.iter().map(|node| node.parent).collect();
        let original_usage: Vec<_> = builder
            .nodes
            .iter()
            .map(|node| serde_json::to_value(&node.usage).unwrap())
            .collect();
        let live = builder.project(false);
        assert!(!live.diagnostics.iter().any(|d| d.code == "missing_end"));
        assert_eq!(builder.diagnostics.len(), original_diagnostics);
        let first = builder
            .project(true)
            .diagnostics
            .iter()
            .map(|d| d.code.clone())
            .collect::<Vec<_>>();
        let second = builder
            .project(true)
            .diagnostics
            .iter()
            .map(|d| d.code.clone())
            .collect::<Vec<_>>();
        assert_eq!(first, second);
        assert!(first.iter().any(|code| code == "missing_end"));
        assert_eq!(builder.diagnostics.len(), original_diagnostics);
        assert_eq!(
            builder
                .nodes
                .iter()
                .map(|node| node.parent)
                .collect::<Vec<_>>(),
            original_parents
        );
        assert_eq!(
            builder
                .nodes
                .iter()
                .map(|node| serde_json::to_value(&node.usage).unwrap())
                .collect::<Vec<_>>(),
            original_usage
        );
    }
    #[test]
    fn late_generation_completion_preserves_recorded_turn_outcome() {
        let mut builder = Builder {
            nodes: vec![],
            diagnostics: vec![],
            sessions: vec![],
            inspection_bytes: 0,
            ..Builder::new()
        };
        let mut occurrence = Occurrence::default();
        for (line, event) in [
            serde_json::json!({"type":"step_start"}),
            serde_json::json!({"type":"turn_summary", "stopReason":"stop"}),
            serde_json::json!({"type":"turn_end"}),
            serde_json::json!({"type":"step_end", "stopReason":"stop"}),
        ]
        .into_iter()
        .enumerate()
        {
            builder.event(
                &mut occurrence,
                event,
                source_ref("events.jsonl", line + 1),
                None,
            );
        }
        let turn = builder
            .nodes
            .iter()
            .position(|node| node.kind == NodeKind::Turn)
            .unwrap();
        assert_eq!(builder.nodes[turn].outcome, Outcome::Incomplete);
        assert_eq!(
            builder.project(false).nodes[turn].outcome,
            Outcome::Incomplete
        );
        assert_eq!(
            builder.project(true).nodes[turn].outcome,
            Outcome::Incomplete
        );
        let generation = builder
            .nodes
            .iter()
            .position(|node| node.kind == NodeKind::Generation)
            .unwrap();
        assert_eq!(
            builder.project(false).nodes[generation].outcome,
            Outcome::Success
        );
        assert_eq!(builder.nodes[turn].outcome, Outcome::Incomplete);
    }
    #[test]
    fn completion_after_shutdown_preserves_shutdown_evidence() {
        let mut builder = Builder::new();
        let mut occurrence = Occurrence::default();
        for (line, event) in [
            serde_json::json!({"type":"session_start"}),
            serde_json::json!({"type":"step_start"}),
            serde_json::json!({"type":"step_end", "stopReason":"stop"}),
            serde_json::json!({"type":"turn_summary", "stopReason":"stop"}),
            serde_json::json!({"type":"session_shutdown"}),
            serde_json::json!({"type":"turn_end"}),
        ]
        .into_iter()
        .enumerate()
        {
            builder.event(
                &mut occurrence,
                event,
                source_ref("events.jsonl", line + 1),
                None,
            );
        }
        for closed in [false, true] {
            let projected = builder.project(closed);
            assert_eq!(projected.nodes[0].outcome, Outcome::Incomplete);
            assert_eq!(projected.nodes[2].outcome, Outcome::Success);
        }
    }
    #[test]
    fn summary_after_turn_end_does_not_rewrite_recorded_completion() {
        let mut builder = Builder::new();
        let mut occurrence = Occurrence::default();
        for (line, event) in [
            serde_json::json!({"type":"step_start"}),
            serde_json::json!({"type":"step_end","stopReason":"stop"}),
            serde_json::json!({"type":"turn_end"}),
            serde_json::json!({"type":"turn_summary","stopReason":"stop"}),
        ]
        .into_iter()
        .enumerate()
        {
            builder.event(
                &mut occurrence,
                event,
                source_ref("events.jsonl", line + 1),
                None,
            );
        }
        let turn = builder
            .nodes
            .iter()
            .position(|node| node.kind == NodeKind::Turn)
            .unwrap();
        assert_eq!(builder.project(true).nodes[turn].outcome, Outcome::Unknown);
    }
    #[test]
    fn derived_diagnostic_identity_survives_later_input_diagnostics() {
        let mut builder = Builder::new();
        let mut child = Occurrence::default();
        builder.record(
            &mut child,
            br#"{"type":"session_start"}"#,
            true,
            source_ref("subagents/a/events.jsonl", 1),
            Some("subagents/a/events.jsonl"),
        );
        let before = builder
            .project(false)
            .diagnostics
            .iter()
            .find(|d| d.code == "unresolved_child")
            .unwrap()
            .id
            .clone();
        builder.record(
            &mut child,
            br#"{"type":"unsupported"}"#,
            true,
            source_ref("subagents/a/events.jsonl", 2),
            Some("subagents/a/events.jsonl"),
        );
        let after = builder
            .project(false)
            .diagnostics
            .iter()
            .find(|d| d.code == "unresolved_child")
            .unwrap()
            .id
            .clone();
        assert_eq!(before, after);
    }
    #[test]
    fn span_ids_survive_appends_and_late_relationships_and_distinguish_occurrences() {
        let mut builder = Builder {
            nodes: vec![],
            diagnostics: vec![],
            sessions: vec![],
            inspection_bytes: 0,
            ..Builder::new()
        };
        let mut parent = Occurrence::default();
        let mut child = Occurrence::default();
        builder.record(
            &mut parent,
            br#"{"type":"step_start","stepIndex":1}"#,
            true,
            source_ref("events.jsonl", 1),
            None,
        );
        builder.record(
            &mut child,
            br#"{"type":"session_start"}"#,
            true,
            source_ref("subagents/a/events.jsonl", 1),
            Some("subagents/a/events.jsonl"),
        );
        let child_span = builder.nodes.last().unwrap().span_id.clone();
        let original: Vec<_> = builder
            .nodes
            .iter()
            .map(|node| node.span_id.clone())
            .collect();
        builder.record(
            &mut parent,
            br#"{"type":"step_start","stepIndex":1}"#,
            true,
            source_ref("events.jsonl", 2),
            None,
        );
        builder.record(
            &mut parent,
            br#"{"type":"tool_start","toolCallId":"x","childTraceId":"a"}"#,
            true,
            source_ref("events.jsonl", 3),
            None,
        );
        let projected = builder.project(false);
        assert_eq!(
            &projected.nodes[..original.len()]
                .iter()
                .map(|node| node.span_id.clone())
                .collect::<Vec<_>>(),
            &original
        );
        assert_eq!(projected.nodes[original.len() - 1].span_id, child_span);
        assert_eq!(
            projected.nodes[original.len() - 1].relationship,
            RelationshipState::Linked
        );
        let mut another_run = Builder::new();
        let mut another_occurrence = Occurrence::default();
        another_run.record(
            &mut another_occurrence,
            br#"{"type":"step_start","stepIndex":1}"#,
            true,
            source_ref("events.jsonl", 1),
            None,
        );
        assert_ne!(another_run.nodes[0].span_id, original[0]);
        let unique: std::collections::HashSet<_> =
            projected.nodes.iter().map(|node| &node.span_id).collect();
        assert_eq!(unique.len(), projected.nodes.len());
        assert!(!child_span.is_empty());
    }
    #[test]
    fn inspection_budget_exhaustion_preserves_nodes_reveal_sources_and_coverage() {
        let mut builder = Builder {
            nodes: vec![],
            diagnostics: vec![],
            sessions: vec![],
            inspection_bytes: crate::inspection::TOTAL_BYTES,
            ..Builder::new()
        };
        let mut occurrence = Occurrence::default();
        for (line, event) in [
            serde_json::json!({"type":"step_start"}),
            serde_json::json!({"type":"llm_request","input":{"unsupported":"PRIVATE"}}),
            serde_json::json!({"type":"tool_start","toolName":"read","toolCallId":"exact","args":{"path":"PRIVATE"}}),
            serde_json::json!({"type":"tool_end","toolCallId":"exact","resultPreview":"PRIVATE_RESULT","resultTotalLength":500}),
        ].into_iter().enumerate() {
            builder.event(&mut occurrence, event, source_ref("events.jsonl", line + 1), None);
        }
        assert_eq!(builder.inspection_bytes, crate::inspection::TOTAL_BYTES);
        let node = &builder.nodes[3];
        let context = node.context.view(&node.sources, &node.usage);
        assert_eq!(context.coverage, Coverage::Unsupported);
        assert_eq!(context.retention, Retention::RetentionLimited);
        assert_eq!(context.source.unwrap().line, 2);
        assert!(context.counts.is_none());
        assert!(context.payload_size.is_none());
        assert!(context.categories.is_empty());
        let tool = &builder.nodes[4];
        let inspection = tool.tool_inspection.view(tool);
        assert_eq!(inspection.retention, Retention::RetentionLimited);
        assert_eq!(inspection.arguments.availability, ContentState::Available);
        assert_eq!(
            inspection.result.preview.availability,
            ContentState::Available
        );
        assert_eq!(inspection.result.preview.source.unwrap().line, 4);
        assert_eq!(tool.metadata.tool_name.as_deref(), Some("read"));
        assert_eq!(builder.project(true).nodes[4].outcome, Outcome::Success);
        assert!(tool.input.text.as_ref().unwrap().contains("PRIVATE"));
        assert_eq!(tool.output.text.as_deref(), Some("PRIVATE_RESULT"));
    }
    #[test]
    fn replacement_releases_only_its_retained_inspection_budget() {
        let mut builder = Builder {
            nodes: vec![],
            diagnostics: vec![],
            sessions: vec![],
            inspection_bytes: 0,
            ..Builder::new()
        };
        let mut occurrence = Occurrence::default();
        for line in 1..=3 {
            builder.event(
                &mut occurrence,
                serde_json::json!({"type":"llm_request","prompt":"x".repeat(line)}),
                source_ref("events.jsonl", line),
                None,
            );
            let retained = builder
                .nodes
                .iter()
                .map(|node| node.context.retained_bytes() + node.tool_inspection.retained_bytes())
                .sum::<usize>();
            assert_eq!(builder.inspection_bytes, retained);
        }
        assert_eq!(
            builder.nodes[3]
                .context
                .view(&builder.nodes[3].sources, &builder.nodes[3].usage)
                .payload_size
                .unwrap()
                .text_chars,
            Some(3)
        );
    }
}
