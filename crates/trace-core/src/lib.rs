mod accounting;
mod follow;
mod identity;
mod import;
mod inspection;
mod model;
pub mod native;
mod projection;
mod query;
mod records;
mod reducer;
pub mod views;

pub use follow::{FollowProgress, FollowReader, FollowUpdate};
pub use identity::RunId;
use model::Node;
pub use query::{DiagnosticQuery, ObservationQuery, StructuralFilter};
use std::path::Path;
pub use views::StructuralError as ImportError;
use views::*;

#[derive(Debug, Clone, Copy)]
pub struct ImportLimits {
    pub total_bytes: usize,
    pub record_bytes: usize,
    pub records: usize,
    pub sources: usize,
    pub response_bytes: usize,
}
impl Default for ImportLimits {
    fn default() -> Self {
        Self {
            total_bytes: 128 * 1024 * 1024,
            record_bytes: 1024 * 1024,
            records: 100_000,
            sources: 1024,
            response_bytes: RESPONSE_BYTES,
        }
    }
}

pub struct Trace {
    run_id: String,
    snapshot_id: String,
    span_index: std::sync::OnceLock<std::collections::HashMap<String, usize>>,
    nodes: Vec<Node>,
    order: Vec<usize>,
    diagnostics: Vec<Diagnostic>,
    source_count: usize,
    record_count: usize,
    timeline: Option<Timeline>,
    token_accounting: TokenAccounting,
    limits: ImportLimits,
}
impl Trace {
    pub fn open(source: &Path, limits: ImportLimits) -> Result<Self, ImportError> {
        import::open(source, limits)
    }
    pub fn run_id(&self) -> &str {
        &self.run_id
    }
    pub fn snapshot_id(&self) -> &str {
        &self.snapshot_id
    }
}
impl<S: std::fmt::Debug> std::fmt::Display for StructuralError<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl<S: std::fmt::Debug> std::error::Error for StructuralError<S> {}
pub(crate) fn error(code: &str, message: &str, source: Option<SourceRef>) -> StructuralError {
    StructuralError {
        code: code.into(),
        message: message.into(),
        source,
    }
}
pub(crate) fn bounded(text: &str, limit: usize) -> (String, bool) {
    let mut end = text.len().min(limit);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].to_owned(), end < text.len())
}

#[cfg(test)]
mod contracts {
    use super::*;
    #[test]
    fn serialized_contract_examples() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut trace = Trace::open(
            &base.join("tests/fixtures/reference"),
            ImportLimits::default(),
        )
        .unwrap();
        trace.snapshot_id = "synthetic-snapshot".into();
        let mut edge =
            Trace::open(&base.join("tests/fixtures/edge"), ImportLimits::default()).unwrap();
        edge.snapshot_id = "synthetic-edge-snapshot".into();
        let empty_directory = tempfile::tempdir().unwrap();
        std::fs::write(empty_directory.path().join("events.jsonl"), "").unwrap();
        let mut empty = Trace::open(empty_directory.path(), ImportLimits::default()).unwrap();
        empty.snapshot_id = "synthetic-empty-snapshot".into();
        let mut limited = Trace::open(
            &base.join("tests/fixtures/reference"),
            ImportLimits::default(),
        )
        .unwrap();
        limited.snapshot_id = "synthetic-limited-snapshot".into();
        limited.nodes[3].context = model::ContextState::Limited {
            coverage: Coverage::Complete,
            shape: RequestShape::ProviderInput,
            field: Some(RequestField::Input),
            source_index: 1,
        };
        limited.nodes[4].tool_inspection = model::ToolState::Limited {
            argument_source: 0,
            result_source: Some(1),
            argument_local: false,
            result_local: false,
        };
        for item in [&mut trace, &mut edge, &mut empty, &mut limited] {
            for (i, node) in item.nodes.iter_mut().enumerate() {
                node.span_id = format!("synthetic-span-{}", i + 1);
                if node.contribution_id.is_some() {
                    node.contribution_id = Some(format!(
                        "{}-{}",
                        node.span_id,
                        if node.kind == NodeKind::Generation {
                            "g"
                        } else {
                            "t"
                        }
                    ));
                }
            }
        }
        let samples = [
            (
                "detail-inspection-retention-limited",
                serde_json::to_value(limited.node("n4").unwrap()).unwrap(),
            ),
            (
                "detail-inspection-tool-retention-limited",
                serde_json::to_value(limited.node("n5").unwrap()).unwrap(),
            ),
            (
                "detail-invalid",
                serde_json::to_value(edge.node("n4").unwrap()).unwrap(),
            ),
            (
                "detail-partial",
                serde_json::to_value(edge.node("n3").unwrap()).unwrap(),
            ),
            (
                "detail-fallback-partial",
                serde_json::to_value(edge.node("n6").unwrap()).unwrap(),
            ),
            (
                "content-truncated",
                serde_json::to_value(edge.content("n4").unwrap()).unwrap(),
            ),
            (
                "diagnostics-nonempty",
                serde_json::to_value(edge.diagnostics(&DiagnosticQuery::default()).unwrap())
                    .unwrap(),
            ),
            (
                "overview-empty",
                serde_json::to_value(empty.overview().unwrap()).unwrap(),
            ),
            (
                "observations-empty",
                serde_json::to_value(empty.observations(&ObservationQuery::default()).unwrap())
                    .unwrap(),
            ),
            (
                "overview",
                serde_json::to_value(trace.overview().unwrap()).unwrap(),
            ),
            (
                "observations",
                serde_json::to_value(trace.observations(&ObservationQuery::default()).unwrap())
                    .unwrap(),
            ),
            (
                "observations-page",
                serde_json::to_value(
                    trace
                        .observations(&ObservationQuery {
                            limit: 2,
                            ..Default::default()
                        })
                        .unwrap(),
                )
                .unwrap(),
            ),
            (
                "detail",
                serde_json::to_value(trace.node("n4").unwrap()).unwrap(),
            ),
            (
                "content",
                serde_json::to_value(trace.content("n4").unwrap()).unwrap(),
            ),
            (
                "content-unavailable",
                serde_json::to_value(trace.content("n1").unwrap()).unwrap(),
            ),
            (
                "diagnostics",
                serde_json::to_value(trace.diagnostics(&DiagnosticQuery::default()).unwrap())
                    .unwrap(),
            ),
            (
                "error",
                serde_json::to_value(ErrorResponse {
                    error: error(
                        "unknown_node",
                        "The observation is unavailable in this snapshot.",
                        None,
                    ),
                })
                .unwrap(),
            ),
        ];
        assert_eq!(trace.overview().unwrap().node_count, 14);
        for (name, value) in samples {
            let path = base.join(format!("tests/contracts/{name}.json"));
            let serialized = format!("{}\n", serde_json::to_string_pretty(&value).unwrap());
            if std::env::var_os("UPDATE_CONTRACTS").is_some() {
                std::fs::write(&path, &serialized).unwrap();
            }
            assert_eq!(std::fs::read_to_string(path).unwrap(), serialized);
        }
    }
}
