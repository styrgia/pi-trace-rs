use serde_json::{Value, json};
use std::{fs, path::Path};
use trace_core::{ImportLimits, ObservationQuery, StructuralFilter, Trace, views::*};

fn write(root: &Path, file: &str, events: &[Value]) {
    let path = root.join(file);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let text: String = events.iter().map(|v| format!("{v}\n")).collect();
    fs::write(path, text).unwrap();
}
fn generations(usages: &[Value]) -> Vec<Value> {
    usages
        .iter()
        .flat_map(|usage| {
            [
                json!({"type":"step_start"}),
                json!({"type":"step_end","usage":usage}),
            ]
        })
        .collect()
}
fn open(events: &[Value]) -> Trace {
    let directory = tempfile::tempdir().unwrap();
    write(directory.path(), "events.jsonl", events);
    Trace::open(directory.path(), ImportLimits::default()).unwrap()
}

fn contract(name: &str, mut detail: NodeDetail) {
    detail.snapshot_id = "synthetic-accounting-snapshot".into();
    let original = detail.observation.span_id.clone();
    detail.observation.span_id = format!("synthetic-span-{}", detail.observation.id);
    for (i, entry) in detail.span_usage.evidence.iter_mut().enumerate() {
        entry.span_id = if entry.span_id == original {
            detail.observation.span_id.clone()
        } else {
            format!("synthetic-descendant-{i}")
        };
        entry.contribution_id = format!("synthetic-contribution-{i}");
    }
    let serialized = format!("{}\n", serde_json::to_string_pretty(&detail).unwrap());
    assert!(serialized.len() <= 1024 * 1024);
    assert!(!serialized.contains("FABRICATED_PRIVATE_USAGE"));
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("tests/contracts/detail-accounting-{name}.json"));
    if std::env::var_os("UPDATE_CONTRACTS").is_some() {
        fs::write(&path, &serialized).unwrap();
    }
    assert_eq!(fs::read_to_string(path).unwrap(), serialized);
}

#[test]
fn ordinary_and_unresolved_tools_keep_applicability_separate() {
    let trace = open(&[
        json!({"type":"tool_start","toolCallId":"ordinary"}),
        json!({"type":"tool_start","toolCallId":"missing","childTraceId":"absent"}),
    ]);
    let page = trace
        .observations(&ObservationQuery {
            filter: Some(StructuralFilter::Tool),
            ..Default::default()
        })
        .unwrap();
    let tools: Vec<_> = page
        .items
        .iter()
        .filter(|row| row.kind == NodeKind::Tool)
        .collect();
    assert_eq!(
        tools[0].span_usage.inclusive.total_tokens.state,
        TokenState::NotApplicable
    );
    assert_eq!(
        tools[1].span_usage.own.total_tokens.state,
        TokenState::NotApplicable
    );
    assert_eq!(
        tools[1].span_usage.inclusive.total_tokens.state,
        TokenState::Unknown
    );
    assert_eq!(
        tools[1].span_usage.child_attribution,
        ChildAttribution::Unresolved
    );
}

