use serde_json::{Value, json};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{Seek, SeekFrom, Write},
    path::Path,
};
use trace_core::{FollowReader, ImportLimits, Trace, views::*};

fn write(root: &Path, file: &str, events: &[Value]) {
    let path = root.join(file);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        path,
        events
            .iter()
            .map(|event| format!("{event}\n"))
            .collect::<String>(),
    )
    .unwrap();
}
fn append(path: &Path, bytes: &[u8]) {
    OpenOptions::new()
        .append(true)
        .open(path)
        .unwrap()
        .write_all(bytes)
        .unwrap();
}
fn rows(trace: &Trace) -> Vec<Observation> {
    trace.observations(&Default::default()).unwrap().items
}

fn source_anchored_views(trace: &Trace) -> Value {
    let observations = rows(trace);
    let diagnostics = trace.diagnostics(&Default::default()).unwrap();
    let mut identifiers = HashMap::new();
    for row in &observations {
        identifiers.insert(
            row.span_id.clone(),
            format!(
                "span:{}:{}:{:?}",
                row.source.file, row.source.line, row.kind
            ),
        );
    }
    for diagnostic in &diagnostics.items {
        identifiers.insert(
            diagnostic.id.clone(),
            format!(
                "diagnostic:{}:{}:{}:{:?}",
                diagnostic.source.file, diagnostic.source.line, diagnostic.code, diagnostic.node_id
            ),
        );
    }
    let details: Vec<_> = observations
        .iter()
        .map(|row| trace.node(&row.id).unwrap())
        .collect();
    for detail in &details {
        for evidence in &detail.span_usage.evidence {
            identifiers.insert(
                evidence.contribution_id.clone(),
                format!(
                    "contribution:{}:{}:{}",
                    evidence.source.file, evidence.source.line, identifiers[&evidence.span_id]
                ),
            );
        }
    }
    let mut value = json!({
        "overview": trace.overview().unwrap(),
        "observations": observations,
        "details": details,
        "content": observations.iter().map(|row| trace.content(&row.id).unwrap()).collect::<Vec<_>>(),
        "diagnostics": diagnostics,
    });
    fn normalize(value: &mut Value, identifiers: &HashMap<String, String>) {
        match value {
            Value::String(text) => {
                if let Some(replacement) = identifiers.get(text) {
                    *text = replacement.clone();
                }
            }
            Value::Array(items) => {
                for item in items {
                    normalize(item, identifiers);
                }
            }
            Value::Object(fields) => {
                for (key, item) in fields {
                    if key == "snapshotId" {
                        *item = json!("normalized-snapshot");
                    } else {
                        normalize(item, identifiers);
                    }
                }
            }
            _ => {}
        }
    }
    normalize(&mut value, &identifiers);
    value
}

