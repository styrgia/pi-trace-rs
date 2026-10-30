use crate::{
    Trace, error,
    model::{aggregate, unknown},
    views::*,
};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StructuralFilter {
    Error,
    Tool,
    Generation,
    Slow,
}
#[derive(Debug, Clone)]
pub struct ObservationQuery {
    pub q: String,
    pub filter: Option<StructuralFilter>,
    pub cursor: Option<String>,
    pub limit: usize,
}
impl Default for ObservationQuery {
    fn default() -> Self {
        Self {
            q: String::new(),
            filter: None,
            cursor: None,
            limit: 100,
        }
    }
}
#[derive(Debug, Clone)]
pub struct DiagnosticQuery {
    pub cursor: Option<String>,
    pub limit: usize,
}
impl Default for DiagnosticQuery {
    fn default() -> Self {
        Self {
            cursor: None,
            limit: 100,
        }
    }
}
impl Trace {
    fn application(&self) -> crate::projection::Application<SourceRef> {
        crate::projection::Application {
            indexes: std::sync::OnceLock::new(),
            snapshot_id: self.snapshot_id.clone(),
            nodes: self
                .nodes
                .iter()
                .map(|n| crate::projection::ApplicationNode {
                    row: n.row(&self.nodes, &self.timeline, true, false),
                    metadata: n.metadata.clone(),
                    parent: n.parent,
                })
                .collect(),
            order: self.order.clone(),
            diagnostics: self.diagnostics.clone(),
            response_bytes: self.limits.response_bytes,
        }
    }
    fn budget<T: Serialize>(&self, view: T) -> Result<T, StructuralError> {
        self.application().budget(view)
    }
    pub fn observations(&self, q: &ObservationQuery) -> Result<ObservationPage, StructuralError> {
        self.application().observations(q)
    }
    pub fn diagnostics(&self, q: &DiagnosticQuery) -> Result<DiagnosticPage, StructuralError> {
        self.application().diagnostics(q)
    }
    pub fn overview(&self) -> Result<Overview, StructuralError> {
        let count = |kind| self.nodes.iter().filter(|node| node.kind == kind).count();
        self.budget(Overview {
            snapshot_id: self.snapshot_id.clone(),
            title: "Recording".into(),
            source: Some(SourceRef {
                file: "events.jsonl".into(),
                line: 1,
            }),
            model: self
                .nodes
                .iter()
                .find_map(|node| node.metadata.model.clone()),
            source_count: self.source_count,
            record_count: self.record_count,
            node_count: self.nodes.len(),
            session_count: count(NodeKind::Session),
            interaction_count: count(NodeKind::Interaction),
            turn_count: count(NodeKind::Turn),
            generation_count: count(NodeKind::Generation),
            tool_count: count(NodeKind::Tool),
            diagnostic_count: self.diagnostics.len(),
            duration: self
                .timeline
                .as_ref()
                .map(|domain| Measurement {
                    state: MeasurementState::Derived,
                    value_ms: Some(domain.duration_ms),
                    meaning: "global valid timeline span".into(),
                })
                .unwrap_or_else(|| unknown("global valid timeline span")),
            usage: aggregate(
                &self
                    .nodes
                    .iter()
                    .filter(|n| n.kind == NodeKind::Turn)
                    .map(|n| &n.usage)
                    .collect::<Vec<_>>(),
            ),
            token_accounting: self.token_accounting.clone(),
            timeline: self.timeline.clone(),
            measurement_availability: MeasurementAvailability {
                duration: self.nodes.iter().any(|n| n.duration.value_ms.is_some()),
                request_to_headers: self.nodes.iter().any(|n| n.headers.value_ms.is_some()),
                ttft: false,
            },
        })
    }
    fn index(&self, id: &str) -> Result<usize, StructuralError> {
        id.strip_prefix('n')
            .and_then(|n| n.parse::<usize>().ok())
            .and_then(|n| n.checked_sub(1))
            .filter(|i| self.nodes.get(*i).is_some_and(|n| n.id == id))
            .ok_or_else(|| {
                error(
                    "unknown_node",
                    "The observation is unavailable in this snapshot.",
                    None,
                )
            })
    }
    pub fn span(&self, id: &str) -> Result<NodeDetail, StructuralError> {
        let index = self.span_index.get_or_init(|| {
            self.nodes
                .iter()
                .enumerate()
                .map(|(i, n)| (n.span_id.clone(), i))
                .collect()
        });
        let i = index.get(id).ok_or_else(|| {
            crate::error(
                "unknown_span",
                "The span is unavailable in this snapshot.",
                None,
            )
        })?;
        let node = &self.nodes[*i];
        self.node(&node.id)
    }
    pub fn node(&self, id: &str) -> Result<NodeDetail, StructuralError> {
        let node = &self.nodes[self.index(id)?];
        self.budget(NodeDetail {
            snapshot_id: self.snapshot_id.clone(),
            span_usage: crate::accounting::detail(&self.nodes, self.index(id)?),
            observation: node.row(&self.nodes, &self.timeline, true, false),
            relationship: Relationship {
                state: node.relationship,
                execution_parent_id: if node.relationship == RelationshipState::Linked {
                    node.parent.map(|i| self.nodes[i].id.clone())
                } else {
                    None
                },
            },
            request_to_headers: node.headers.clone(),
            ttft: unknown("exact first token latency is not recorded by this source"),
            usage: node.usage.clone(),
            metadata: node.metadata.clone(),
            context: (node.kind == NodeKind::Generation).then(|| {
                let mut context = node.context.view(&node.sources, &node.usage);
                context.context_input_tokens_exact =
                    crate::accounting::context_input(node.generation_usage.as_ref());
                context
            }),
            tool_inspection: (node.kind == NodeKind::Tool).then(|| node.tool_inspection.view(node)),
            sources: node.sources.clone(),
            diagnostic_ids: node.diagnostic_ids.clone(),
        })
    }
    pub fn content(&self, id: &str) -> Result<NodeContent, StructuralError> {
        let node = &self.nodes[self.index(id)?];
        self.budget(NodeContent {
            snapshot_id: self.snapshot_id.clone(),
            node_id: node.id.clone(),
            input: node.input.clone(),
            output: node.output.clone(),
        })
    }
}