#[test]
fn exact_components_partial_evidence_and_invalid_totals_remain_independent() {
    for (case, (usage, total, state, diagnostic)) in [
        (
            json!({"input":0,"output":0,"cacheRead":0,"cacheWrite":0}),
            Some("0"),
            TokenState::Exact,
            None,
        ),
        (
            json!({"totalTokens":70}),
            Some("70"),
            TokenState::Exact,
            None,
        ),
        (
            json!({"input":100,"output":20}),
            None,
            TokenState::Unknown,
            None,
        ),
        (json!({"cost":0.1}), None, TokenState::Unknown, None),
        (
            json!({"input":-1,"output":20}),
            None,
            TokenState::Unknown,
            Some("invalid_usage_value"),
        ),
        (
            json!({"input":60,"output":40,"cacheRead":0,"cacheWrite":0,"totalTokens":90}),
            None,
            TokenState::Invalid,
            Some("conflicting_usage_total"),
        ),
        (
            json!({"input":u64::MAX,"output":1,"cacheRead":0,"cacheWrite":0}),
            None,
            TokenState::Invalid,
            Some("usage_overflow"),
        ),
        (
            json!({"totalTokens":9007199254740993_u64}),
            Some("9007199254740993"),
            TokenState::Exact,
            None,
        ),
        (
            json!({"totalTokens":u64::MAX}),
            Some("18446744073709551615"),
            TokenState::Exact,
            None,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let cost_only = usage == json!({"cost":0.1});
        let trace = open(&generations(&[usage]));
        let detail = trace.node("n4").unwrap();
        contract(&format!("counter-{case}"), detail.clone());
        let counter = &detail.span_usage.summary.own.total_tokens;
        assert_eq!(counter.state, state);
        assert_eq!(counter.value.as_deref(), total);
        assert_eq!(
            trace
                .overview()
                .unwrap()
                .token_accounting
                .total_tokens
                .value
                .as_deref(),
            total
        );
        if let Some(code) = diagnostic {
            assert!(
                trace
                    .diagnostics(&Default::default())
                    .unwrap()
                    .items
                    .iter()
                    .any(|d| d.code == code && d.source.line == 2)
            );
        }
        if state == TokenState::Invalid
            && detail.span_usage.evidence[0]
                .reported_total
                .value
                .as_deref()
                == Some("90")
        {
            assert_eq!(
                detail.span_usage.summary.own.input_tokens.value.as_deref(),
                Some("60")
            );
        }
        assert_eq!(detail.usage.cost, if cost_only { Some(0.1) } else { None });
    }
}

#[test]
fn aggregate_overflow_does_not_wrap_or_destroy_independent_counters() {
    let trace = open(&generations(&[
        json!({"totalTokens":u64::MAX,"output":1}),
        json!({"totalTokens":1,"output":2}),
    ]));
    let overview = trace.overview().unwrap();
    assert_eq!(
        overview.token_accounting.total_tokens.state,
        TokenState::Invalid
    );
    assert_eq!(overview.token_accounting.total_tokens.value, None);
    assert_eq!(
        overview.token_accounting.output_tokens.value.as_deref(),
        Some("3")
    );
    assert!(
        trace
            .diagnostics(&Default::default())
            .unwrap()
            .items
            .iter()
            .any(|d| d.code == "aggregate_usage_overflow")
    );
}

#[test]
fn contributor_evidence_is_capped_and_queries_do_not_change_totals() {
    let trace = open(&generations(&vec![json!({"totalTokens":1}); 20]));
    let before = serde_json::to_value(trace.overview().unwrap().token_accounting).unwrap();
    let detail = trace.node("n1").unwrap();
    assert_eq!(detail.span_usage.contributor_count, 20);
    assert_eq!(detail.span_usage.evidence.len(), 16);
    assert_eq!(detail.span_usage.omitted_count, 4);
    contract("evidence-cap", detail.clone());
    for filter in [
        None,
        Some(StructuralFilter::Generation),
        Some(StructuralFilter::Tool),
    ] {
        let mut query = ObservationQuery {
            filter,
            limit: 2,
            ..Default::default()
        };
        loop {
            let page = trace.observations(&query).unwrap();
            for row in &page.items {
                assert_eq!(
                    serde_json::to_value(&row.span_usage).unwrap(),
                    serde_json::to_value(trace.node(&row.id).unwrap().span_usage.summary).unwrap()
                );
            }
            query.cursor = page.next_cursor;
            if query.cursor.is_none() {
                break;
            }
        }
    }
    assert_eq!(
        serde_json::to_value(trace.overview().unwrap().token_accounting).unwrap(),
        before
    );
    assert_eq!(before["totalTokens"]["value"], "20");
}

#[test]
fn all_epochs_in_one_child_recording_have_the_same_confirmed_attribution() {
    let directory = tempfile::tempdir().unwrap();
    write(
        directory.path(),
        "events.jsonl",
        &[json!({"type":"tool_start","toolCallId":"a","childTraceId":"child"})],
    );
    write(
        directory.path(),
        "subagents/child/events.jsonl",
        &[
            json!({"type":"session_start"}),
            json!({"type":"step_end","usage":{"totalTokens":20}}),
            json!({"type":"session_start"}),
            json!({"type":"step_end","usage":{"totalTokens":20}}),
        ],
    );
    let trace = Trace::open(directory.path(), Default::default()).unwrap();
    let rows = trace.observations(&Default::default()).unwrap();
    let tool = rows
        .items
        .iter()
        .find(|row| row.kind == NodeKind::Tool)
        .unwrap();
    assert_eq!(
        tool.span_usage.child_attribution,
        ChildAttribution::Complete
    );
    assert_eq!(
        tool.span_usage.inclusive.total_tokens.value.as_deref(),
        Some("40")
    );
}

#[test]
fn known_subtotals_are_partial_when_generation_usage_is_missing() {
    let trace = open(&generations(&[json!({"totalTokens":150}), json!({})]));
    let accounting = trace.overview().unwrap().token_accounting;
    assert_eq!(accounting.total_tokens.value.as_deref(), Some("150"));
    assert_eq!(accounting.total_tokens.state, TokenState::Partial);
    assert_eq!(accounting.observed_generations, 1);
    assert_eq!(accounting.total_generations, 2);
}
