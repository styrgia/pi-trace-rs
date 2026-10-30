use super::{types::*, usage::*};
use crate::{
    DiagnosticQuery, ObservationQuery,
    accounting::{self, Tally},
    model::{empty_usage, unavailable, unknown},
    projection::{Application, ApplicationNode},
    views::*,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeDetail {
    #[serde(flatten)]
    pub detail: NodeDetail<NativeSource>,
    pub native_usage: Vec<NativeUsageEvidence>,
    pub relationships: Vec<NativeRelationship>,
    pub timing_evidence: Vec<TimingEvidence>,
    pub selected_context: Option<String>,
}
#[derive(Clone)]
pub(crate) struct Entity {
    pub key: String,
    pub kind: NodeKind,
    pub label: String,
    pub source: NativeSource,
    pub sources: Vec<NativeSource>,
    pub metadata: Metadata,
    pub duration: Measurement,
    pub outcome: Outcome,
    pub parent: Option<String>,
    pub ambiguous_parent: bool,
    pub input: ContentField,
    pub input_source: Option<NativeSource>,
    pub output_source: Option<NativeSource>,
    pub call_id: Option<String>,
    pub output: ContentField,
    pub relationships: Vec<NativeRelationship>,
    pub timing: Vec<TimingEvidence>,
}
impl Entity {
    pub fn new(key: &str, kind: NodeKind, label: &str, source: NativeSource) -> Self {
        Self {
            key: key.into(),
            kind,
            label: crate::bounded(label, DISPLAY_BYTES).0,
            source: source.clone(),
            sources: vec![source],
            metadata: Metadata {
                session_id: None,
                model: None,
                tool_name: None,
                turn_index: None,
                step_index: None,
            },
            duration: unknown("execution duration is not persisted"),
            outcome: Outcome::Unknown,
            parent: None,
            ambiguous_parent: false,
            input: unavailable(),
            input_source: None,
            output_source: None,
            call_id: key.strip_prefix("tool:").map(str::to_owned),
            output: unavailable(),
            relationships: Vec::new(),
            timing: Vec::new(),
        }
    }
}
pub(crate) fn content(value: Option<&Value>) -> ContentField {
    let Some(v) = value else { return unavailable() };
    let text = v
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| v.to_string());
    let (text, truncated) = crate::bounded(&text, CONTENT_BYTES);
    ContentField {
        state: ContentState::Available,
        text: Some(text),
        truncated,
        limit_bytes: CONTENT_BYTES,
    }
}
fn tool_inspection(entity: &Entity) -> ToolInspection<NativeSource> {
    let preview = |field: &ContentField, source: &Option<NativeSource>| PreviewEvidence {
        availability: field.state,
        selected_size: None,
        preview_utf8_bytes: field.text.as_ref().map(String::len),
        source: source.clone(),
        local_truncated: field.truncated,
        source_truncation: SourceTruncation::Unknown,
    };
    ToolInspection {
        retention: Retention::Complete,
        call_id: entity.call_id.clone(),
        call_id_truncated: false,
        call_id_source: entity.call_id.as_ref().map(|_| entity.sources[0].clone()),
        arguments: preview(&entity.input, &entity.input_source),
        result: ResultEvidence {
            preview: preview(&entity.output, &entity.output_source),
            recorded_field: None,
            original_length: None,
        },
    }
}
#[derive(Clone)]
pub(crate) struct SessionState {
    pub descriptor: SessionDescriptor,
    pub run_id: String,
    pub entities: BTreeMap<String, Entity>,
    pub ledger: Ledger,
    pub diagnostics: Vec<NativeDiagnostic>,
    pub records: usize,
    pub selected_context: Option<String>,
    pub cumulative: HashMap<String, (Value, NativeSource)>,
    pub ordinal: u64,
    pub recorded_ordinal: Option<u64>,
    pub inherited_before: Option<u64>,
    pub attribution_partial: bool,
    pub source_names: std::collections::BTreeSet<String>,
}
impl SessionState {
    pub fn new(descriptor: SessionDescriptor) -> Self {
        let attribution_partial = descriptor.provider == Provider::Pi
            && descriptor.relationships.iter().any(|r| r.kind == "fork");
        Self {
            descriptor,
            attribution_partial,
            source_names: Default::default(),
            run_id: crate::RunId::new().as_str().to_owned(),
            entities: BTreeMap::new(),
            ledger: Ledger::default(),
            diagnostics: Vec::new(),
            records: 0,
            selected_context: None,
            cumulative: HashMap::new(),
            ordinal: 0,
            recorded_ordinal: None,
            inherited_before: None,
        }
    }
    pub fn diagnostic(&mut self, code: &str, source: NativeSource) {
        self.diagnostics.push(NativeDiagnostic {
            id: format!("d{}", self.diagnostics.len() + 1),
            code: code.into(),
            message: match code {
                "malformed_record" => "A complete record is malformed.",
                "unsupported_record" => "This persisted record variant is not supported.",
                "usage_discontinuity" => {
                    "Cumulative usage cannot be compared across this boundary."
                }
                _ => "Native evidence is incomplete or incompatible.",
            }
            .into(),
            source,
            node_id: None,
        });
    }
    pub fn put(&mut self, mut entity: Entity) {
        entity.metadata.session_id = Some(self.descriptor.native_id.clone());
        if entity.kind == NodeKind::Tool
            && let Some(previous) = self.entities.get(&entity.key)
        {
            if previous.parent.is_some()
                && entity.parent.is_some()
                && previous.parent != entity.parent
            {
                entity.parent = None;
                entity.ambiguous_parent = true;
            }
            entity.ambiguous_parent |= previous.ambiguous_parent;
            if entity.ambiguous_parent {
                entity.parent = None;
            }
            if entity.output.state == ContentState::Unavailable {
                entity.output = previous.output.clone();
                entity.output_source = previous.output_source.clone();
                if let Some(source) = &entity.output_source
                    && !entity.sources.contains(source)
                {
                    entity.sources.push(source.clone());
                }
                if entity.outcome == Outcome::Unknown {
                    entity.outcome = previous.outcome;
                }
            }
            if entity.duration.state == MeasurementState::Unknown {
                entity.duration = previous.duration.clone();
            }
        }
        self.entities.insert(entity.key.clone(), entity);
    }
    pub fn claim(
        &mut self,
        key: &str,
        owner: &str,
        usage: &Value,
        source: NativeSource,
        category: &str,
        priority: u8,
    ) {
        let role = UsageRole::Contribution {
            category: category.into(),
        };
        let mut c = UsageClaim::parse(self.descriptor.provider, owner, key, usage, source, role);
        c.priority = priority;
        c.ordinal = self.recorded_ordinal;
        c.partial = self.attribution_partial;
        if let Some(boundary) = self.inherited_before {
            match c.ordinal {
                Some(ordinal) if ordinal < boundary => {
                    c.view.role = UsageRole::Inherited {
                        origin: "recordedForkPrefix".into(),
                    }
                }
                None => c.partial = true,
                _ => {}
            }
        }
        self.ledger.put(key.into(), c);
    }
    pub fn project(&self, revision: &SourceRevision, limits: NativeLimits) -> SessionSnapshot {
        let root = "session";
        let mut root_entity = Entity::new(
            root,
            NodeKind::Session,
            "Session",
            self.descriptor.source.clone(),
        );
        root_entity.relationships = self.descriptor.relationships.clone();
        let mut entities = vec![root_entity];
        entities.extend(self.entities.values().cloned());
        let index: HashMap<_, _> = entities
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let mut parents = vec![None; entities.len()];
        let mut attribution = vec![ChildAttribution::NotApplicable; entities.len()];
        for (i, e) in entities.iter().enumerate().skip(1) {
            if e.ambiguous_parent {
                attribution[i] = ChildAttribution::Ambiguous;
            }
            if let Some(p) = &e.parent {
                if let Some(&target) = index.get(p) {
                    if target != i {
                        parents[i] = Some(target);
                    } else {
                        attribution[i] = ChildAttribution::Ambiguous;
                    }
                } else {
                    attribution[i] = ChildAttribution::Unresolved;
                }
            }
        }
        let mut colors = vec![0u8; parents.len()];
        for start in 0..parents.len() {
            if colors[start] != 0 {
                continue;
            }
            let mut path = Vec::new();
            let mut cursor = Some(start);
            while let Some(i) = cursor {
                if colors[i] == 2 {
                    break;
                }
                if colors[i] == 1 {
                    parents[i] = None;
                    attribution[i] = ChildAttribution::Ambiguous;
                    break;
                }
                colors[i] = 1;
                path.push(i);
                cursor = parents[i];
            }
            for i in path {
                colors[i] = 2;
            }
        }
        let accepted = self.ledger.selected();
        let mut own = vec![Tally::empty(); entities.len()];
        let mut samples = vec![Vec::<usize>::new(); entities.len()];
        let mut has_accepted = vec![false; entities.len()];
        let mut inherited = vec![false; entities.len()];
        let mut explanations = vec![Vec::<NativeUsageEvidence>::new(); entities.len()];
        let accepted_sources: std::collections::HashSet<_> = accepted
            .iter()
            .map(|c| {
                (
                    c.view.id.clone(),
                    serde_json::to_string(&c.view.source).unwrap(),
                )
            })
            .collect();
        for (claim_index, c) in accepted.iter().enumerate() {
            if let Some(&i) = index.get(&c.owner) {
                own[i].merge(&c.tally());
                has_accepted[i] = true;
                if samples[i].len() < 16 {
                    samples[i].push(claim_index);
                }
            }
        }
        for c in self.ledger.claims.values() {
            if let Some(&i) = index.get(&c.owner) {
                inherited[i] |= matches!(c.view.role, UsageRole::Inherited { .. });
                let mut view = c.view.clone();
                view.accounting = c.tally().view(false);
                view.accepted = accepted_sources.contains(&(
                    view.id.clone(),
                    serde_json::to_string(&view.source).unwrap(),
                ));
                if explanations[i].len() < 16 {
                    explanations[i].push(view.clone());
                }
                if i != 0 && explanations[0].len() < 16 {
                    explanations[0].push(view);
                }
            }
        }
        for (i, e) in entities.iter().enumerate() {
            if e.kind == NodeKind::Generation && !has_accepted[i] && !inherited[i] {
                own[i].expected();
            }
        }
        let mut children = vec![Vec::new(); entities.len()];
        for (i, parent) in parents.iter().enumerate() {
            if let Some(p) = parent {
                children[*p].push(i);
            }
        }
        let mut traversal = Vec::new();
        let mut pending: Vec<_> = parents
            .iter()
            .enumerate()
            .filter_map(|(i, p)| p.is_none().then_some(i))
            .collect();
        while let Some(i) = pending.pop() {
            traversal.push(i);
            pending.extend(children[i].iter().rev().copied());
        }
        for i in traversal.into_iter().rev() {
            if let Some(parent) = parents[i] {
                let room = 16usize.saturating_sub(samples[parent].len());
                let child = samples[i].iter().take(room).copied().collect::<Vec<_>>();
                samples[parent].extend(child);
            }
        }
        samples[0] = (0..accepted.len().min(16)).collect();
        let display_parents: Vec<_> = parents
            .iter()
            .enumerate()
            .map(|(i, p)| p.or((i != 0).then_some(0)))
            .collect();
        let mut display_children = vec![Vec::new(); entities.len()];
        for (i, parent) in display_parents.iter().enumerate() {
            if let Some(p) = parent {
                display_children[*p].push(i);
            }
        }
        let mut order = Vec::new();
        let mut depth = vec![0usize; entities.len()];
        let mut pending = vec![0];
        while let Some(i) = pending.pop() {
            order.push(i);
            for child in display_children[i].iter().rev() {
                depth[*child] = depth[i] + 1;
                pending.push(*child);
            }
        }
        let (mut spans, accounting, _) = accounting::project_tallies(
            own,
            &parents,
            &entities
                .iter()
                .map(|e| e.kind != NodeKind::Tool)
                .collect::<Vec<_>>(),
            attribution.clone(),
        );
        spans[0].inclusive = accounting.clone();
        let snapshot_id = format!(
            "{}-{}",
            self.run_id,
            identity(&[revision.as_str(), &self.records.to_string()])
        );
        let mut nodes = Vec::new();
        let mut details = Vec::new();
        let mut contents = Vec::new();
        for (i, e) in entities.iter().enumerate() {
            let id = format!("n{}", identity(&[&self.run_id, &e.key]));
            let span_id = format!("s{}", identity(&[&self.run_id, &e.key]));
            let parent = display_parents[i];
            let row = Observation {
                id: id.clone(),
                span_id: span_id.clone(),
                span_usage: spans[i].clone(),
                kind: e.kind,
                label: e.label.clone(),
                label_truncated: false,
                parent_id: parent
                    .map(|p| format!("n{}", identity(&[&self.run_id, &entities[p].key]))),
                depth: depth[i],
                has_children: !display_children[i].is_empty(),
                r#match: true,
                context: false,
                outcome: e.outcome,
                duration: e.duration.clone(),
                bar: None,
                source: e.source.clone(),
            };
            let evidence = samples[i]
                .iter()
                .map(|&j| {
                    let c = &accepted[j];
                    UsageEvidenceEntry {
                        contribution_id: c.view.id.clone(),
                        span_id: format!("s{}", identity(&[&self.run_id, &c.owner])),
                        source: c.view.source.clone(),
                        accounting: c.tally().view(false),
                        reported_total: accounting::Counter::from_amount(&c.reported_total, false)
                            .view(),
                    }
                })
                .collect::<Vec<_>>();
            let native_usage = explanations[i].clone();
            details.push(NativeDetail {
                detail: NodeDetail {
                    snapshot_id: snapshot_id.clone(),
                    span_usage: SpanUsageDetail {
                        summary: spans[i].clone(),
                        contributor_count: spans[i].inclusive.contributor_count,
                        omitted_count: spans[i]
                            .inclusive
                            .contributor_count
                            .saturating_sub(evidence.len()),
                        evidence,
                    },
                    observation: row.clone(),
                    relationship: Relationship {
                        state: if e.ambiguous_parent {
                            RelationshipState::Ambiguous
                        } else if parents[i].is_some() {
                            RelationshipState::Linked
                        } else {
                            RelationshipState::Root
                        },
                        execution_parent_id: parents[i]
                            .map(|p| format!("n{}", identity(&[&self.run_id, &entities[p].key]))),
                    },
                    request_to_headers: unknown("request to headers is not persisted"),
                    ttft: unknown("first-token latency is not persisted"),
                    usage: empty_usage(0),
                    metadata: e.metadata.clone(),
                    context: None,
                    tool_inspection: (e.kind == NodeKind::Tool).then(|| tool_inspection(e)),
                    sources: e.sources.clone(),
                    diagnostic_ids: Vec::new(),
                },
                native_usage,
                relationships: e.relationships.clone(),
                timing_evidence: e.timing.clone(),
                selected_context: self.selected_context.clone(),
            });
            contents.push(NodeContent {
                snapshot_id: snapshot_id.clone(),
                node_id: id,
                input: e.input.clone(),
                output: e.output.clone(),
            });
            nodes.push(ApplicationNode {
                row,
                metadata: e.metadata.clone(),
                parent,
            });
        }
        let count = |k| entities.iter().filter(|e| e.kind == k).count();
        let overview = Overview {
            snapshot_id: snapshot_id.clone(),
            title: format!("{} session", self.descriptor.provider),
            source: Some(self.descriptor.source.clone()),
            model: entities.iter().find_map(|e| e.metadata.model.clone()),
            source_count: self.source_names.len().max(1),
            record_count: self.records,
            node_count: entities.len(),
            session_count: 1,
            interaction_count: 0,
            turn_count: 0,
            generation_count: count(NodeKind::Generation),
            tool_count: count(NodeKind::Tool),
            diagnostic_count: self.diagnostics.len(),
            duration: unknown("session execution interval is not persisted"),
            usage: empty_usage(0),
            token_accounting: accounting,
            timeline: None,
            measurement_availability: MeasurementAvailability {
                duration: entities.iter().any(|e| e.duration.value_ms.is_some()),
                request_to_headers: false,
                ttft: false,
            },
        };
        SessionSnapshot {
            run_id: self.run_id.clone(),
            application: Application {
                indexes: std::sync::OnceLock::new(),
                snapshot_id,
                nodes,
                order,
                diagnostics: self.diagnostics.clone(),
                response_bytes: limits.import.response_bytes,
            },
            overview,
            details,
            contents,
        }
    }
}
pub struct SessionSnapshot {
    run_id: String,
    application: Application<NativeSource>,
    overview: Overview<NativeSource>,
    details: Vec<NativeDetail>,
    contents: Vec<NodeContent>,
}
impl SessionSnapshot {
    pub fn run_id(&self) -> &str {
        &self.run_id
    }
    pub fn snapshot_id(&self) -> &str {
        &self.application.snapshot_id
    }
    pub fn overview(&self) -> Result<Overview<NativeSource>, NativeError> {
        self.application
            .budget(self.overview.clone())
            .map_err(Box::new)
    }
    pub fn observations(
        &self,
        q: &ObservationQuery,
    ) -> Result<ObservationPage<NativeSource>, NativeError> {
        self.application.observations(q).map_err(Box::new)
    }
    pub fn diagnostics(
        &self,
        q: &DiagnosticQuery,
    ) -> Result<DiagnosticPage<NativeSource>, NativeError> {
        self.application.diagnostics(q).map_err(Box::new)
    }
    pub fn node(&self, id: &str) -> Result<NativeDetail, NativeError> {
        let i = self.application.index(id)?;
        self.application
            .budget(self.details[i].clone())
            .map_err(Box::new)
    }
    pub fn span(&self, id: &str) -> Result<NativeDetail, NativeError> {
        let i = self.application.span_index(id)?;
        self.application
            .budget(self.details[i].clone())
            .map_err(Box::new)
    }
    pub fn content(&self, id: &str) -> Result<NodeContent, NativeError> {
        let i = self.application.index(id)?;
        self.application
            .budget(self.contents[i].clone())
            .map_err(Box::new)
    }
}
