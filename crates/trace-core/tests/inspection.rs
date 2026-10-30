use serde_json::{Value, json};
use std::{fs, path::Path};
use trace_core::{
    DiagnosticQuery, ImportLimits, ObservationQuery, StructuralFilter, Trace, views::*,
};

fn open(events: Vec<Value>) -> Trace {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("events.jsonl"),
        events.iter().map(|e| format!("{e}\n")).collect::<String>(),
    )
    .unwrap();
    Trace::open(directory.path(), ImportLimits::default()).unwrap()
}
fn events(request: Value) -> Vec<Value> {
    vec![
        json!({"type":"session_start"}),
        json!({"type":"interaction_start"}),
        json!({"type":"turn_start"}),
        json!({"type":"step_start","stepIndex":1}),
        request,
    ]
}
fn contract(name: &str, mut detail: NodeDetail) {
    detail.snapshot_id = "synthetic-inspection-snapshot".into();
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
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("tests/contracts/detail-inspection-{name}.json"));
    let serialized = format!("{}\n", serde_json::to_string_pretty(&detail).unwrap());
    if std::env::var_os("UPDATE_CONTRACTS").is_some() {
        fs::write(&path, &serialized).unwrap();
    }
    assert_eq!(fs::read_to_string(path).unwrap(), serialized);
}
fn provider() -> (Value, String, Vec<Value>) {
    let skills = (0..167).map(|i| format!("<skill><name>synthetic-skill-{i:03}</name><description>{}PRIVATE_DESCRIPTOR_SENTINEL</description><location>/synthetic/private-location</location></skill>", "é😀".repeat(i % 5 + 40))).collect::<String>();
    let instructions = format!(
        "<rules>PRIVATE_INSTRUCTIONS_SENTINEL</rules><skills><available_skills>{skills}</available_skills></skills>"
    );
    let tools = (0..45).map(|i| json!({"type":"function", "name": format!("synthetic-tool-{i:02}"), "description": "PRIVATE_SCHEMA_SENTINEL".repeat(i + 1), "parameters":{"type":"object","properties":{"path":{"type":"string"}}}})).collect::<Vec<_>>();
    let request = json!({"type":"llm_request","input":{"model":"synthetic-model","input":[
        {"type":"additional_tools","role":"developer","tools":tools},
        {"role":"developer","content":[{"type":"input_text","text":instructions}]},
        {"role":"user","content":[{"text":"é😀\"\nPRIVATE_USER_SENTINEL"}]},
        {"type":"function_call","name":"read","arguments":"{\"path\":\"PRIVATE_ARGUMENT_SENTINEL\"}"},
        {"type":"function_call_output","output":"é😀\nPRIVATE_MODEL_OUTPUT_SENTINEL"},
        {"type":"reasoning","encrypted_content":"PRIVATE_REASONING_SENTINEL"}
    ]}});
    (request, instructions, tools)
}
#[test]
fn incomplete_function_evidence_makes_overall_composition_partial() {
    for item in [
        json!({"type":"function_call"}),
        json!({"type":"function_call","arguments":{"private":"PRIVATE_ARGUMENT_SENTINEL"}}),
        json!({"type":"function_call_output"}),
        json!({"type":"function_call_output","output":{"private":"PRIVATE_RESULT_SENTINEL"}}),
    ] {
        let trace = open(events(
            json!({"type":"llm_request","input":{"input":[item]}}),
        ));
        let detail = trace.node("n4").unwrap();
        assert_eq!(detail.context.as_ref().unwrap().coverage, Coverage::Partial);
        let metadata = serde_json::to_string(&detail).unwrap();
        assert!(!metadata.contains("PRIVATE_ARGUMENT_SENTINEL"));
        assert!(!metadata.contains("PRIVATE_RESULT_SENTINEL"));
    }
}