#[test]
fn settled_live_and_offline_views_agree_without_changing_source_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("events.jsonl");
    write(directory.path(), "events.jsonl", &[]);
    let mut reader = FollowReader::open(directory.path(), Default::default()).unwrap();
    reader.update().unwrap();
    let events = [
        json!({"type":"session_start","ts":0,"sessionId":"synthetic-comparison"}),
        json!({"type":"interaction_start","ts":1,"prompt":"SYNTHETIC_PRIVATE_COMPARISON"}),
        json!({"type":"turn_start","ts":2,"turnIndex":1}),
        json!({"type":"step_start","ts":3,"stepIndex":1}),
        json!({"type":"llm_request","ts":3,"input":[{"role":"user","content":"SYNTHETIC_PRIVATE_COMPARISON"}]}),
        json!({"type":"llm_response","ts":5,"durationMs":2}),
        json!({"type":"step_end","ts":13,"durationMs":10,"stopReason":"toolUse","usage":{"totalTokens":150}}),
        json!({"type":"step_end","ts":14,"usage":{"totalTokens":999}}),
        json!({"type":"tool_start","ts":15,"toolCallId":"a","childTraceId":"a","toolName":"subagent"}),
        json!({"type":"tool_end","ts":25,"toolCallId":"a","durationMs":10,"isError":false}),
        json!({"type":"turn_summary","ts":26,"usage":{"totalTokens":150},"stopReason":"toolUse"}),
        json!({"type":"turn_end","ts":26,"turnIndex":1}),
        json!({"type":"turn_start","ts":30,"turnIndex":2}),
        json!({"type":"step_start","ts":31,"stepIndex":1}),
        json!({"type":"step_end","ts":32,"durationMs":1,"usage":{"input":10,"output":20}}),
        json!({"type":"turn_summary","ts":33,"usage":{"totalTokens":900},"stopReason":"stop"}),
        json!({"type":"turn_end","ts":33,"turnIndex":2}),
        json!({"type":"turn_start","ts":40,"turnIndex":3}),
        json!({"type":"step_start","ts":41,"stepIndex":1}),
        json!({"type":"step_end","ts":42,"durationMs":1}),
        json!({"type":"turn_summary","ts":43,"usage":{"totalTokens":40},"stopReason":"stop"}),
        json!({"type":"turn_end","ts":43,"turnIndex":3}),
        json!({"type":"session_shutdown","ts":44}),
    ];
    for event in events {
        append(&path, format!("{event}\n").as_bytes());
        reader.update().unwrap();
    }
    write(
        directory.path(),
        "subagents/a/events.jsonl",
        &[
            json!({"type":"session_start","ts":16}),
            json!({"type":"interaction_start","ts":17}),
            json!({"type":"turn_start","ts":18}),
            json!({"type":"step_start","ts":19}),
            json!({"type":"step_end","ts":23,"durationMs":4,"usage":{"input":9007199254740993_u64,"output":0,"cacheRead":0,"cacheWrite":0}}),
            json!({"type":"turn_end","ts":24}),
            json!({"type":"session_shutdown","ts":24}),
        ],
    );
    let child = directory.path().join("subagents/a/events.jsonl");
    let before = (fs::read(&path).unwrap(), fs::read(&child).unwrap());
    let live = reader.update().unwrap().snapshot.unwrap();
    let offline = Trace::open(directory.path(), Default::default()).unwrap();
    assert_ne!(live.run_id(), offline.run_id());
    assert_eq!(
        source_anchored_views(&live),
        source_anchored_views(&offline)
    );
    for _ in 0..20 {
        assert!(reader.update().unwrap().snapshot.is_none());
    }
    assert_eq!(before, (fs::read(path).unwrap(), fs::read(child).unwrap()));
}

#[test]
fn complete_malformed_and_unknown_lines_consume_cumulative_budgets_without_body_leaks() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("events.jsonl");
    fs::write(&path, b"").unwrap();
    let mut reader = FollowReader::open(
        directory.path(),
        ImportLimits {
            records: 2,
            ..Default::default()
        },
    )
    .unwrap();
    reader.update().unwrap();
    append(&path, b"PRIVATE_NOT_JSON\n{\"type\":\"unsupported\"}\n");
    let accepted = reader.update().unwrap().snapshot.unwrap();
    assert_eq!(accepted.overview().unwrap().record_count, 2);
    let diagnostics = accepted.diagnostics(&Default::default()).unwrap();
    assert_eq!(diagnostics.items[0].code, "malformed_record");
    assert_eq!(diagnostics.items[0].source.line, 1);
    assert_eq!(diagnostics.items[1].source.line, 2);
    assert!(
        !serde_json::to_string(&diagnostics)
            .unwrap()
            .contains("PRIVATE_NOT_JSON")
    );
    for _ in 0..20 {
        assert!(reader.update().unwrap().snapshot.is_none());
    }
    append(&path, b"{}\n");
    assert_eq!(reader.update().err().unwrap().code, "record_count_limit");
    assert_eq!(reader.progress().record_count, 2);
    assert_eq!(accepted.overview().unwrap().record_count, 2);
}

