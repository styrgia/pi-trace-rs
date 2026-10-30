use crate::{model::Node, views::*};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Amount {
    Missing,
    Known(u64),
    Invalid,
}

#[derive(Clone, Debug)]
pub(crate) struct RecordedUsage {
    pub source: SourceRef,
    pub amounts: [Amount; 5],
    pub observed: bool,
    pub recorded_total: Amount,
    pub problems: Vec<&'static str>,
}

impl RecordedUsage {
    pub fn parse(value: Option<&Value>, source: SourceRef) -> Self {
        let fields = [
            value.and_then(|v| v.get("inputTokens").or_else(|| v.get("input"))),
            value.and_then(|v| v.get("outputTokens").or_else(|| v.get("output"))),
            value.and_then(|v| v.get("cacheRead")),
            value.and_then(|v| v.get("cacheWrite")),
            value.and_then(|v| v.get("totalTokens")),
        ];
        let observed =
            fields.iter().any(|v| v.is_some()) || value.is_some_and(|v| v.get("cost").is_some());
        let mut amounts = fields.map(|field| match field {
            None => Amount::Missing,
            Some(v) => v.as_u64().map_or(Amount::Invalid, Amount::Known),
        });
        let recorded_total = amounts[4].clone();
        let mut problems = Vec::new();
        if amounts.contains(&Amount::Invalid) {
            problems.push("invalid_usage_value");
        }
        if amounts[..4].iter().all(|a| matches!(a, Amount::Known(_))) {
            let sum = amounts[..4].iter().try_fold(0_u64, |sum, a| match a {
                Amount::Known(value) => sum.checked_add(*value),
                _ => None,
            });
            match (sum, &recorded_total) {
                (None, _) => {
                    amounts[4] = Amount::Invalid;
                    problems.push("usage_overflow");
                }
                (Some(total), Amount::Known(reported)) if total != *reported => {
                    amounts[4] = Amount::Invalid;
                    problems.push("conflicting_usage_total");
                }
                (Some(total), Amount::Missing) => amounts[4] = Amount::Known(total),
                _ => (),
            }
        }
        Self {
            source,
            amounts,
            observed,
            recorded_total,
            problems,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ContributionKind {
    Generation,
    TurnSummaryFallback,
}

#[derive(Clone, Debug)]
pub(crate) struct Contribution {
    pub id: String,
    pub owner: usize,
    pub kind: ContributionKind,
    pub usage: RecordedUsage,
}

pub(crate) fn contributions(nodes: &[Node]) -> Vec<Contribution> {
    let mut turns_with_usage = std::collections::HashSet::new();
    for node in nodes {
        if node.kind == NodeKind::Generation
            && node.generation_usage.as_ref().is_some_and(|u| u.observed)
            && let Some(turn) = node.parent
        {
            turns_with_usage.insert(turn);
        }
    }
    nodes
        .iter()
        .enumerate()
        .filter_map(|(owner, node)| {
            let (kind, usage) = match node.kind {
                NodeKind::Generation => (
                    ContributionKind::Generation,
                    node.generation_usage.as_ref()?,
                ),
                NodeKind::Turn if !turns_with_usage.contains(&owner) => (
                    ContributionKind::TurnSummaryFallback,
                    node.summary_evidence.as_ref()?,
                ),
                _ => return None,
            };
            usage.observed.then(|| Contribution {
                id: format!(
                    "{}-{}",
                    node.span_id,
                    if kind == ContributionKind::Generation {
                        "g"
                    } else {
                        "t"
                    }
                ),
                owner,
                kind,
                usage: usage.clone(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reducer::{Builder, Occurrence};
    use serde_json::json;

    fn record(builder: &mut Builder, occurrence: &mut Occurrence, line: usize, event: Value) {
        builder.record(
            occurrence,
            &serde_json::to_vec(&event).unwrap(),
            true,
            SourceRef {
                file: "events.jsonl".into(),
                line,
            },
            None,
        );
    }

    #[test]
    fn generation_evidence_replaces_fallback_including_partial_cost_only_and_invalid() {
        for usage in [
            json!({"totalTokens":120}),
            json!({"output":20}),
            json!({"cost":0.1}),
            json!({"input":-1}),
            json!({"input":0,"output":0,"cacheRead":0,"cacheWrite":0}),
        ] {
            let mut builder = Builder::new();
            let mut occurrence = Occurrence::default();
            record(
                &mut builder,
                &mut occurrence,
                1,
                json!({"type":"step_start"}),
            );
            record(
                &mut builder,
                &mut occurrence,
                2,
                json!({"type":"turn_summary","usage":{"totalTokens":150}}),
            );
            let before = contributions(&builder.nodes);
            assert_eq!(before.len(), 1);
            assert_eq!(before[0].kind, ContributionKind::TurnSummaryFallback);
            assert_eq!(before[0].usage.amounts[4], Amount::Known(150));
            record(
                &mut builder,
                &mut occurrence,
                3,
                json!({"type":"step_end","usage":usage}),
            );
            record(
                &mut builder,
                &mut occurrence,
                4,
                json!({"type":"step_end","usage":{"totalTokens":999}}),
            );
            let after = contributions(&builder.nodes);
            assert_eq!(after.len(), 1);
            assert_eq!(after[0].kind, ContributionKind::Generation);
            assert_eq!(after[0].usage.source.line, 3);
            assert_ne!(before[0].id, after[0].id);
            assert_eq!(after[0].owner, 3);
            assert_eq!(contributions(&builder.nodes)[0].id, after[0].id);
        }
    }

    #[test]
    fn invalid_values_conflicting_total_and_overflow_preserve_other_components() {
        let source = SourceRef {
            file: "events.jsonl".into(),
            line: 1,
        };
        for bad in [
            json!(-1),
            json!(0.5),
            json!("PRIVATE"),
            Value::Null,
            serde_json::from_str::<Value>("18446744073709551616").unwrap(),
        ] {
            let usage =
                RecordedUsage::parse(Some(&json!({"input":bad,"output":20})), source.clone());
            assert_eq!(usage.amounts[0], Amount::Invalid);
            assert_eq!(usage.amounts[1], Amount::Known(20));
        }
        let conflict = RecordedUsage::parse(
            Some(&json!({"input":60,"output":40,"cacheRead":0,"cacheWrite":0,"totalTokens":90})),
            source.clone(),
        );
        assert_eq!(conflict.amounts[4], Amount::Invalid);
        let overflow = RecordedUsage::parse(
            Some(&json!({"input":u64::MAX,"output":1,"cacheRead":0,"cacheWrite":0})),
            source.clone(),
        );
        assert_eq!(overflow.amounts[4], Amount::Invalid);
        assert_eq!(overflow.amounts[0], Amount::Known(u64::MAX));
        let exact =
            RecordedUsage::parse(Some(&json!({"totalTokens":9007199254740993_u64})), source);
        assert_eq!(exact.amounts[4], Amount::Known(9007199254740993));
    }
    #[test]
    fn verified_child_usage_is_inclusive_once_and_overview_is_independent() {
        let mut builder = Builder::new();
        let mut parent = Occurrence::default();
        record(&mut builder, &mut parent, 1, json!({"type":"step_start"}));
        record(
            &mut builder,
            &mut parent,
            2,
            json!({"type":"step_end","usage":{"totalTokens":150}}),
        );
        record(
            &mut builder,
            &mut parent,
            3,
            json!({"type":"tool_start","toolCallId":"child"}),
        );
        let mut child = Occurrence::default();
        for (line, event) in [
            json!({"type":"step_start"}),
            json!({"type":"step_end","usage":{"totalTokens":40}}),
        ]
        .into_iter()
        .enumerate()
        {
            builder.record(
                &mut child,
                &serde_json::to_vec(&event).unwrap(),
                true,
                SourceRef {
                    file: "subagents/a/events.jsonl".into(),
                    line: line + 1,
                },
                Some("subagents/a/events.jsonl"),
            );
        }
        let before = builder.project(false);
        assert_eq!(
            before.token_accounting.total_tokens.value.as_deref(),
            Some("190")
        );
        assert_eq!(
            before.nodes[3]
                .span_usage
                .inclusive
                .total_tokens
                .value
                .as_deref(),
            Some("150")
        );
        record(
            &mut builder,
            &mut parent,
            4,
            json!({"type":"tool_end","toolCallId":"child","childTraceId":"a"}),
        );
        let mut projected = builder.project(false);
        let overview = apply(&mut projected.nodes).overview;
        assert_eq!(overview.total_tokens.value.as_deref(), Some("190"));
        assert_eq!(
            projected.nodes[3]
                .span_usage
                .own
                .total_tokens
                .value
                .as_deref(),
            Some("150")
        );
        assert_eq!(
            projected.nodes[3]
                .span_usage
                .inclusive
                .total_tokens
                .value
                .as_deref(),
            Some("190")
        );
        assert_eq!(
            projected.nodes[4]
                .span_usage
                .inclusive
                .total_tokens
                .value
                .as_deref(),
            Some("40")
        );
        assert_eq!(
            projected.nodes[4].span_usage.own.total_tokens.state,
            TokenState::NotApplicable
        );
        record(
            &mut builder,
            &mut parent,
            5,
            json!({"type":"tool_start","toolCallId":"other","childTraceId":"a"}),
        );
        let ambiguous = builder.project(false);
        assert_eq!(
            ambiguous.token_accounting.total_tokens.value.as_deref(),
            Some("190")
        );
        assert_eq!(
            ambiguous.nodes[3]
                .span_usage
                .inclusive
                .total_tokens
                .value
                .as_deref(),
            Some("150")
        );
        assert_eq!(
            ambiguous.nodes[4].span_usage.child_attribution,
            ChildAttribution::Ambiguous
        );
        assert_eq!(
            ambiguous.nodes[4].span_usage.inclusive.total_tokens.state,
            TokenState::Unknown
        );
    }
    #[test]
    fn cyclic_execution_candidate_is_excluded_without_double_counting() {
        let mut builder = Builder::new();
        let mut occurrence = Occurrence::default();
        for (line, event) in [
            json!({"type":"step_start"}),
            json!({"type":"step_end","usage":{"totalTokens":150}}),
            json!({"type":"tool_start","childTraceId":"a","toolCallId":"cycle"}),
        ]
        .into_iter()
        .enumerate()
        {
            builder.record(
                &mut occurrence,
                &serde_json::to_vec(&event).unwrap(),
                true,
                SourceRef {
                    file: "events.jsonl".into(),
                    line: line + 1,
                },
                Some("subagents/a/events.jsonl"),
            );
        }
        let projected = builder.project(false);
        assert_eq!(projected.nodes[0].parent, None);
        assert_eq!(
            projected.nodes[0].relationship,
            RelationshipState::Ambiguous
        );
        assert_eq!(
            projected.token_accounting.total_tokens.value.as_deref(),
            Some("150")
        );
        assert_eq!(
            projected.nodes[4].span_usage.inclusive.total_tokens.state,
            TokenState::Unknown
        );
    }
    #[test]
    fn reused_turn_and_step_indices_have_distinct_canonical_contributions() {
        let mut builder = Builder::new();
        let mut occurrence = Occurrence::default();
        for turn in 0..2 {
            record(
                &mut builder,
                &mut occurrence,
                turn * 3 + 1,
                json!({"type":"turn_start","turnIndex":1}),
            );
            record(
                &mut builder,
                &mut occurrence,
                turn * 3 + 2,
                json!({"type":"step_start","stepIndex":1}),
            );
            record(
                &mut builder,
                &mut occurrence,
                turn * 3 + 3,
                json!({"type":"step_end","usage":{"totalTokens":150}}),
            );
        }
        let accepted = contributions(&builder.nodes);
        assert_eq!(accepted.len(), 2);
        assert_ne!(accepted[0].id, accepted[1].id);
        assert!(
            accepted
                .iter()
                .all(|c| c.usage.amounts[4] == Amount::Known(150))
        );
    }
}

pub(crate) fn empty_accounting() -> TokenAccounting {
    let counter = TokenCounter {
        state: TokenState::Unknown,
        value: None,
    };
    TokenAccounting {
        input_tokens: counter.clone(),
        output_tokens: counter.clone(),
        cache_read_tokens: counter.clone(),
        cache_write_tokens: counter.clone(),
        total_tokens: counter,
        provenance: UsageProvenance::None,
        observed_generations: 0,
        total_generations: 0,
        fallback_contributions: 0,
        contributor_count: 0,
    }
}

pub(crate) fn empty_span_usage() -> SpanUsage {
    SpanUsage {
        own: empty_accounting(),
        inclusive: empty_accounting(),
        child_attribution: ChildAttribution::NotApplicable,
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Counter {
    state: TokenState,
    value: Option<u64>,
}
impl Counter {
    pub(crate) fn none() -> Self {
        Self {
            state: TokenState::NotApplicable,
            value: None,
        }
    }
    pub(crate) fn from_amount(amount: &Amount, fallback: bool) -> Self {
        match amount {
            Amount::Missing => Self {
                state: TokenState::Unknown,
                value: None,
            },
            Amount::Invalid => Self {
                state: TokenState::Invalid,
                value: None,
            },
            Amount::Known(value) => Self {
                state: if fallback {
                    TokenState::Partial
                } else {
                    TokenState::Exact
                },
                value: Some(*value),
            },
        }
    }
    pub(crate) fn merge(&mut self, other: Self) -> bool {
        if other.state == TokenState::NotApplicable {
            return false;
        }
        if self.state == TokenState::NotApplicable {
            *self = other;
            return false;
        }
        if self.state == TokenState::Invalid || other.state == TokenState::Invalid {
            self.state = TokenState::Invalid;
            self.value = None;
            return false;
        }
        let incomplete = self.state != TokenState::Exact || other.state != TokenState::Exact;
        self.value = match (self.value, other.value) {
            (Some(a), Some(b)) => match a.checked_add(b) {
                Some(sum) => Some(sum),
                None => {
                    self.state = TokenState::Invalid;
                    self.value = None;
                    return true;
                }
            },
            (a, b) => a.or(b),
        };
        self.state = if self.value.is_none() {
            TokenState::Unknown
        } else if incomplete {
            TokenState::Partial
        } else {
            TokenState::Exact
        };
        false
    }
    pub(crate) fn view(self) -> TokenCounter {
        TokenCounter {
            state: self.state,
            value: self.value.map(|n| n.to_string()),
        }
    }
}
#[derive(Clone)]
pub(crate) struct Tally {
    counters: [Counter; 5],
    observed: usize,
    expected: usize,
    contributors: usize,
    fallback: usize,
    kinds: u8,
}
impl Tally {
    pub(crate) fn empty() -> Self {
        Self {
            counters: [Counter::none(); 5],
            observed: 0,
            expected: 0,
            contributors: 0,
            fallback: 0,
            kinds: 0,
        }
    }
    pub(crate) fn contribution(amounts: &[Amount; 5], partial: bool, kind: u8) -> Self {
        Self {
            counters: amounts.each_ref().map(|a| Counter::from_amount(a, partial)),
            observed: usize::from(kind == 1 || kind == 4),
            expected: usize::from(kind == 4),
            contributors: 1,
            fallback: usize::from(kind == 2),
            kinds: kind,
        }
    }
    pub(crate) fn expected(&mut self) {
        self.expected += 1;
        self.applicable();
    }

    pub(crate) fn merge(&mut self, other: &Self) -> bool {
        let mut overflow = false;
        for (counter, value) in self.counters.iter_mut().zip(other.counters) {
            overflow |= counter.merge(value);
        }
        self.observed += other.observed;
        self.expected += other.expected;
        self.contributors += other.contributors;
        self.fallback += other.fallback;
        self.kinds |= other.kinds;
        overflow
    }
    pub(crate) fn partial(&mut self) {
        for counter in &mut self.counters {
            if counter.state == TokenState::Exact {
                counter.state = TokenState::Partial;
            }
        }
    }
    pub(crate) fn applicable(&mut self) {
        for counter in &mut self.counters {
            if counter.state == TokenState::NotApplicable {
                counter.state = TokenState::Unknown;
            }
        }
    }
    pub(crate) fn view(&self, aggregate: bool) -> TokenAccounting {
        let [
            input_tokens,
            output_tokens,
            cache_read_tokens,
            cache_write_tokens,
            total_tokens,
        ] = self.counters.map(Counter::view);
        TokenAccounting {
            input_tokens,
            output_tokens,
            cache_read_tokens,
            cache_write_tokens,
            total_tokens,
            provenance: match (self.kinds, aggregate || self.contributors > 1) {
                (0, _) => UsageProvenance::None,
                (3, _) => UsageProvenance::Mixed,
                (_, true) => UsageProvenance::Aggregate,
                (1, _) => UsageProvenance::RecordedGeneration,
                (4, _) => UsageProvenance::NativeResponse,
                (8, _) => UsageProvenance::NativeAuxiliary,
                (16, _) => UsageProvenance::DerivedCumulative,
                _ => UsageProvenance::TurnSummaryFallback,
            },
            observed_generations: self.observed,
            total_generations: self.expected,
            fallback_contributions: self.fallback,
            contributor_count: self.contributors,
        }
    }
}

pub(crate) struct AccountingProjection {
    pub overview: TokenAccounting,
    pub overflow_nodes: Vec<usize>,
}

pub(crate) fn apply(nodes: &mut [Node]) -> AccountingProjection {
    let accepted = contributions(nodes);
    let mut own = vec![Tally::empty(); nodes.len()];
    for (i, node) in nodes.iter().enumerate() {
        if node.kind == NodeKind::Generation {
            own[i].expected = 1;
            own[i].applicable();
        }
    }
    for node in nodes.iter_mut() {
        node.contribution_id = None;
    }
    for contribution in accepted {
        nodes[contribution.owner].contribution_id = Some(contribution.id.clone());
        let tally = &mut own[contribution.owner];
        let fallback = contribution.kind == ContributionKind::TurnSummaryFallback;
        let expected = tally.expected;
        tally.merge(&Tally::contribution(
            &contribution.usage.amounts,
            fallback,
            if fallback { 2 } else { 1 },
        ));
        if expected > 0 {
            tally.counters = contribution
                .usage
                .amounts
                .each_ref()
                .map(|a| Counter::from_amount(a, fallback));
        }
    }
    let mut attribution = vec![ChildAttribution::NotApplicable; nodes.len()];
    let mut sessions: std::collections::HashMap<&str, Vec<usize>> =
        std::collections::HashMap::new();
    for (i, node) in nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.kind == NodeKind::Session)
    {
        sessions.entry(&node.sources[0].file).or_default().push(i);
    }
    for (i, node) in nodes.iter().enumerate() {
        for id in &node.child_ids {
            let prefix = node.sources[0]
                .file
                .strip_suffix("events.jsonl")
                .unwrap_or("");
            let path = format!("{prefix}subagents/{id}/events.jsonl");
            let state = match sessions.get(path.as_str()) {
                Some(found)
                    if !found.is_empty()
                        && found.iter().all(|child| {
                            nodes[*child].parent == Some(i)
                                && nodes[*child].relationship == RelationshipState::Linked
                        }) =>
                {
                    ChildAttribution::Complete
                }
                Some(found)
                    if found.len() > 1
                        || found.iter().any(|child| {
                            nodes[*child].relationship == RelationshipState::Ambiguous
                        }) =>
                {
                    ChildAttribution::Ambiguous
                }
                _ => ChildAttribution::Unresolved,
            };
            attribution[i] = merge_attribution(attribution[i], state);
        }
    }
    let parents: Vec<_> = nodes.iter().map(|n| n.parent).collect();
    let applicable: Vec<_> = nodes.iter().map(|n| n.kind != NodeKind::Tool).collect();
    let (spans, overview, overflow_nodes) =
        project_tallies(own, &parents, &applicable, attribution);
    for (node, span) in nodes.iter_mut().zip(spans) {
        node.span_usage = span;
    }
    AccountingProjection {
        overview,
        overflow_nodes,
    }
}
pub(crate) fn project_tallies(
    own: Vec<Tally>,
    parents: &[Option<usize>],
    applicable: &[bool],
    mut attribution: Vec<ChildAttribution>,
) -> (Vec<SpanUsage>, TokenAccounting, Vec<usize>) {
    let mut children = vec![Vec::new(); parents.len()];
    for (i, parent) in parents.iter().enumerate() {
        if let Some(p) = parent {
            children[*p].push(i);
        }
    }
    let mut order = Vec::with_capacity(parents.len());
    let mut stack: Vec<_> = parents
        .iter()
        .enumerate()
        .filter_map(|(i, p)| p.is_none().then_some(i))
        .collect();
    while let Some(i) = stack.pop() {
        order.push(i);
        stack.extend(children[i].iter().copied());
    }
    let mut inclusive = own.clone();
    let mut overflow_nodes = Vec::new();
    for i in order.into_iter().rev() {
        if matches!(
            attribution[i],
            ChildAttribution::Unresolved | ChildAttribution::Ambiguous
        ) {
            inclusive[i].applicable();
            inclusive[i].partial();
        }
        if applicable[i] {
            inclusive[i].applicable();
        }
        if let Some(parent) = parents[i] {
            let child = inclusive[i].clone();
            if inclusive[parent].merge(&child) {
                overflow_nodes.push(parent);
            }
            attribution[parent] = merge_attribution(attribution[parent], attribution[i]);
        }
    }
    let mut overview = Tally::empty();
    for (i, tally) in own.iter().enumerate() {
        if overview.merge(tally) {
            overflow_nodes.push(i);
        }
    }
    overview.applicable();
    if attribution.iter().any(|s| {
        matches!(
            s,
            ChildAttribution::Unresolved | ChildAttribution::Ambiguous
        )
    }) {
        overview.partial();
    }
    let spans = (0..parents.len())
        .map(|i| SpanUsage {
            own: own[i].view(false),
            inclusive: inclusive[i].view(inclusive[i].contributors > own[i].contributors),
            child_attribution: attribution[i],
        })
        .collect();
    overflow_nodes.sort_unstable();
    overflow_nodes.dedup();
    (spans, overview.view(true), overflow_nodes)
}

fn merge_attribution(a: ChildAttribution, b: ChildAttribution) -> ChildAttribution {
    use ChildAttribution::*;
    match (a, b) {
        (Ambiguous, _) | (_, Ambiguous) => Ambiguous,
        (Unresolved, _) | (_, Unresolved) => Unresolved,
        (Complete, _) | (_, Complete) => Complete,
        _ => NotApplicable,
    }
}

pub(crate) fn detail(nodes: &[Node], index: usize) -> SpanUsageDetail {
    let mut evidence = Vec::new();
    let mut pending = vec![index];
    while let Some(i) = pending.pop() {
        let node = &nodes[i];
        if node.span_usage.own.contributor_count > 0 {
            let usage = if node.kind == NodeKind::Generation {
                node.generation_usage.as_ref()
            } else {
                node.summary_evidence.as_ref()
            };
            if let Some(usage) = usage {
                evidence.push(UsageEvidenceEntry {
                    contribution_id: node
                        .contribution_id
                        .clone()
                        .expect("accepted contribution identity"),
                    span_id: node.span_id.clone(),
                    source: usage.source.clone(),
                    accounting: node.span_usage.own.clone(),
                    reported_total: Counter::from_amount(&usage.recorded_total, false).view(),
                });
                if evidence.len() == 16 {
                    break;
                }
            }
        }
        pending.extend(node.children.iter().rev().copied());
    }
    let contributor_count = nodes[index].span_usage.inclusive.contributor_count;
    SpanUsageDetail {
        summary: nodes[index].span_usage.clone(),
        omitted_count: contributor_count.saturating_sub(evidence.len()),
        contributor_count,
        evidence,
    }
}

pub(crate) fn context_input(usage: Option<&RecordedUsage>) -> TokenCounter {
    let Some(usage) = usage else {
        return Counter {
            state: TokenState::Unknown,
            value: None,
        }
        .view();
    };
    let components = [&usage.amounts[0], &usage.amounts[2], &usage.amounts[3]];
    if components.contains(&&Amount::Invalid) {
        return Counter {
            state: TokenState::Invalid,
            value: None,
        }
        .view();
    }
    if components
        .iter()
        .any(|amount| !matches!(amount, Amount::Known(_)))
    {
        return Counter {
            state: TokenState::Unknown,
            value: None,
        }
        .view();
    }
    let sum = components
        .iter()
        .try_fold(0_u64, |sum, amount| match amount {
            Amount::Known(value) => sum.checked_add(*value),
            _ => None,
        });
    Counter {
        state: if sum.is_some() {
            TokenState::Exact
        } else {
            TokenState::Invalid
        },
        value: sum,
    }
    .view()
}
