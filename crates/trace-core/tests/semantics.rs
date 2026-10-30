use serde_json::{Value, json};
use std::{fs, path::Path};
use trace_core::{
    DiagnosticQuery, ImportLimits, ObservationQuery, StructuralFilter, Trace, views::*,
};

fn recording(events: &[Value]) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    write_events(&directory.path().join("events.jsonl"), events);
    directory
}
fn write_events(path: &Path, events: &[Value]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        path,
        events.iter().map(|v| format!("{v}\n")).collect::<String>(),
    )
    .unwrap();
}
fn open(events: &[Value]) -> Trace {
    let directory = recording(events);
    Trace::open(directory.path(), ImportLimits::default()).unwrap()
}
fn rows(trace: &Trace) -> Vec<Observation> {
    trace
        .observations(&ObservationQuery::default())
        .unwrap()
        .items
}
fn start() -> Vec<Value> {
    vec![
        json!({"type":"session_start","ts":0}),
        json!({"type":"interaction_start","ts":1}),
        json!({"type":"turn_start","ts":2,"turnIndex":0}),
    ]
}
fn usage(input: u64, output: u64) -> Value {
    json!({"input":input,"output":output,"cacheRead":0,"cacheWrite":0})
}
fn turn(trace: &Trace) -> trace_core::views::NodeDetail {
    let row = rows(trace)
        .into_iter()
        .find(|r| r.kind == NodeKind::Turn)
        .unwrap();
    trace.node(&row.id).unwrap()
}