#[test]
fn inconsistent_or_literal_marker_lengths_do_not_prove_source_completeness() {
    for (preview, length, expected) in [
        ("abcdef".to_owned(), 1, SourceTruncation::Unknown),
        (
            "literal ...[truncated]".to_owned(),
            "literal ...[truncated]".encode_utf16().count() as u64,
            SourceTruncation::Unknown,
        ),
        (
            format!("{}...[truncated]", "x".repeat(500)),
            501,
            SourceTruncation::KnownTruncated,
        ),
        (
            format!("{}...[truncated]", "x".repeat(500)),
            514,
            SourceTruncation::KnownTruncated,
        ),
    ] {
        let mut records = events(json!({"type":"llm_request"}));
        records.push(json!({"type":"tool_start","toolName":"read","toolCallId":"length"}));
        records.push(json!({"type":"tool_end","toolCallId":"length","resultPreview":preview,"resultTotalLength":length}));
        let trace = open(records);
        let detail = trace.node("n5").unwrap();
        assert_eq!(
            detail
                .tool_inspection
                .unwrap()
                .result
                .preview
                .source_truncation,
            expected
        );
        assert_eq!(
            trace.content("n5").unwrap().output.truncated,
            expected == SourceTruncation::KnownTruncated
        );
    }
}
#[test]
fn full_provider_catalogues_survive_preview_caps_and_default_views_omit_bodies() {
    let (request, instructions, tools) = provider();
    let expected_bytes = serde_json::to_vec(&request["input"]).unwrap().len();
    let trace = open(events(request));
    let detail = trace.node("n4").unwrap();
    let context = detail.context.as_ref().unwrap();
    assert_eq!(context.shape, RequestShape::ProviderInput);
    assert_eq!(context.coverage, Coverage::Complete);
    assert_eq!(context.retention, Retention::ListsLimited);
    let counts = context.counts.as_ref().unwrap();
    assert_eq!(
        (
            counts.messages,
            counts.tool_definitions,
            counts.skill_descriptors,
            counts.unclassified_items
        ),
        (Some(2), Some(45), Some(167), 0)
    );
    assert_eq!(
        context.payload_size.as_ref().unwrap().compact_json_bytes,
        expected_bytes
    );
    assert_eq!(context.source.as_ref().unwrap().line, 5);
    let developer = context
        .categories
        .iter()
        .find(|c| c.kind == ContextKind::DeveloperInstructions)
        .unwrap();
    assert_eq!(
        developer.size.text_chars,
        Some(instructions.chars().count())
    );
    let definitions = context
        .categories
        .iter()
        .find(|c| c.kind == ContextKind::ToolDefinitions)
        .unwrap();
    assert_eq!(
        definitions.size.compact_json_bytes,
        tools
            .iter()
            .map(|t| serde_json::to_vec(t).unwrap().len())
            .sum::<usize>()
    );
    assert_eq!(context.largest_tools.len(), 16);
    assert_eq!(context.largest_skills.len(), 16);
    assert_eq!(context.largest_items.len(), 16);
    assert_eq!(context.omitted.tools, Some(29));
    assert_eq!(context.omitted.skills, Some(151));
    assert_eq!(
        context.largest_tools[0].name.as_deref(),
        Some("synthetic-tool-44")
    );
    let skills = context
        .sections
        .iter()
        .find(|s| s.kind == "skills")
        .unwrap();
    let available = context
        .sections
        .iter()
        .find(|s| s.kind == "available_skills")
        .unwrap();
    assert_eq!(available.parent_id, Some(skills.id));
    assert!(skills.text_chars > available.text_chars);
    assert!(
        context
            .categories
            .iter()
            .any(|c| c.kind == ContextKind::OpaqueReasoning && c.size.text_chars.is_none())
    );
    let preview = trace.content("n4").unwrap().input;
    assert!(preview.truncated);
    assert!(preview.text.unwrap().len() <= CONTENT_BYTES);
    for view in [
        serde_json::to_string(&detail).unwrap(),
        serde_json::to_string(&trace.overview().unwrap()).unwrap(),
        serde_json::to_string(&trace.observations(&Default::default()).unwrap()).unwrap(),
        serde_json::to_string(&trace.diagnostics(&Default::default()).unwrap()).unwrap(),
    ] {
        for sentinel in [
            "PRIVATE_DESCRIPTOR_SENTINEL",
            "PRIVATE_SCHEMA_SENTINEL",
            "PRIVATE_INSTRUCTIONS_SENTINEL",
            "PRIVATE_USER_SENTINEL",
            "PRIVATE_ARGUMENT_SENTINEL",
            "PRIVATE_MODEL_OUTPUT_SENTINEL",
            "PRIVATE_REASONING_SENTINEL",
            "/synthetic/private-location",
        ] {
            assert!(!view.contains(sentinel), "metadata exposed {sentinel}");
        }
    }
    contract("provider", detail);
}
#[test]
fn missing_null_unsupported_plain_mixed_and_envelope_shapes_are_distinct() {
    for (name, request, shape, coverage) in [
        (
            "missing",
            json!({"type":"llm_request"}),
            RequestShape::Missing,
            Coverage::Missing,
        ),
        (
            "null",
            json!({"type":"llm_request","messages":null,"prompt":"PRIVATE_LOWER_PRECEDENCE","input":[]}),
            RequestShape::Missing,
            Coverage::Missing,
        ),
        (
            "unsupported",
            json!({"type":"llm_request","input":{"unrecognized":"PRIVATE_UNSUPPORTED"}}),
            RequestShape::Unsupported,
            Coverage::Unsupported,
        ),
        (
            "plain",
            json!({"type":"llm_request","prompt":"é😀\"\nPRIVATE_PLAIN"}),
            RequestShape::PlainText,
            Coverage::Complete,
        ),
        (
            "partial",
            json!({"type":"llm_request","messages":[{"role":"system","content":"<skills><available_skills><skill><name>alpha</name></skill></available_skills></skills>"},{"role":"user","content":[{"type":"image","data":"PRIVATE_IMAGE"},{"text":"é😀"}]},{"type":"future","body":"PRIVATE_FUTURE"}]}),
            RequestShape::Messages,
            Coverage::Partial,
        ),
        (
            "envelope",
            json!({"type":"llm_request","input":{"system":"<rules>PRIVATE_SYSTEM</rules>","tools":[{"function":{"name":"supported-name","description":"PRIVATE_DESCRIPTION"}}],"messages":[{"role":"assistant","content":"PRIVATE_HISTORY"},{"role":"tool","content":"PRIVATE_TOOL_OUTPUT"}]}}),
            RequestShape::ProviderInput,
            Coverage::Complete,
        ),
    ] {
        let trace = open(events(request));
        let detail = trace.node("n4").unwrap();
        let context = detail.context.as_ref().unwrap();
        assert_eq!(context.shape, shape, "{name}");
        assert_eq!(context.coverage, coverage, "{name}");
        if matches!(
            shape,
            RequestShape::Missing | RequestShape::PlainText | RequestShape::Unsupported
        ) {
            assert!(context.counts.is_none());
        }
        if name == "null" {
            assert_eq!(context.request_field, Some(RequestField::Messages));
            assert_eq!(
                trace.content("n4").unwrap().input.state,
                ContentState::Unavailable
            );
        }
        if name == "plain" {
            assert_eq!(
                context.payload_size.as_ref().unwrap().text_chars,
                Some("é😀\"\nPRIVATE_PLAIN".chars().count())
            );
        }
        if name == "partial" {
            assert_eq!(context.counts.as_ref().unwrap().unclassified_items, 1);
        }
        contract(name, detail);
    }
    let trace = open(events(json!({"type":"ignored"})));
    assert_eq!(
        trace.node("n4").unwrap().context.unwrap().coverage,
        Coverage::Missing
    );
    assert!(trace.node("n1").unwrap().context.is_none());
}
#[test]
fn duplicate_request_summary_and_preview_share_last_record_and_null_precedence() {
    let mut records = events(
        json!({"type":"llm_request","messages":[{"role":"user","content":"FIRST_PRIVATE"}]}),
    );
    records.push(json!({"type":"llm_request","prompt":"last😀"}));
    let trace = open(records.clone());
    let context = trace.node("n4").unwrap().context.unwrap();
    assert_eq!(context.shape, RequestShape::PlainText);
    assert_eq!(context.source.unwrap().line, 6);
    assert_eq!(context.payload_size.unwrap().text_chars, Some(5));
    assert_eq!(
        trace.content("n4").unwrap().input.text.as_deref(),
        Some("last😀")
    );
    assert!(
        trace
            .diagnostics(&DiagnosticQuery::default())
            .unwrap()
            .items
            .iter()
            .any(|d| d.code == "duplicate_request" && d.source.line == 6)
    );
    records.push(json!({"type":"llm_request","messages":null,"prompt":"NOT_SELECTED"}));
    let trace = open(records);
    assert_eq!(
        trace.node("n4").unwrap().context.unwrap().coverage,
        Coverage::Missing
    );
    assert_eq!(
        trace.content("n4").unwrap().input.state,
        ContentState::Unavailable
    );
}
#[test]
fn tool_identity_and_three_measurement_domains_remain_distinct() {
    let argument = json!({"path":"PRIVATE_ARGUMENT_PATH","value":"é😀\"\n"});
    let result = format!("{}...[truncated]", "é😀".repeat(1800));
    let call_id = format!("{}final", "é".repeat(300));
    let mut records = events(
        json!({"type":"llm_request","input":{"input":[{"type":"function_call_output","output":"model-visible😀"}]}}),
    );
    records.extend([json!({"type":"tool_start","toolName":"read","toolCallId":call_id,"args":argument}), json!({"type":"tool_end","toolCallId":call_id,"resultPreview":result,"resultTotalLength":20000})]);
    let trace = open(records);
    let detail = trace.node("n5").unwrap();
    let tool = detail.tool_inspection.as_ref().unwrap();
    assert_eq!(tool.call_id.as_ref().unwrap().len(), DISPLAY_BYTES);
    assert!(tool.call_id_truncated);
    assert_eq!(detail.observation.outcome, Outcome::Success);
    assert_eq!(
        tool.arguments
            .selected_size
            .as_ref()
            .unwrap()
            .compact_json_bytes,
        serde_json::to_vec(&argument).unwrap().len()
    );
    assert_eq!(
        tool.arguments.preview_utf8_bytes,
        Some(serde_json::to_string(&argument).unwrap().len())
    );
    assert_eq!(
        tool.result
            .preview
            .selected_size
            .as_ref()
            .unwrap()
            .compact_json_bytes,
        serde_json::to_vec(&result).unwrap().len()
    );
    assert_eq!(
        tool.result.preview.source_truncation,
        SourceTruncation::KnownTruncated
    );
    assert!(tool.result.preview.local_truncated);
    assert_eq!(
        tool.result.original_length.as_ref().unwrap().unit,
        OriginalLengthUnit::Utf16CodeUnitsOfSerializedResult
    );
    assert_eq!(tool.result.preview.source.as_ref().unwrap().line, 7);
    let content = trace.content("n5").unwrap();
    assert!(content.output.truncated);
    assert!(content.output.text.unwrap().len() <= CONTENT_BYTES);
    assert!(
        content
            .input
            .text
            .unwrap()
            .contains("PRIVATE_ARGUMENT_PATH")
    );
    assert!(
        !serde_json::to_string(&detail)
            .unwrap()
            .contains("PRIVATE_ARGUMENT_PATH")
    );
    let output_category = trace
        .node("n4")
        .unwrap()
        .context
        .unwrap()
        .categories
        .into_iter()
        .find(|c| c.kind == ContextKind::ToolOutputs)
        .unwrap();
    assert_eq!(
        output_category.size.text_chars,
        Some("model-visible😀".chars().count())
    );
    contract("tool", detail);
    for (name, extra, truncation, unit) in [
        (
            "tool-complete",
            json!({"resultPreview":"\"é😀\"","resultTotalLength":5}),
            SourceTruncation::KnownComplete,
            Some(OriginalLengthUnit::Utf16CodeUnitsOfSerializedResult),
        ),
        (
            "tool-unknown",
            json!({"resultPreview":"small"}),
            SourceTruncation::Unknown,
            None,
        ),
        (
            "tool-foreign",
            json!({"resultPreview":"small","resultTotalLength":99,"collector":"foreign"}),
            SourceTruncation::Unknown,
            Some(OriginalLengthUnit::Unspecified),
        ),
        (
            "tool-result",
            json!({"result":{"text":"PRIVATE_RESULT"},"resultTotalLength":77}),
            SourceTruncation::Unknown,
            Some(OriginalLengthUnit::Utf16CodeUnitsOfSerializedResult),
        ),
        (
            "tool-unavailable",
            json!({"resultPreview":null,"result":"LOWER_RESULT"}),
            SourceTruncation::Unknown,
            None,
        ),
        (
            "tool-huge-length",
            json!({"resultPreview":"small","resultTotalLength":u64::MAX}),
            SourceTruncation::KnownTruncated,
            Some(OriginalLengthUnit::Utf16CodeUnitsOfSerializedResult),
        ),
    ] {
        let mut records = events(json!({"type":"llm_request"}));
        records.push(json!({"type":"tool_start","toolName":"read","toolCallId":"id"}));
        let mut end = json!({"type":"tool_end","toolCallId":"id"});
        end.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        records.push(end);
        let trace = open(records);
        let detail = trace.node("n5").unwrap();
        let result = &detail.tool_inspection.as_ref().unwrap().result;
        assert_eq!(result.preview.source_truncation, truncation);
        assert_eq!(result.original_length.as_ref().map(|l| l.unit), unit);
        if name == "tool-unavailable" {
            assert_eq!(result.preview.availability, ContentState::Unavailable);
        }
        contract(name, detail);
    }
}
#[test]
fn generation_usage_derivation_requires_every_operand_and_checked_addition_without_fallback() {
    let mut records = vec![
        json!({"type":"session_start"}),
        json!({"type":"interaction_start"}),
        json!({"type":"turn_start"}),
    ];
    for usage in [
        json!({"input":49788,"cacheRead":0,"cacheWrite":0,"output":216}),
        json!({"input":2715,"cacheRead":49408,"cacheWrite":0,"output":26}),
        json!({"input":865,"cacheRead":51968,"cacheWrite":0,"output":29}),
        json!({"input":1510,"cacheRead":52736,"cacheWrite":0,"output":491}),
        json!({"input":1,"cacheRead":2,"output":0}),
        json!({"input":u64::MAX,"cacheRead":1,"cacheWrite":0,"output":0}),
    ] {
        records.extend([
            json!({"type":"step_start"}),
            json!({"type":"step_end","usage":usage}),
        ]);
    }
    records.push(json!({"type":"turn_summary","usage":{"input":999999,"cacheRead":0,"cacheWrite":0,"output":0}}));
    let trace = open(records);
    let page = trace
        .observations(&ObservationQuery {
            filter: Some(StructuralFilter::Generation),
            ..Default::default()
        })
        .unwrap();
    let totals = page
        .items
        .into_iter()
        .filter(|row| row.r#match)
        .map(|row| {
            trace
                .node(&row.id)
                .unwrap()
                .context
                .unwrap()
                .context_input_tokens
        })
        .collect::<Vec<_>>();
    assert_eq!(
        totals,
        [
            Some(49788),
            Some(52123),
            Some(52833),
            Some(54246),
            None,
            None
        ]
    );
    assert!(trace.node("n3").unwrap().context.is_none());
}
#[test]
fn fixed_lexer_bounds_names_lists_and_malformed_unknown_descriptor_evidence() {
    let long_name = "é".repeat(300);
    let text = format!(
        "<skills><available_skills><skill><name>{long_name}</name><description>PRIVATE</description></skill></available_skills></skills>{}",
        "<rules>x</rules>".repeat(40)
    );
    let trace = open(events(
        json!({"type":"llm_request","messages":[{"role":"developer","content":text}]}),
    ));
    let context = trace.node("n4").unwrap().context.unwrap();
    assert_eq!(context.sections.len(), 32);
    assert_eq!(context.omitted.sections, Some(10));
    assert_eq!(context.largest_skills[0].name.as_ref().unwrap().len(), 512);
    assert!(context.largest_skills[0].name_truncated);
    for text in ["<skills><available_skills><skill><description>unsupported descriptor</description></skill></available_skills></skills>".into(), "<skills><available_skills></skills></available_skills>".into(), format!("{}x{}", "<skills>".repeat(33), "</skills>".repeat(33))] {
        let trace = open(events(json!({"type":"llm_request","messages":[{"role":"developer","content":text}]}))); let context = trace.node("n4").unwrap().context.unwrap();
        assert_eq!(context.coverage,Coverage::Partial); assert_eq!(context.counts.unwrap().skill_descriptors,None); assert_eq!(context.omitted.sections,None);
        assert!(trace.diagnostics(&Default::default()).unwrap().items.iter().any(|d| d.code == "context_sections_malformed"));
    }
}
#[test]
fn generation_pages_preserve_ancestors_match_counts_and_query_cursor_identity() {
    let mut records = events(json!({"type":"llm_request"}));
    records.extend([
        json!({"type":"tool_start","toolCallId":"tool"}),
        json!({"type":"step_start"}),
        json!({"type":"llm_request"}),
    ]);
    let trace = open(records);
    let mut query = ObservationQuery {
        filter: Some(StructuralFilter::Generation),
        limit: 2,
        ..Default::default()
    };
    let first = trace.observations(&query).unwrap();
    assert_eq!(first.total_matches, 2);
    assert!(first.items.iter().all(|r| r.context && !r.r#match));
    assert!(first.next_cursor.is_some());
    let mut loaded = first.items;
    query.cursor = first.next_cursor;
    while query.cursor.is_some() {
        let page = trace.observations(&query).unwrap();
        loaded.extend(page.items);
        query.cursor = page.next_cursor;
    }
    assert_eq!(
        loaded
            .iter()
            .filter(|r| r.r#match && r.kind == NodeKind::Generation)
            .count(),
        2
    );
    assert!(!loaded.iter().any(|r| r.kind == NodeKind::Tool));
    let first = trace
        .observations(&ObservationQuery {
            filter: Some(StructuralFilter::Generation),
            limit: 2,
            ..Default::default()
        })
        .unwrap();
    assert!(
        trace
            .observations(&ObservationQuery {
                filter: Some(StructuralFilter::Tool),
                cursor: first.next_cursor,
                limit: 2,
                ..Default::default()
            })
            .is_err()
    );
}

#[test]
fn malformed_tool_envelopes_keep_unknown_counts_separate_from_empty_arrays() {
    for tools in [
        Value::Null,
        json!({"unexpected":"PRIVATE"}),
        json!([42,{"name":"recognized"}]),
    ] {
        let trace = open(events(
            json!({"type":"llm_request","input":{"input":[],"tools":tools}}),
        ));
        let context = trace.node("n4").unwrap().context.unwrap();
        assert_eq!(context.coverage, Coverage::Partial);
        assert_eq!(context.counts.unwrap().tool_definitions, None);
        assert_eq!(context.omitted.tools, None);
    }
    let trace = open(events(
        json!({"type":"llm_request","input":{"input":[],"tools":[]}}),
    ));
    let context = trace.node("n4").unwrap().context.unwrap();
    assert_eq!(context.coverage, Coverage::Complete);
    assert_eq!(context.counts.unwrap().tool_definitions, Some(0));
    assert_eq!(context.payload_size.unwrap().text_chars, Some(0));
}