#[test]
fn byte_limits_include_pending_bytes_and_rejected_batches_do_not_advance_cursors() {
    for (limit, expected) in [
        ("record", "record_byte_limit"),
        ("input", "input_byte_limit"),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("events.jsonl");
        fs::write(&path, b"").unwrap();
        let limits = if limit == "record" {
            ImportLimits {
                record_bytes: 4,
                ..Default::default()
            }
        } else {
            ImportLimits {
                total_bytes: 4,
                ..Default::default()
            }
        };
        let mut reader = FollowReader::open(directory.path(), limits).unwrap();
        reader.update().unwrap();
        append(&path, b"{{{{");
        let pending = reader.update().unwrap();
        assert!(pending.snapshot.is_none());
        assert_eq!(pending.progress.pending_bytes, 4);
        assert_eq!(pending.progress.read_bytes, 4);
        for _ in 0..20 {
            assert!(reader.update().unwrap().snapshot.is_none());
        }
        append(&path, b"{");
        assert_eq!(reader.update().err().unwrap().code, expected);
        assert_eq!(reader.progress().read_bytes, 4);
        assert_eq!(reader.progress().record_count, 0);
        fs::write(&path, b"").unwrap();
        assert_eq!(reader.update().err().unwrap().code, expected);
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("events.jsonl");
    fs::write(&path, b"").unwrap();
    let mut reader = FollowReader::open(
        directory.path(),
        ImportLimits {
            record_bytes: 5,
            ..Default::default()
        },
    )
    .unwrap();
    let original = reader.update().unwrap().snapshot.unwrap();
    append(&path, b"{}\n123456\n");
    assert_eq!(reader.update().err().unwrap().code, "record_byte_limit");
    assert_eq!(reader.progress().record_count, 0);
    assert_eq!(reader.progress().read_bytes, 0);
    assert_eq!(original.overview().unwrap().record_count, 0);
}

#[test]
fn child_before_link_and_late_ambiguity_preserve_span_identity_and_canonical_totals() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("events.jsonl");
    write(
        directory.path(),
        "events.jsonl",
        &[
            json!({"type":"step_start"}),
            json!({"type":"step_end","usage":{"totalTokens":150}}),
        ],
    );
    let mut reader = FollowReader::open(directory.path(), Default::default()).unwrap();
    reader.update().unwrap();
    fs::create_dir_all(directory.path().join("subagents/a")).unwrap();
    assert!(reader.update().unwrap().snapshot.is_none());
    write(
        directory.path(),
        "subagents/a/events.jsonl",
        &[
            json!({"type":"step_start"}),
            json!({"type":"step_end","usage":{"totalTokens":40}}),
        ],
    );
    let before = reader.update().unwrap().snapshot.unwrap();
    let child = rows(&before)
        .into_iter()
        .find(|row| row.kind == NodeKind::Session && row.source.file != "events.jsonl")
        .unwrap();
    assert_eq!(
        before.node(&child.id).unwrap().relationship.state,
        RelationshipState::Unresolved
    );
    assert_eq!(
        before
            .overview()
            .unwrap()
            .token_accounting
            .total_tokens
            .value
            .as_deref(),
        Some("190")
    );
    append(
        &path,
        b"{\"type\":\"tool_start\",\"toolCallId\":\"a\",\"childTraceId\":\"a\"}\n",
    );
    let linked = reader.update().unwrap().snapshot.unwrap();
    let same = rows(&linked)
        .into_iter()
        .find(|row| row.span_id == child.span_id)
        .unwrap();
    assert_eq!(
        linked.node(&same.id).unwrap().relationship.state,
        RelationshipState::Linked
    );
    assert_eq!(
        linked
            .node("n4")
            .unwrap()
            .span_usage
            .summary
            .inclusive
            .total_tokens
            .value
            .as_deref(),
        Some("190")
    );
    assert_eq!(
        linked
            .overview()
            .unwrap()
            .token_accounting
            .total_tokens
            .value
            .as_deref(),
        Some("190")
    );
    append(
        &path,
        b"{\"type\":\"tool_start\",\"toolCallId\":\"other\",\"childTraceId\":\"a\"}\n",
    );
    let ambiguous = reader.update().unwrap().snapshot.unwrap();
    let same = rows(&ambiguous)
        .into_iter()
        .find(|row| row.span_id == child.span_id)
        .unwrap();
    assert_eq!(
        ambiguous.node(&same.id).unwrap().relationship.state,
        RelationshipState::Ambiguous
    );
    assert_eq!(
        ambiguous
            .overview()
            .unwrap()
            .token_accounting
            .total_tokens
            .value
            .as_deref(),
        Some("190")
    );
    assert_eq!(
        ambiguous
            .node("n4")
            .unwrap()
            .span_usage
            .summary
            .inclusive
            .total_tokens
            .value
            .as_deref(),
        Some("150")
    );
}