#[test]
fn reference_is_fourteen_observations_with_recovered_outcome_and_distinct_measurements() {
    let trace = Trace::open(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference"),
        ImportLimits::default(),
    )
    .unwrap();
    let overview = trace.overview().unwrap();
    assert_eq!(
        (
            overview.node_count,
            overview.generation_count,
            overview.tool_count
        ),
        (14, 4, 6)
    );
    assert_eq!(overview.usage.total_tokens, Some(680));
    assert_eq!(overview.usage.cost, None);
    let rows = rows(&trace);
    let interaction = rows
        .iter()
        .find(|r| r.kind == NodeKind::Interaction)
        .unwrap();
    assert_eq!(interaction.outcome, Outcome::Success);
    assert!(
        rows.iter()
            .any(|r| r.kind == NodeKind::Tool && r.outcome == Outcome::Error)
    );
    let detail = trace.node("n4").unwrap();
    assert_eq!(detail.observation.duration.value_ms, Some(900.0));
    assert_eq!(detail.request_to_headers.value_ms, Some(120.0));
    assert_eq!(detail.ttft.state, MeasurementState::Unknown);
    assert_eq!(
        detail.observation.source,
        SourceRef {
            file: "events.jsonl".into(),
            line: 4
        }
    );
    let metadata = [
        serde_json::to_string(&overview).unwrap(),
        serde_json::to_string(&rows).unwrap(),
        serde_json::to_string(&detail).unwrap(),
        serde_json::to_string(&trace.diagnostics(&Default::default()).unwrap()).unwrap(),
    ]
    .join("");
    assert!(!metadata.contains("FABRICATED_PRIVATE"));
    assert!(!metadata.contains("invalid.example"));
    assert!(
        trace
            .content("n4")
            .unwrap()
            .input
            .text
            .unwrap()
            .contains("FABRICATED_PRIVATE")
    );
}
#[test]
fn import_limits_accept_exact_values_and_reject_one_over_including_unknown_and_malformed() {
    for bytes in [
        b"{}\n".as_slice(),
        b"bad\n".as_slice(),
        b"{\"type\":\"future\"}\n".as_slice(),
    ] {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("events.jsonl"), bytes).unwrap();
        let exact = ImportLimits {
            total_bytes: bytes.len(),
            record_bytes: bytes.len(),
            records: 1,
            sources: 1,
            ..Default::default()
        };
        assert!(Trace::open(directory.path(), exact).is_ok());
        for (limits, code) in [
            (
                ImportLimits {
                    total_bytes: bytes.len() - 1,
                    ..exact
                },
                "input_byte_limit",
            ),
            (
                ImportLimits {
                    record_bytes: bytes.len() - 1,
                    ..exact
                },
                "record_byte_limit",
            ),
            (
                ImportLimits {
                    records: 0,
                    ..exact
                },
                "record_count_limit",
            ),
            (
                ImportLimits {
                    sources: 0,
                    ..exact
                },
                "source_limit",
            ),
        ] {
            assert_eq!(
                Trace::open(directory.path(), limits).err().unwrap().code,
                code
            );
        }
    }
    let directory = recording(&[json!({"type":"future"}), json!({"type":"future"})]);
    assert!(
        Trace::open(
            directory.path(),
            ImportLimits {
                records: 2,
                ..Default::default()
            }
        )
        .is_ok()
    );
    assert_eq!(
        Trace::open(
            directory.path(),
            ImportLimits {
                records: 1,
                ..Default::default()
            }
        )
        .err()
        .unwrap()
        .code,
        "record_count_limit"
    );
    fs::create_dir_all(directory.path().join("subagents/a")).unwrap();
    fs::write(directory.path().join("subagents/a/events.jsonl"), "").unwrap();
    assert_eq!(
        Trace::open(
            directory.path(),
            ImportLimits {
                sources: 2,
                ..Default::default()
            }
        )
        .unwrap()
        .overview()
        .unwrap()
        .source_count,
        2
    );
    assert!(matches!(
        Trace::open(
            directory.path(),
            ImportLimits {
                sources: 1,
                ..Default::default()
            }
        )
        .err()
        .unwrap()
        .code
        .as_str(),
        "source_limit" | "source_entry_limit"
    ));
}
#[test]
fn bounded_discovery_counts_unrecognized_entries_and_refuses_escaping_links() {
    let directory = recording(&[]);
    fs::create_dir(directory.path().join("subagents")).unwrap();
    for i in 0..3 {
        fs::write(directory.path().join(format!("subagents/ignored-{i}")), "").unwrap();
    }
    assert_eq!(
        Trace::open(
            directory.path(),
            ImportLimits {
                sources: 3,
                ..Default::default()
            }
        )
        .err()
        .unwrap()
        .code,
        "source_entry_limit"
    );
    #[cfg(unix)]
    {
        let outside = recording(&[]);
        let source = recording(&[]);
        fs::create_dir(source.path().join("subagents")).unwrap();
        std::os::unix::fs::symlink(outside.path(), source.path().join("subagents/escape")).unwrap();
        assert_eq!(
            Trace::open(source.path(), ImportLimits::default())
                .err()
                .unwrap()
                .code,
            "source_escape"
        );
        let source = recording(&[]);
        fs::remove_file(source.path().join("events.jsonl")).unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("events.jsonl"),
            source.path().join("events.jsonl"),
        )
        .unwrap();
        assert_eq!(
            Trace::open(source.path(), ImportLimits::default())
                .err()
                .unwrap()
                .code,
            "source_escape"
        );
    }
}
#[test]
fn structural_diagnostics_preserve_surrounding_records_and_never_echo_bodies() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("events.jsonl"), "{\"type\":\"session_start\"}\nSECRET malformed\n{\"type\":\"future\",\"body\":\"SECRET\"}\n{\"type\":\"session_shutdown\"}\n{\"secret\":\"SECRET").unwrap();
    let trace = Trace::open(directory.path(), ImportLimits::default()).unwrap();
    let diagnostic = trace.diagnostics(&DiagnosticQuery::default()).unwrap();
    for (code, line) in [
        ("malformed_record", 2),
        ("unknown_event", 3),
        ("incomplete_tail", 5),
    ] {
        assert!(
            diagnostic
                .items
                .iter()
                .any(|d| d.code == code && d.source.line == line)
        );
    }
    assert!(
        !serde_json::to_string(&diagnostic)
            .unwrap()
            .contains("SECRET")
    );
    assert_eq!(rows(&trace).len(), 1);
    assert!(Trace::open(&directory.path().join("missing"), ImportLimits::default()).is_err());
}
#[test]
fn reused_epochs_turns_and_tool_ids_do_not_overwrite_or_cross_occurrences() {
    let mut events = start();
    events.extend([
        json!({"type":"step_start","stepIndex":1}),
        json!({"type":"tool_start","toolCallId":"same","stepIndex":1}),
        json!({"type":"turn_start","turnIndex":0}),
        json!({"type":"tool_end","toolCallId":"same","stepIndex":1}),
        json!({"type":"session_start"}),
        json!({"type":"tool_end","toolCallId":"same","stepIndex":1}),
    ]);
    let trace = open(&events);
    let rows = rows(&trace);
    assert_eq!(
        rows.iter().filter(|r| r.kind == NodeKind::Session).count(),
        2
    );
    assert_eq!(rows.iter().filter(|r| r.kind == NodeKind::Turn).count(), 2);
    assert_eq!(rows.iter().filter(|r| r.kind == NodeKind::Tool).count(), 1);
    assert_eq!(
        trace
            .diagnostics(&Default::default())
            .unwrap()
            .items
            .iter()
            .filter(|d| d.code == "unmatched_tool_end")
            .count(),
        2
    );
    let ids: std::collections::HashSet<_> = rows.iter().map(|r| &r.id).collect();
    assert_eq!(ids.len(), rows.len());
}
#[test]
fn delayed_tool_end_uses_its_original_step_and_preserves_failure() {
    let mut events = start();
    events.extend([
        json!({"type":"step_start","stepIndex":1}),
        json!({"type":"tool_start","stepIndex":1,"toolCallId":"parallel","toolName":"bash"}),
        json!({"type":"step_start","stepIndex":2}),
        json!({"type":"tool_start","stepIndex":2,"toolCallId":"parallel"}),
        json!({"type":"tool_end","stepIndex":1,"toolCallId":"parallel","isError":true}),
        json!({"type":"tool_end","stepIndex":2,"toolCallId":"parallel","isError":false}),
    ]);
    let trace = open(&events);
    let tools: Vec<_> = rows(&trace)
        .into_iter()
        .filter(|r| r.kind == NodeKind::Tool)
        .collect();
    assert_eq!(tools[0].outcome, Outcome::Error);
    assert_eq!(tools[1].outcome, Outcome::Success);
    assert_ne!(tools[0].parent_id, tools[1].parent_id);
}
#[test]
fn explicit_children_link_by_ids_in_reverse_order_and_ambiguity_stays_unlinked() {
    let mut events = start();
    events.extend([json!({"type":"step_start","stepIndex":1}),json!({"type":"tool_start","toolCallId":"a","toolName":"subagent"}),json!({"type":"tool_start","toolCallId":"b","toolName":"subagent"}),json!({"type":"tool_end","toolCallId":"b","subagent":{"results":[{"childTraceId":"second"}]}}),json!({"type":"tool_end","toolCallId":"a","subagent":{"results":[{"childTraceId":"first"}]}})]);
    let directory = recording(&events);
    for child in ["first", "second", "unlinked"] {
        write_events(
            &directory
                .path()
                .join(format!("subagents/{child}/events.jsonl")),
            &[json!({"type":"session_start","sessionId":child})],
        );
    }
    let trace = Trace::open(directory.path(), ImportLimits::default()).unwrap();
    for row in rows(&trace)
        .iter()
        .filter(|r| r.kind == NodeKind::Session && r.source.file != "events.jsonl")
    {
        let detail = trace.node(&row.id).unwrap();
        if row.source.file.contains("unlinked") {
            assert_eq!(detail.relationship.state, RelationshipState::Unresolved);
            assert_eq!(detail.relationship.execution_parent_id, None);
        } else {
            assert_eq!(detail.relationship.state, RelationshipState::Linked);
            let parent = trace
                .node(detail.relationship.execution_parent_id.as_ref().unwrap())
                .unwrap();
            assert_eq!(
                parent.observation.id,
                if row.source.file.contains("first") {
                    "n5"
                } else {
                    "n6"
                }
            );
        }
    }
    events.push(json!({"type":"tool_start","toolCallId":"c","childTraceId":"first"}));
    write_events(&directory.path().join("events.jsonl"), &events);
    let trace = Trace::open(directory.path(), ImportLimits::default()).unwrap();
    let child = rows(&trace)
        .into_iter()
        .find(|r| r.source.file == "subagents/first/events.jsonl")
        .unwrap();
    assert_eq!(
        trace.node(&child.id).unwrap().relationship.state,
        RelationshipState::Ambiguous
    );
    assert_eq!(child.parent_id, None);
}
#[test]
fn usage_deduplicates_falls_back_and_preserves_partial_zero_and_unknown_cost() {
    let mut events = start();
    events.extend([
        json!({"type":"step_start"}),
        json!({"type":"step_end","usage":usage(100,50)}),
        json!({"type":"turn_summary","usage":usage(100,50)}),
    ]);
    let complete = turn(&open(&events)).usage;
    assert_eq!(complete.state, UsageState::Complete);
    assert_eq!(complete.total_tokens, Some(150));
    assert_eq!(complete.cache_read_tokens, Some(0));
    assert_eq!(complete.cache_write_tokens, Some(0));
    assert_eq!(complete.cost, None);
    events.insert(5, json!({"type":"step_start"}));
    events.insert(6, json!({"type":"step_end"}));
    let partial = turn(&open(&events)).usage;
    assert_eq!(partial.state, UsageState::Partial);
    assert_eq!(
        (partial.observed_generations, partial.total_generations),
        (1, 2)
    );
    let mut events = start();
    events.push(json!({"type":"turn_summary","usage":usage(0,0)}));
    let fallback = turn(&open(&events)).usage;
    assert_eq!(fallback.state, UsageState::Fallback);
    assert_eq!(fallback.total_tokens, Some(0));
    assert_eq!(fallback.cost, None);
    assert_eq!(fallback.fallback_source, Some(FallbackSource::TurnSummary));
    let mut events = start();
    events.extend([
        json!({"type":"step_start"}),
        json!({"type":"step_end","usage":{"cacheRead":5}}),
        json!({"type":"turn_summary","usage":usage(100,50)}),
    ]);
    let usage = turn(&open(&events)).usage;
    assert_eq!(usage.state, UsageState::Partial);
    assert_eq!(usage.cache_read_tokens, Some(5));
    assert_eq!(usage.total_tokens, None);
    assert_eq!(usage.fallback_source, None);
    let mut events = start();
    events.push(json!({"type":"turn_summary","usage":{"input":5}}));
    let partial = turn(&open(&events)).usage;
    assert_eq!(partial.state, UsageState::Partial);
    assert_eq!(partial.fallback_source, Some(FallbackSource::TurnSummary));
    let mut events = start();
    events.extend([
        json!({"type":"step_start"}),
        json!({"type":"step_end","usage":{"inputTokens":40,"outputTokens":10,"totalTokens":50,"cost":0}}),
    ]);
    let alias = turn(&open(&events)).usage;
    assert_eq!(alias.state, UsageState::Partial);
    assert_eq!(alias.input_tokens, Some(40));
    assert_eq!(alias.output_tokens, Some(10));
    assert_eq!(alias.total_tokens, Some(50));
    assert_eq!(alias.cache_read_tokens, None);
    assert_eq!(alias.cache_write_tokens, None);
    assert_eq!(alias.cost, Some(0.0));
    let mut events = start();
    events.extend([
        json!({"type":"step_start"}),
        json!({"type":"step_end","usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0}}),
    ]);
    let zero = turn(&open(&events)).usage;
    assert_eq!(zero.state, UsageState::Complete);
    assert_eq!(zero.total_tokens, Some(0));
    assert_eq!(zero.cost, None);
}
#[test]
fn unknown_and_invalid_intervals_have_no_fabricated_bars_or_success() {
    let mut events = start();
    events.extend([
        json!({"type":"step_start","ts":20}),
        json!({"type":"step_end","ts":10}),
        json!({"type":"turn_summary","stopReason":"stop"}),
        json!({"type":"turn_end","ts":30}),
    ]);
    let trace = open(&events);
    let generation = rows(&trace)
        .into_iter()
        .find(|r| r.kind == NodeKind::Generation)
        .unwrap();
    assert_eq!(generation.duration.state, MeasurementState::Invalid);
    assert!(generation.bar.is_none());
    assert!(
        trace
            .diagnostics(&Default::default())
            .unwrap()
            .items
            .iter()
            .any(|d| d.code == "invalid_interval")
    );
    let mut events = start();
    events.extend([
        json!({"type":"turn_summary","stopReason":"stop"}),
        json!({"type":"turn_end","ts":30}),
    ]);
    let trace = open(&events);
    assert_eq!(turn(&trace).observation.outcome, Outcome::Incomplete);
    assert_eq!(
        rows(&trace)
            .into_iter()
            .find(|r| r.kind == NodeKind::Interaction)
            .unwrap()
            .outcome,
        Outcome::Incomplete
    );
}
#[test]
fn content_is_bounded_utf8_and_recorded_utf16_truncation_is_respected() {
    let mut events = start();
    events.extend([
        json!({"type":"step_start"}),
        json!({"type":"llm_request","input":"Ж".repeat(5000)}),
        json!({"type":"tool_start","toolCallId":"emoji"}),
        json!({"type":"tool_end","toolCallId":"emoji","resultPreview":"😀","resultTotalLength":2}),
    ]);
    let trace = open(&events);
    let generation = rows(&trace)
        .into_iter()
        .find(|r| r.kind == NodeKind::Generation)
        .unwrap();
    let content = trace.content(&generation.id).unwrap();
    assert!(content.input.truncated);
    assert_eq!(content.input.text.unwrap().len(), 8192);
    assert_eq!(content.output.state, ContentState::Unavailable);
    let tool = rows(&trace)
        .into_iter()
        .find(|r| r.kind == NodeKind::Tool)
        .unwrap();
    assert!(!trace.content(&tool.id).unwrap().output.truncated);
}
#[test]
fn queries_keep_ancestors_and_bind_cursors_to_snapshot_query_and_page_size() {
    let trace = Trace::open(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference"),
        Default::default(),
    )
    .unwrap();
    let mut query = ObservationQuery {
        filter: Some(StructuralFilter::Error),
        limit: 2,
        ..Default::default()
    };
    let mut all = Vec::new();
    loop {
        let page = trace.observations(&query).unwrap();
        assert_eq!(page.total_matches, 1);
        all.extend(page.items);
        if page.next_cursor.is_none() {
            break;
        }
        query.cursor = page.next_cursor;
    }
    assert!(all.last().unwrap().r#match);
    assert!(all[..all.len() - 1].iter().all(|r| r.context));
    for (index, row) in all.iter().enumerate() {
        if let Some(parent) = &row.parent_id {
            assert!(all[..index].iter().any(|r| &r.id == parent));
        }
    }
    let first = trace
        .observations(&ObservationQuery {
            limit: 1,
            ..Default::default()
        })
        .unwrap();
    let cursor = first.next_cursor.unwrap();
    assert_eq!(
        trace
            .observations(&ObservationQuery {
                cursor: Some(cursor.clone()),
                limit: 2,
                ..Default::default()
            })
            .err()
            .unwrap()
            .code,
        "invalid_cursor"
    );
    let other = open(&start());
    assert_eq!(
        other
            .observations(&ObservationQuery {
                cursor: Some(cursor),
                limit: 1,
                ..Default::default()
            })
            .err()
            .unwrap()
            .code,
        "invalid_cursor"
    );
    assert_eq!(
        trace
            .observations(&ObservationQuery {
                q: "Ж".repeat(257),
                ..Default::default()
            })
            .err()
            .unwrap()
            .code,
        "query_limit"
    );
    assert!(
        trace
            .observations(&ObservationQuery {
                q: "Ж".repeat(256),
                ..Default::default()
            })
            .is_ok()
    );
    assert!(
        trace
            .observations(&ObservationQuery {
                limit: 0,
                ..Default::default()
            })
            .is_err()
    );
    assert!(
        trace
            .observations(&ObservationQuery {
                limit: 201,
                ..Default::default()
            })
            .is_err()
    );
    assert!(
        trace
            .observations(&ObservationQuery {
                cursor: Some("a".repeat(2049)),
                ..Default::default()
            })
            .is_err()
    );
    assert_eq!(
        trace
            .observations(&ObservationQuery {
                q: "FABRICATED_PRIVATE".into(),
                ..Default::default()
            })
            .unwrap()
            .total_matches,
        0
    );
}
#[test]
fn response_byte_budget_includes_envelope_and_continuation() {
    let directory = recording(&start());
    let trace = Trace::open(
        directory.path(),
        ImportLimits {
            response_bytes: 2500,
            ..Default::default()
        },
    )
    .unwrap();
    let mut query = ObservationQuery::default();
    let mut count = 0;
    let mut pages = 0;
    loop {
        let page = trace.observations(&query).unwrap();
        assert!(serde_json::to_vec(&page).unwrap().len() <= 2500);
        count += page.items.len();
        pages += 1;
        if page.next_cursor.is_none() {
            break;
        }
        query.cursor = page.next_cursor;
    }
    assert_eq!(count, 3);
    assert!(pages > 1);
    let trace = Trace::open(
        directory.path(),
        ImportLimits {
            response_bytes: 100,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        trace.observations(&Default::default()).err().unwrap().code,
        "view_limit"
    );
    assert_eq!(trace.overview().err().unwrap().code, "view_limit");
}

#[test]
fn ancestor_closure_exceeding_two_hundred_rows_is_appendable_in_preorder() {
    let directory = tempfile::tempdir().unwrap();
    let mut path = directory.path().to_path_buf();
    for depth in 0..45 {
        let mut events = start();
        events.extend([json!({"type":"step_start","stepIndex":1}),json!({"type":"tool_start","toolCallId":"child","toolName":if depth==44 {"needle"} else {"subagent"}})]);
        if depth < 44 {
            events.push(json!({"type":"tool_end","toolCallId":"child","subagent":{"results":[{"childTraceId":"a"}]}}));
        }
        write_events(&path.join("events.jsonl"), &events);
        path = path.join("subagents/a");
    }
    let trace = Trace::open(directory.path(), ImportLimits::default()).unwrap();
    let mut query = ObservationQuery {
        q: "needle".into(),
        limit: 100,
        ..Default::default()
    };
    let mut loaded = Vec::new();
    let mut pages = 0;
    loop {
        let page = trace.observations(&query).unwrap();
        assert_eq!(page.total_matches, 1);
        assert_eq!(page.total_rows, 225);
        assert!(page.items.len() <= 100);
        loaded.extend(page.items);
        pages += 1;
        if page.next_cursor.is_none() {
            break;
        }
        query.cursor = page.next_cursor;
    }
    assert_eq!(pages, 3);
    assert_eq!(loaded.len(), 225);
    assert_eq!(loaded.last().unwrap().depth, 224);
    assert!(loaded.last().unwrap().r#match);
    for (index, row) in loaded.iter().enumerate() {
        if let Some(parent) = &row.parent_id {
            assert!(
                loaded[..index]
                    .iter()
                    .any(|previous| &previous.id == parent)
            );
        }
    }
}
#[test]
fn diagnostic_pages_and_labels_obey_boundaries_without_shortening_sources() {
    let mut events = start();
    events.extend([
        json!({"type":"step_start"}),
        json!({"type":"tool_start","toolCallId":"long","toolName":"Ж".repeat(300)}),
    ]);
    let trace = open(&events);
    let tool = rows(&trace)
        .into_iter()
        .find(|row| row.kind == NodeKind::Tool)
        .unwrap();
    assert_eq!(tool.label.len(), 512);
    assert!(tool.label_truncated);
    assert_eq!(tool.source.file, "events.jsonl");
    let mut query = DiagnosticQuery {
        limit: 1,
        ..Default::default()
    };
    let mut diagnostics = Vec::new();
    loop {
        let page = trace.diagnostics(&query).unwrap();
        assert_eq!(page.items.len(), 1);
        diagnostics.extend(page.items);
        if page.next_cursor.is_none() {
            break;
        }
        query.cursor = page.next_cursor;
    }
    assert_eq!(diagnostics.len(), 5);
    let directory = recording(&events);
    let bounded = Trace::open(
        directory.path(),
        ImportLimits {
            response_bytes: 300,
            ..Default::default()
        },
    )
    .unwrap();
    let page = bounded.diagnostics(&Default::default()).unwrap();
    assert!(serde_json::to_vec(&page).unwrap().len() <= 300);
    assert!(page.next_cursor.is_some());
}

#[test]
fn final_summary_does_not_turn_a_tool_use_generation_into_final_success() {
    let mut events = start();
    events.extend([
        json!({"type":"step_start"}),
        json!({"type":"step_end","stopReason":"toolUse"}),
        json!({"type":"turn_summary","stopReason":"stop"}),
        json!({"type":"turn_end"}),
    ]);
    let trace = open(&events);
    assert_eq!(turn(&trace).observation.outcome, Outcome::Incomplete);
}
#[test]
fn structural_id_and_kind_are_searchable_without_content() {
    let trace = Trace::open(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference"),
        Default::default(),
    )
    .unwrap();
    for (q, total) in [("n4", 1), ("tool", 6)] {
        let page = trace
            .observations(&ObservationQuery {
                q: q.into(),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(page.total_matches, total);
    }
}

#[test]
fn collector_truncation_marker_can_equal_or_exceed_the_original_result_length() {
    for full_length in [501, 514] {
        let serialized = serde_json::to_string(&"x".repeat(full_length - 2)).unwrap();
        assert_eq!(serialized.encode_utf16().count(), full_length);
        let recorded_preview = format!("{}...[truncated]", &serialized[..500]);
        assert_eq!(recorded_preview.encode_utf16().count(), 514);
        let mut events = start();
        events.extend([
            json!({"type":"step_start"}),
            json!({"type":"tool_start","toolCallId":"marker"}),
            json!({"type":"tool_end","toolCallId":"marker","resultPreview":recorded_preview,"resultTotalLength":full_length}),
        ]);
        let trace = open(&events);
        let tool = rows(&trace)
            .into_iter()
            .find(|row| row.kind == NodeKind::Tool)
            .unwrap();
        let output = trace.content(&tool.id).unwrap().output;
        assert_eq!(output.text.as_deref(), Some(recorded_preview.as_str()));
        assert!(output.truncated, "original UTF-16 length {full_length}");
    }
}

#[test]
fn collector_marker_inside_an_untruncated_json_string_is_not_truncation() {
    let recorded_preview = serde_json::to_string("literal ...[truncated]").unwrap();
    let mut events = start();
    events.extend([
        json!({"type":"step_start"}),
        json!({"type":"tool_start","toolCallId":"literal"}),
        json!({"type":"tool_end","toolCallId":"literal","resultPreview":recorded_preview,"resultTotalLength":recorded_preview.encode_utf16().count()}),
    ]);
    let trace = open(&events);
    let tool = rows(&trace)
        .into_iter()
        .find(|row| row.kind == NodeKind::Tool)
        .unwrap();
    let output = trace.content(&tool.id).unwrap().output;
    assert_eq!(output.text.as_deref(), Some(recorded_preview.as_str()));
    assert!(!output.truncated);
}
