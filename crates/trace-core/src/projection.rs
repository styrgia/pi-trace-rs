use crate::{
    query::{DiagnosticQuery, ObservationQuery, StructuralFilter},
    views::*,
};
use serde::Serialize;
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};
pub(crate) struct ApplicationNode<S> {
    pub row: Observation<S>,
    pub metadata: Metadata,
    pub parent: Option<usize>,
}
pub(crate) struct Application<S> {
    pub snapshot_id: String,
    pub nodes: Vec<ApplicationNode<S>>,
    pub order: Vec<usize>,
    pub diagnostics: Vec<Diagnostic<S>>,
    pub response_bytes: usize,
    pub indexes: std::sync::OnceLock<(
        std::collections::HashMap<String, usize>,
        std::collections::HashMap<String, usize>,
    )>,
}
pub(crate) fn failure<S>(code: &str, message: &str, source: Option<S>) -> StructuralError<S> {
    StructuralError {
        code: code.into(),
        message: message.into(),
        source,
    }
}
fn validate<S>(limit: usize, cursor: &Option<String>) -> Result<(), StructuralError<S>> {
    if !(1..=200).contains(&limit) {
        return Err(failure(
            "invalid_query",
            "Page size must be between 1 and 200.",
            None,
        ));
    }
    if cursor.as_ref().is_some_and(|c| c.len() > 2048) {
        return Err(failure(
            "cursor_limit",
            "The continuation token exceeds 2048 bytes.",
            None,
        ));
    }
    Ok(())
}
impl<S: Clone + Serialize> Application<S> {
    pub(crate) fn budget<T: Serialize>(&self, view: T) -> Result<T, StructuralError<S>> {
        if self.fits(&view) {
            Ok(view)
        } else {
            Err(failure(
                "view_limit",
                "The serialized view exceeds the response byte limit.",
                None,
            ))
        }
    }
    pub(crate) fn fits(&self, view: &impl Serialize) -> bool {
        serde_json::to_vec(view).is_ok_and(|bytes| bytes.len() <= self.response_bytes)
    }
    fn cursor(&self, scope: u64, offset: usize) -> String {
        format!(
            "{}.{}.{offset:x}",
            self.snapshot_id,
            format_args!("{scope:x}")
        )
    }
    fn offset(
        &self,
        cursor: &Option<String>,
        scope: u64,
        total: usize,
    ) -> Result<usize, StructuralError<S>> {
        let Some(cursor) = cursor else {
            return Ok(0);
        };
        let prefix = format!("{}.{scope:x}.", self.snapshot_id);
        let parsed = cursor
            .strip_prefix(&prefix)
            .and_then(|offset| usize::from_str_radix(offset, 16).ok());
        match parsed {
            Some(n) if n > 0 && n < total => Ok(n),
            _ => Err(failure(
                "invalid_cursor",
                "The continuation token does not match this snapshot and query.",
                None,
            )),
        }
    }
    pub fn observations(
        &self,
        query: &ObservationQuery,
    ) -> Result<ObservationPage<S>, StructuralError<S>> {
        validate::<S>(query.limit, &query.cursor)?;
        if query.q.chars().count() > 256 {
            return Err(failure(
                "query_limit",
                "Metadata search exceeds 256 Unicode characters.",
                None,
            ));
        }
        let mut hasher = DefaultHasher::new();
        ("observations", &query.q, query.filter, query.limit).hash(&mut hasher);
        let scope = hasher.finish();
        let needle = query.q.to_lowercase();
        let mut matched = vec![false; self.nodes.len()];
        let mut included = vec![false; self.nodes.len()];
        for (i, item) in self.nodes.iter().enumerate() {
            let node = &item.row;
            let filter = match query.filter {
                None => true,
                Some(StructuralFilter::Error) => node.outcome == Outcome::Error,
                Some(StructuralFilter::Tool) => node.kind == NodeKind::Tool,
                Some(StructuralFilter::Generation) => node.kind == NodeKind::Generation,
                Some(StructuralFilter::Slow) => {
                    matches!(node.kind, NodeKind::Generation | NodeKind::Tool)
                        && matches!(
                            node.duration.state,
                            MeasurementState::Recorded | MeasurementState::Derived
                        )
                        && node.duration.value_ms.is_some_and(|ms| ms > 5000.0)
                }
            };
            let kind = match node.kind {
                NodeKind::Session => "session",
                NodeKind::Interaction => "interaction",
                NodeKind::Turn => "turn",
                NodeKind::Generation => "generation",
                NodeKind::Tool => "tool",
                NodeKind::Message => "message",
            };
            let search = needle.is_empty()
                || node.id.contains(&needle)
                || kind.contains(&needle)
                || [
                    &Some(node.label.clone()),
                    &item.metadata.model,
                    &item.metadata.tool_name,
                    &item.metadata.session_id,
                ]
                .iter()
                .any(|value| {
                    value
                        .as_ref()
                        .is_some_and(|value| value.to_lowercase().contains(&needle))
                });
            if filter && search {
                matched[i] = true;
                let mut cursor = Some(i);
                while let Some(index) = cursor {
                    if included[index] {
                        break;
                    }
                    included[index] = true;
                    cursor = self.nodes[index].parent;
                }
            }
        }
        let ordered: Vec<_> = self
            .order
            .iter()
            .copied()
            .filter(|i| included[*i])
            .collect();
        let offset = self.offset(&query.cursor, scope, ordered.len())?;
        let mut page = ObservationPage {
            snapshot_id: self.snapshot_id.clone(),
            items: Vec::new(),
            total_matches: matched.iter().filter(|n| **n).count(),
            total_rows: ordered.len(),
            next_cursor: None,
        };
        for i in ordered.iter().skip(offset).take(query.limit) {
            let mut row = self.nodes[*i].row.clone();
            row.r#match = matched[*i];
            row.context = !matched[*i];
            page.items.push(row);
            let next = offset + page.items.len();
            page.next_cursor = (next < ordered.len()).then(|| self.cursor(scope, next));
            if !self.fits(&page) {
                page.items.pop();
                if page.items.is_empty() {
                    return Err(failure(
                        "view_limit",
                        "One observation cannot fit in the response byte limit.",
                        Some(self.nodes[*i].row.source.clone()),
                    ));
                }
                page.next_cursor = Some(self.cursor(scope, offset + page.items.len()));
                break;
            }
        }
        self.budget(page)
    }
    pub fn diagnostics(
        &self,
        query: &DiagnosticQuery,
    ) -> Result<DiagnosticPage<S>, StructuralError<S>> {
        validate::<S>(query.limit, &query.cursor)?;
        let mut hasher = DefaultHasher::new();
        ("diagnostics", query.limit).hash(&mut hasher);
        let scope = hasher.finish();
        let offset = self.offset(&query.cursor, scope, self.diagnostics.len())?;
        let mut page = DiagnosticPage {
            snapshot_id: self.snapshot_id.clone(),
            items: Vec::new(),
            total: self.diagnostics.len(),
            next_cursor: None,
        };
        for item in self.diagnostics.iter().skip(offset).take(query.limit) {
            page.items.push(item.clone());
            let next = offset + page.items.len();
            page.next_cursor = (next < page.total).then(|| self.cursor(scope, next));
            if !self.fits(&page) {
                page.items.pop();
                if page.items.is_empty() {
                    return Err(failure(
                        "view_limit",
                        "One diagnostic cannot fit in the response byte limit.",
                        Some(item.source.clone()),
                    ));
                }
                page.next_cursor = Some(self.cursor(scope, offset + page.items.len()));
                break;
            }
        }
        self.budget(page)
    }
}

impl<S> Application<S> {
    fn indexes(
        &self,
    ) -> &(
        std::collections::HashMap<String, usize>,
        std::collections::HashMap<String, usize>,
    ) {
        self.indexes.get_or_init(|| {
            (
                self.nodes
                    .iter()
                    .enumerate()
                    .map(|(i, n)| (n.row.id.clone(), i))
                    .collect(),
                self.nodes
                    .iter()
                    .enumerate()
                    .map(|(i, n)| (n.row.span_id.clone(), i))
                    .collect(),
            )
        })
    }
    pub fn index(&self, id: &str) -> Result<usize, StructuralError<S>> {
        self.indexes().0.get(id).copied().ok_or_else(|| {
            failure(
                "unknown_node",
                "The observation is unavailable in this snapshot.",
                None,
            )
        })
    }
    pub fn span_index(&self, id: &str) -> Result<usize, StructuralError<S>> {
        self.indexes().1.get(id).copied().ok_or_else(|| {
            failure(
                "unknown_span",
                "The span is unavailable in this snapshot.",
                None,
            )
        })
    }
}