#[test]
fn source_replacement_truncation_same_size_rewrite_and_boundary_changes_stop_the_run() {
    for action in ["replace", "truncate", "rewrite", "boundary", "disappear"] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("events.jsonl");
        fs::write(&path, b"{\"type\":\"session_start\"}\n").unwrap();
        let mut reader = FollowReader::open(directory.path(), Default::default()).unwrap();
        let before = reader.update().unwrap().snapshot.unwrap();
        let old_run = reader.run_id().to_owned();
        match action {
            "replace" => {
                let replacement = directory.path().join("replacement");
                fs::write(&replacement, b"{\"type\":\"session_start\"}\n").unwrap();
                fs::rename(replacement, &path).unwrap();
            }
            "truncate" => fs::write(&path, b"").unwrap(),
            "rewrite" => {
                let mut bytes = fs::read(&path).unwrap();
                bytes[2] = b'X';
                fs::write(&path, bytes).unwrap();
                File::options()
                    .write(true)
                    .open(&path)
                    .unwrap()
                    .set_modified(std::time::UNIX_EPOCH)
                    .unwrap();
            }
            "boundary" => {
                let mut file = OpenOptions::new().write(true).open(&path).unwrap();
                file.seek(SeekFrom::Start(0)).unwrap();
                file.write_all(b"X").unwrap();
                append(&path, b"{}\n");
            }
            _ => fs::remove_file(&path).unwrap(),
        }
        let error = reader
            .update()
            .err()
            .unwrap_or_else(|| panic!("{action} was not detected"));
        assert!(
            matches!(error.code.as_str(), "source_changed" | "source_unreadable"),
            "{action}: {}",
            error.code
        );
        assert_eq!(reader.progress().record_count, 1);
        assert_eq!(before.overview().unwrap().record_count, 1);
        fs::write(&path, b"{\"type\":\"session_start\"}\n").unwrap();
        assert_eq!(reader.update().err().unwrap().code, error.code);
        let mut reset = FollowReader::open(directory.path(), Default::default()).unwrap();
        assert_ne!(reset.run_id(), old_run);
        assert_eq!(
            reset
                .update()
                .unwrap()
                .snapshot
                .unwrap()
                .overview()
                .unwrap()
                .record_count,
            1
        );
    }
}

#[test]
fn discovery_and_input_limits_are_not_replenished_by_idle_scans_or_ignored_removal() {
    let directory = tempfile::tempdir().unwrap();
    write(directory.path(), "events.jsonl", &[]);
    fs::create_dir_all(directory.path().join("subagents")).unwrap();
    fs::write(directory.path().join("subagents/ignored-a"), b"").unwrap();
    fs::write(directory.path().join("subagents/ignored-b"), b"").unwrap();
    let mut reader = FollowReader::open(
        directory.path(),
        ImportLimits {
            sources: 3,
            ..Default::default()
        },
    )
    .unwrap();
    reader.update().unwrap();
    for _ in 0..10 {
        reader.update().unwrap();
    }
    fs::remove_file(directory.path().join("subagents/ignored-a")).unwrap();
    fs::remove_file(directory.path().join("subagents/ignored-b")).unwrap();
    fs::write(directory.path().join("subagents/ignored-c"), b"").unwrap();
    assert_eq!(reader.update().err().unwrap().code, "source_entry_limit");
}

