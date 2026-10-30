use crate::{bounded, model::preview, views::*};
use serde::Serialize;
use serde_json::Value;
use std::io::{self, Write};

pub(crate) const NODE_BYTES: usize = 64 * 1024;
pub(crate) const TOTAL_BYTES: usize = 16 * 1024 * 1024;
const TOP: usize = 16;
const SECTIONS: usize = 32;

#[derive(Default)]
struct Counter(usize);
impl Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(crate) fn bytes(value: &(impl Serialize + ?Sized)) -> usize {
    let mut counter = Counter::default();
    serde_json::to_writer(&mut counter, value).expect("serializable inspection value");
    counter.0
}
fn size(value: &Value, text_chars: Option<usize>) -> ValueSize {
    ValueSize {
        compact_json_bytes: bytes(value),
        text_chars,
    }
}
pub(crate) fn selected(event: &Value) -> (Option<RequestField>, Option<&Value>) {
    for (key, field) in [
        ("messages", RequestField::Messages),
        ("prompt", RequestField::Prompt),
        ("input", RequestField::Input),
    ] {
        if let Some(value) = event.get(key) {
            return (Some(field), Some(value));
        }
    }
    (None, None)
}
pub(crate) fn empty_context() -> ContextInspection {
    ContextInspection {
        coverage: Coverage::Missing,
        retention: Retention::Complete,
        shape: RequestShape::Missing,
        request_field: None,
        source: None,
        payload_size: None,
        counts: None,
        categories: vec![],
        largest_items: vec![],
        largest_tools: vec![],
        largest_skills: vec![],
        sections: vec![],
        omitted: OmittedCounts {
            items: None,
            tools: None,
            skills: None,
            sections: None,
        },
        context_input_tokens: None,
        context_input_tokens_exact: TokenCounter {
            state: TokenState::Unknown,
            value: None,
        },
    }
}
pub(crate) fn limited_context(
    coverage: Coverage,
    shape: RequestShape,
    field: Option<RequestField>,
    source: Option<SourceRef>,
) -> ContextInspection {
    ContextInspection {
        coverage,
        retention: Retention::RetentionLimited,
        shape,
        request_field: field,
        source,
        ..empty_context()
    }
}
struct Analyzer {
    context: ContextInspection,
    items: usize,
    tools: usize,
    tools_known: bool,
    skills: usize,
    sections: usize,
    text_chars: usize,
    malformed_sections: bool,
    skills_known: bool,
}
fn top<T>(items: &mut Vec<T>, value: T, rank: impl Fn(&T) -> usize) {
    items.push(value);
    items.sort_by_key(|item| std::cmp::Reverse(rank(item)));
    items.truncate(TOP);
}
impl Analyzer {
    fn component(
        &mut self,
        value: &Value,
        kind: ContextKind,
        text_chars: Option<usize>,
        coverage: Coverage,
    ) -> usize {
        let ordinal = self.items;
        self.items += 1;
        if kind == ContextKind::Other {
            self.context.counts.as_mut().unwrap().unclassified_items += 1;
            self.context.coverage = Coverage::Partial;
        }
        let measured = size(value, text_chars);
        if let Some(chars) = text_chars {
            self.text_chars += chars;
        }
        if let Some(category) = self.context.categories.iter_mut().find(|c| c.kind == kind) {
            category.count += 1;
            category.size.compact_json_bytes += measured.compact_json_bytes;
            category.size.text_chars = match (category.size.text_chars, text_chars) {
                (Some(a), Some(b)) => Some(a + b),
                (a, b) => a.or(b),
            };
            if category.text_coverage != coverage {
                category.text_coverage = Coverage::Partial;
            }
        } else {
            self.context.categories.push(ContextCategory {
                kind,
                count: 1,
                size: measured.clone(),
                text_coverage: coverage,
            });
        }
        top(
            &mut self.context.largest_items,
            ContextItem {
                ordinal,
                kind,
                size: measured,
                text_coverage: coverage,
            },
            |item| item.size.compact_json_bytes,
        );
        ordinal
    }
    fn tools(&mut self, value: &Value) {
        let Some(tools) = value.as_array() else {
            self.tools_known = false;
            self.component(value, ContextKind::Other, None, Coverage::Unsupported);
            return;
        };
        for tool in tools {
            if !tool.is_object() {
                self.tools_known = false;
                self.component(tool, ContextKind::Other, None, Coverage::Unsupported);
                continue;
            }
            self.component(
                tool,
                ContextKind::ToolDefinitions,
                None,
                Coverage::Unsupported,
            );
            let name = tool
                .get("name")
                .or_else(|| tool.get("function").and_then(|f| f.get("name")))
                .and_then(Value::as_str)
                .map(|s| bounded(s, DISPLAY_BYTES));
            top(
                &mut self.context.largest_tools,
                NamedContributor {
                    name: name.as_ref().map(|n| n.0.clone()),
                    name_truncated: name.is_some_and(|n| n.1),
                    ordinal: self.tools,
                    size: size(tool, None),
                },
                |item| item.size.compact_json_bytes,
            );
            self.tools += 1;
        }
    }
    fn item(&mut self, value: &Value) {
        let role = value.get("role").and_then(Value::as_str);
        match value.get("type").and_then(Value::as_str) {
            Some("additional_tools") => {
                if let Some(tools) = value.get("tools") {
                    self.tools(tools);
                } else {
                    self.tools_known = false;
                    self.component(value, ContextKind::Other, None, Coverage::Unsupported);
                }
                return;
            }
            Some("function_call") => {
                let arguments = value.get("arguments").and_then(Value::as_str);
                if arguments.is_none() {
                    self.context.coverage = Coverage::Partial;
                }
                self.component(
                    value,
                    ContextKind::ToolCalls,
                    arguments.map(|s| s.chars().count()),
                    Coverage::Partial,
                );
                return;
            }
            Some("function_call_output") => {
                let text = value.get("output").and_then(Value::as_str);
                if text.is_none() {
                    self.context.coverage = Coverage::Partial;
                }
                self.component(
                    value,
                    ContextKind::ToolOutputs,
                    text.map(|s| s.chars().count()),
                    if text.is_some() {
                        Coverage::Complete
                    } else {
                        Coverage::Unsupported
                    },
                );
                return;
            }
            Some("reasoning") => {
                self.component(
                    value,
                    ContextKind::OpaqueReasoning,
                    None,
                    Coverage::Unsupported,
                );
                return;
            }
            Some("message") | None => {}
            _ => {
                self.component(value, ContextKind::Other, None, Coverage::Unsupported);
                return;
            }
        }
        let kind = match role {
            Some("system") => ContextKind::SystemInstructions,
            Some("developer") => ContextKind::DeveloperInstructions,
            Some("user") => ContextKind::User,
            Some("assistant") => ContextKind::AssistantHistory,
            Some("tool" | "toolResult") => ContextKind::ToolOutputs,
            _ => {
                self.component(value, ContextKind::Other, None, Coverage::Unsupported);
                return;
            }
        };
        let (texts, complete) = recognized_text(value.get("content"));
        let chars = texts.iter().map(|s| s.chars().count()).sum();
        let ordinal = self.component(
            value,
            kind,
            Some(chars),
            if complete {
                Coverage::Complete
            } else {
                Coverage::Partial
            },
        );
        if !complete {
            self.context.coverage = Coverage::Partial;
        }
        if matches!(
            kind,
            ContextKind::SystemInstructions | ContextKind::DeveloperInstructions
        ) {
            for text in texts {
                self.scan(text, ordinal);
            }
        }
        if let Some(calls) = value.get("tool_calls") {
            self.context.coverage = Coverage::Partial;
            let _ = calls;
        }
    }
    fn scan(&mut self, text: &str, item_ordinal: usize) {
        const TAGS: &[&str] = &[
            "tools",
            "rules",
            "docs",
            "project_context",
            "skills",
            "available_skills",
            "cwd",
            "acp_delegate",
            "ponytail",
            "skill",
            "name",
            "description",
            "location",
        ];
        struct Open<'a> {
            name: &'a str,
            start: usize,
            body: usize,
            id: Option<usize>,
            parent: Option<usize>,
        }
        let mut stack: Vec<Open<'_>> = vec![];
        let mut cursor = 0;
        let mut local_sections = vec![];
        let mut local_skills = vec![];
        let mut found_available = false;
        while let Some(offset) = text[cursor..].find('<') {
            let start = cursor + offset;
            let Some(close) = text[start..].find('>') else {
                break;
            };
            cursor = start + close + 1;
            let raw = &text[start + 1..cursor - 1];
            let closing = raw.starts_with('/');
            let name = raw.strip_prefix('/').unwrap_or(raw);
            if !TAGS.contains(&name) {
                continue;
            }
            if closing {
                let Some(open) = stack.pop() else {
                    self.malformed_sections = true;
                    return;
                };
                if open.name != name {
                    self.malformed_sections = true;
                    return;
                }
                if name == "available_skills" {
                    found_available = true;
                }
                if let Some(id) = open.id {
                    local_sections.push(ContextSection {
                        id,
                        parent_id: open.parent,
                        kind: name.into(),
                        item_ordinal,
                        text_chars: text[open.start..cursor].chars().count(),
                    });
                }
                if name == "skill" && stack.iter().any(|s| s.name == "available_skills") {
                    let body = &text[open.body..start];
                    let name = body
                        .strip_prefix("<name>")
                        .or_else(|| body.trim_start().strip_prefix("<name>"))
                        .and_then(|b| b.split_once("</name>").map(|(name, _)| name))
                        .filter(|name| !name.contains(['<', '>']))
                        .map(|name| bounded(name.trim(), DISPLAY_BYTES));
                    if name.is_none() {
                        self.malformed_sections = true;
                        return;
                    }
                    local_skills.push(NamedContributor {
                        name: name.as_ref().map(|n| n.0.clone()),
                        name_truncated: name.is_some_and(|n| n.1),
                        ordinal: self.skills + local_skills.len(),
                        size: ValueSize {
                            compact_json_bytes: bytes(&text[open.start..cursor]),
                            text_chars: Some(text[open.start..cursor].chars().count()),
                        },
                    });
                }
            } else {
                if stack.len() >= 32 {
                    self.malformed_sections = true;
                    return;
                }
                let is_section = !matches!(name, "skill" | "name" | "description" | "location");
                let id = is_section.then(|| {
                    let id = self.sections;
                    self.sections += 1;
                    id
                });
                let parent = stack.iter().rev().find_map(|s| s.id);
                stack.push(Open {
                    name,
                    start,
                    body: cursor,
                    id,
                    parent,
                });
            }
        }
        if !stack.is_empty() {
            self.malformed_sections = true;
            return;
        }
        self.skills_known |= found_available;
        local_sections.sort_by_key(|section| section.id);
        for section in local_sections {
            if self.context.sections.len() < SECTIONS {
                self.context.sections.push(section);
            }
        }
        for skill in local_skills {
            self.skills += 1;
            top(&mut self.context.largest_skills, skill, |item| {
                item.size.text_chars.unwrap_or(0)
            });
        }
    }
}
fn recognized_text(value: Option<&Value>) -> (Vec<&str>, bool) {
    match value {
        Some(Value::String(text)) => (vec![text], true),
        Some(Value::Array(blocks)) => {
            let mut texts = vec![];
            let mut complete = true;
            for block in blocks {
                if block
                    .get("type")
                    .and_then(Value::as_str)
                    .is_none_or(|kind| matches!(kind, "text" | "input_text" | "output_text"))
                    && let Some(text) = block.get("text").and_then(Value::as_str)
                {
                    texts.push(text);
                    continue;
                }
                complete = false;
            }
            (texts, complete)
        }
        _ => (vec![], false),
    }
}
pub(crate) fn context(event: &Value, source: &SourceRef) -> (ContextInspection, bool) {
    let (field, value) = selected(event);
    let mut context = empty_context();
    context.request_field = field;
    context.source = Some(source.clone());
    let Some(value) = value.filter(|v| !v.is_null()) else {
        return (context, false);
    };
    context.payload_size = Some(size(value, None));
    context.coverage = Coverage::Complete;
    if let Some(text) = value.as_str() {
        context.shape = RequestShape::PlainText;
        context.payload_size.as_mut().unwrap().text_chars = Some(text.chars().count());
        context.categories.push(ContextCategory {
            kind: ContextKind::Other,
            count: 1,
            size: size(value, Some(text.chars().count())),
            text_coverage: Coverage::Complete,
        });
        return (context, false);
    }
    let envelope = value.as_object();
    let messages = value.as_array().or_else(|| {
        envelope
            .and_then(|o| o.get("input").or_else(|| o.get("messages")))
            .and_then(Value::as_array)
    });
    let Some(messages) = messages else {
        context.coverage = Coverage::Unsupported;
        context.shape = RequestShape::Unsupported;
        return (context, false);
    };
    context.shape = if value.is_array() {
        RequestShape::Messages
    } else {
        RequestShape::ProviderInput
    };
    context.counts = Some(ContextCounts {
        messages: Some(
            messages
                .iter()
                .filter(|v| {
                    v.get("role").is_some()
                        && v.get("type").and_then(Value::as_str) != Some("additional_tools")
                })
                .count(),
        ),
        tool_definitions: Some(0),
        skill_descriptors: None,
        unclassified_items: 0,
    });
    let mut analyzer = Analyzer {
        context,
        items: 0,
        tools: 0,
        tools_known: true,
        skills: 0,
        sections: 0,
        text_chars: 0,
        malformed_sections: false,
        skills_known: false,
    };
    if let Some(system) = envelope.and_then(|e| e.get("system")) {
        let (texts, complete) = recognized_text(Some(system));
        let chars = texts.iter().map(|s| s.chars().count()).sum();
        let ordinal = analyzer.component(
            system,
            ContextKind::SystemInstructions,
            Some(chars),
            if complete {
                Coverage::Complete
            } else {
                Coverage::Partial
            },
        );
        if !complete {
            analyzer.context.coverage = Coverage::Partial;
        }
        for text in texts {
            analyzer.scan(text, ordinal);
        }
    }
    if let Some(tools) = envelope.and_then(|e| e.get("tools")) {
        analyzer.tools(tools);
    }
    for item in messages {
        analyzer.item(item);
    }
    let counts = analyzer.context.counts.as_mut().unwrap();
    counts.tool_definitions = analyzer.tools_known.then_some(analyzer.tools);
    counts.skill_descriptors =
        (analyzer.skills_known && !analyzer.malformed_sections).then_some(analyzer.skills);
    analyzer.context.payload_size.as_mut().unwrap().text_chars = Some(analyzer.text_chars);
    analyzer.context.omitted = OmittedCounts {
        items: Some(
            analyzer
                .items
                .saturating_sub(analyzer.context.largest_items.len()),
        ),
        tools: analyzer.tools_known.then_some(
            analyzer
                .tools
                .saturating_sub(analyzer.context.largest_tools.len()),
        ),
        skills: (analyzer.skills_known && !analyzer.malformed_sections).then_some(
            analyzer
                .skills
                .saturating_sub(analyzer.context.largest_skills.len()),
        ),
        sections: (!analyzer.malformed_sections).then_some(
            analyzer
                .sections
                .saturating_sub(analyzer.context.sections.len()),
        ),
    };
    if analyzer.malformed_sections {
        analyzer.context.coverage = Coverage::Partial;
    }
    if [
        analyzer.context.omitted.items,
        analyzer.context.omitted.tools,
        analyzer.context.omitted.skills,
        analyzer.context.omitted.sections,
    ]
    .iter()
    .any(|n| n.is_some_and(|n| n > 0))
    {
        analyzer.context.retention = Retention::ListsLimited;
    }
    (analyzer.context, analyzer.malformed_sections)
}
pub(crate) fn trim_context(context: &mut ContextInspection, limit: usize) {
    if bytes(context) <= limit {
        return;
    }
    context.retention = Retention::ListsLimited;
    for (list, omitted) in [
        (&mut context.largest_tools, &mut context.omitted.tools),
        (&mut context.largest_skills, &mut context.omitted.skills),
    ] {
        *omitted = omitted.map(|n| n + list.len());
        list.clear();
    }
    context.omitted.items = context
        .omitted
        .items
        .map(|n| n + context.largest_items.len());
    context.largest_items.clear();
    context.omitted.sections = context.omitted.sections.map(|n| n + context.sections.len());
    context.sections.clear();
}
pub(crate) fn evidence(
    value: Option<&Value>,
    source: Option<&SourceRef>,
    source_truncation: SourceTruncation,
) -> PreviewEvidence {
    let content = preview(value);
    PreviewEvidence {
        availability: content.state,
        selected_size: value
            .filter(|v| !v.is_null())
            .map(|v| size(v, v.as_str().map(|s| s.chars().count()))),
        preview_utf8_bytes: content.text.as_ref().map(String::len),
        source: source.cloned(),
        local_truncated: content.truncated,
        source_truncation,
    }
}
pub(crate) fn tool(event: &Value, source: &SourceRef) -> ToolInspection {
    let call = event
        .get("toolCallId")
        .and_then(Value::as_str)
        .map(|s| bounded(s, DISPLAY_BYTES));
    ToolInspection {
        retention: Retention::Complete,
        call_id: call.as_ref().map(|c| c.0.clone()),
        call_id_truncated: call.is_some_and(|c| c.1),
        call_id_source: event
            .get("toolCallId")
            .and_then(Value::as_str)
            .map(|_| source.clone()),
        arguments: evidence(
            event
                .get("arguments")
                .or_else(|| event.get("args"))
                .or_else(|| event.get("input")),
            Some(source),
            SourceTruncation::Unknown,
        ),
        result: ResultEvidence {
            preview: evidence(None, None, SourceTruncation::Unknown),
            recorded_field: None,
            original_length: None,
        },
    }
}
pub(crate) fn tool_result(event: &Value, source: &SourceRef) -> ResultEvidence {
    let field = [
        ("resultPreview", ResultField::ResultPreview),
        ("result", ResultField::Result),
        ("output", ResultField::Output),
    ]
    .into_iter()
    .find(|(key, _)| event.get(key).is_some());
    let value = field.and_then(|(key, _)| event.get(key));
    let original = event.get("resultTotalLength").and_then(Value::as_u64);
    // The supported collector has no schema version. An explicit foreign marker cannot establish its length convention.
    let supported = event.get("collector").is_none() && event.get("schemaVersion").is_none();
    let truncation = match (original, value.and_then(Value::as_str)) {
        (Some(length), Some(text))
            if supported && field.is_some_and(|(_, f)| f == ResultField::ResultPreview) =>
        {
            let recorded_length = text.encode_utf16().count() as u64;
            let marker_prefix = text.strip_suffix("...[truncated]");
            let collector_marker = marker_prefix.is_some_and(|prefix| {
                let prefix_length = prefix.encode_utf16().count() as u64;
                matches!(prefix_length, 500 | 3000 | 8000) && length > prefix_length
            });
            if length > recorded_length || collector_marker {
                SourceTruncation::KnownTruncated
            } else if length == recorded_length && marker_prefix.is_none() {
                SourceTruncation::KnownComplete
            } else {
                SourceTruncation::Unknown
            }
        }
        _ => SourceTruncation::Unknown,
    };
    ResultEvidence {
        preview: evidence(value, Some(source), truncation),
        recorded_field: field.map(|(_, f)| f),
        original_length: original.map(|value| OriginalLength {
            value,
            unit: if supported {
                OriginalLengthUnit::Utf16CodeUnitsOfSerializedResult
            } else {
                OriginalLengthUnit::Unspecified
            },
            source: source.clone(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn per_node_list_trimming_and_compact_counting_do_not_retain_private_text() {
        let event = json!({"input":{"input":[{"type":"additional_tools","tools":(0..45).map(|i| json!({"name":"é".repeat(300),"description":format!("PRIVATE_{i}")})).collect::<Vec<_>>()}]}});
        let (mut inspection, _) = context(
            &event,
            &SourceRef {
                file: "events.jsonl".into(),
                line: 1,
            },
        );
        assert_eq!(
            inspection.largest_tools[0].name.as_ref().unwrap().len(),
            512
        );
        assert!(inspection.largest_tools[0].name_truncated);
        let original = bytes(&inspection);
        trim_context(&mut inspection, original - 1);
        assert!(inspection.largest_tools.is_empty());
        assert!(inspection.largest_items.is_empty());
        assert_eq!(inspection.omitted.tools, Some(45));
        assert_eq!(inspection.counts.unwrap().tool_definitions, Some(45));
        assert_eq!(inspection.retention, Retention::ListsLimited);
        assert!(bytes(&event) == serde_json::to_vec(&event).unwrap().len());
    }
}