#[cfg(unix)]
#[test]
fn root_retargeting_escaping_child_and_discovery_cycles_fail_structurally() {
    use std::os::unix::fs::symlink;
    for action in ["root", "escape", "cycle"] {
        let directory = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        write(directory.path(), "recording/events.jsonl", &[]);
        write(outside.path(), "events.jsonl", &[]);
        symlink(
            directory.path().join("recording"),
            directory.path().join("selected"),
        )
        .unwrap();
        let mut reader =
            FollowReader::open(&directory.path().join("selected"), Default::default()).unwrap();
        reader.update().unwrap();
        if action == "root" {
            fs::remove_file(directory.path().join("selected")).unwrap();
            symlink(outside.path(), directory.path().join("selected")).unwrap();
        } else {
            fs::create_dir_all(directory.path().join("recording/subagents")).unwrap();
            symlink(
                if action == "escape" {
                    outside.path().to_owned()
                } else {
                    directory.path().join("recording")
                },
                directory.path().join("recording/subagents/child"),
            )
            .unwrap();
        }
        let code = reader.update().err().unwrap().code;
        assert_eq!(
            code,
            match action {
                "root" => "source_changed",
                "escape" => "source_escape",
                _ => "source_cycle",
            }
        );
    }
}

#[test]
fn catchup_batches_bound_records_and_bytes_while_preserving_source_data() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("events.jsonl");
    let bytes = b"{}\n".repeat(5001);
    fs::write(&path, &bytes).unwrap();
    let mut reader = FollowReader::open(directory.path(), Default::default()).unwrap();
    let first = reader.update().unwrap();
    assert_eq!(first.progress.record_count, 2000);
    assert!(first.catching_up);
    assert_eq!(reader.update().unwrap().progress.record_count, 4000);
    let last = reader.update().unwrap();
    assert_eq!(last.progress.record_count, 5001);
    assert!(!last.catching_up);
    assert!(reader.update().unwrap().snapshot.is_none());
    assert_eq!(fs::read(path).unwrap(), bytes);
}

#[test]
fn unread_backlog_truncation_stops_even_when_it_stays_above_the_cursor() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("events.jsonl");
    fs::write(&path, b"{}\n".repeat(5001)).unwrap();
    let mut reader = FollowReader::open(directory.path(), Default::default()).unwrap();
    let first = reader.update().unwrap();
    assert_eq!(first.progress.read_bytes, 6000);
    File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(12000)
        .unwrap();
    assert_eq!(reader.update().err().unwrap().code, "source_changed");
    assert_eq!(reader.progress().read_bytes, 6000);
}

#[test]
fn byte_batches_split_long_records_without_resetting_the_pending_budget() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("events.jsonl");
    let mut bytes = vec![b'x'; 1024 * 1024 + 7];
    bytes.push(b'\n');
    fs::write(&path, &bytes).unwrap();
    let mut reader = FollowReader::open(
        directory.path(),
        ImportLimits {
            record_bytes: 2 * 1024 * 1024,
            ..Default::default()
        },
    )
    .unwrap();
    let first = reader.update().unwrap();
    assert!(first.catching_up);
    assert_eq!(first.progress.read_bytes, 1024 * 1024);
    assert_eq!(first.progress.pending_bytes, 1024 * 1024);
    assert_eq!(first.progress.record_count, 0);
    let second = reader.update().unwrap();
    assert!(!second.catching_up);
    assert_eq!(second.progress.read_bytes, bytes.len());
    assert_eq!(second.progress.pending_bytes, 0);
    assert_eq!(second.progress.record_count, 1);
    assert_eq!(
        second
            .snapshot
            .unwrap()
            .diagnostics(&Default::default())
            .unwrap()
            .items[0]
            .code,
        "malformed_record"
    );
    assert_eq!(fs::read(path).unwrap(), bytes);
}

#[test]
fn live_projection_retains_preview_and_response_bounds() {
    let directory = tempfile::tempdir().unwrap();
    write(
        directory.path(),
        "events.jsonl",
        &[
            json!({"type":"step_start"}),
            json!({"type":"llm_request","input":"é".repeat(10000)}),
        ],
    );
    let mut reader = FollowReader::open(directory.path(), Default::default()).unwrap();
    let snapshot = reader.update().unwrap().snapshot.unwrap();
    let preview = snapshot.content("n4").unwrap().input;
    assert_eq!(preview.text.unwrap().len(), 8192);
    assert!(preview.truncated);
    let inspection = snapshot.node("n4").unwrap().context.unwrap();
    assert_eq!(inspection.payload_size.unwrap().text_chars, Some(10000));
    let mut bounded = FollowReader::open(
        directory.path(),
        ImportLimits {
            response_bytes: 100,
            ..Default::default()
        },
    )
    .unwrap();
    let snapshot = bounded.update().unwrap().snapshot.unwrap();
    assert_eq!(snapshot.node("n4").err().unwrap().code, "view_limit");
    assert_eq!(
        snapshot
            .observations(&Default::default())
            .err()
            .unwrap()
            .code,
        "view_limit"
    );
    assert_eq!(snapshot.overview().err().unwrap().code, "view_limit");
}

#[test]
fn recognized_source_cap_is_cumulative_across_updates() {
    let directory = tempfile::tempdir().unwrap();
    write(directory.path(), "events.jsonl", &[]);
    let mut reader = FollowReader::open(
        directory.path(),
        ImportLimits {
            sources: 2,
            ..Default::default()
        },
    )
    .unwrap();
    reader.update().unwrap();
    write(directory.path(), "subagents/a/events.jsonl", &[]);
    let accepted = reader.update().unwrap();
    assert_eq!(accepted.progress.source_count, 2);
    assert_eq!(
        accepted.snapshot.unwrap().overview().unwrap().source_count,
        2
    );
    for _ in 0..10 {
        reader.update().unwrap();
    }
    write(directory.path(), "subagents/b/events.jsonl", &[]);
    assert_eq!(reader.update().err().unwrap().code, "source_entry_limit");
    assert_eq!(reader.progress().source_count, 2);
}

#[test]
fn reverse_tool_completion_order_links_children_only_by_explicit_identifiers() {
    let directory = tempfile::tempdir().unwrap();
    write(
        directory.path(),
        "events.jsonl",
        &[
            json!({"type":"tool_start","toolCallId":"a"}),
            json!({"type":"tool_start","toolCallId":"b"}),
        ],
    );
    let mut reader = FollowReader::open(directory.path(), Default::default()).unwrap();
    reader.update().unwrap();
    for name in ["first", "second"] {
        write(
            directory.path(),
            &format!("subagents/{name}/events.jsonl"),
            &[json!({"type":"step_end","usage":{"totalTokens":40}})],
        );
    }
    reader.update().unwrap();
    let path = directory.path().join("events.jsonl");
    append(&path, b"{\"type\":\"tool_end\",\"toolCallId\":\"b\",\"subagent\":{\"results\":[{\"childTraceId\":\"second\"}]}}\n");
    reader.update().unwrap();
    append(&path, b"{\"type\":\"tool_end\",\"toolCallId\":\"a\",\"subagent\":{\"results\":[{\"childTraceId\":\"first\"}]}}\n");
    let trace = reader.update().unwrap().snapshot.unwrap();
    for row in rows(&trace)
        .into_iter()
        .filter(|row| row.kind == NodeKind::Session && row.source.file != "events.jsonl")
    {
        let parent_id = trace
            .node(&row.id)
            .unwrap()
            .relationship
            .execution_parent_id
            .unwrap();
        let parent = trace.node(&parent_id).unwrap();
        assert_eq!(
            parent.tool_inspection.unwrap().call_id.as_deref(),
            Some(if row.source.file.contains("first") {
                "a"
            } else {
                "b"
            })
        );
    }
}
