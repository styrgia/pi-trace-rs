use super::{session::*, types::*, usage::*};
use crate::views::*;
use serde_json::Value;
fn text<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}
fn entity_id(v: &Value, source: &NativeSource) -> String {
    text(v, "id")
        .or_else(|| text(v, "uuid"))
        .map(str::to_owned)
        .unwrap_or_else(|| {
            format!(
                "source-{}",
                identity(&[&serde_json::to_string(source).unwrap()])
            )
        })
}
fn duration(entity: &mut Entity, v: Option<&Value>) {
    if let Some(v) = v {
        entity.duration = match v.as_f64() {
            Some(n) if n.is_finite() && n >= 0.0 => Measurement {
                state: MeasurementState::Recorded,
                value_ms: Some(n),
                meaning: "recorded monotonic duration".into(),
            },
            _ => Measurement {
                state: MeasurementState::Invalid,
                value_ms: None,
                meaning: "invalid recorded monotonic duration".into(),
            },
        };
    }
}
fn relation(entity: &mut Entity, v: &Value, key: &str, kind: &str, source: &NativeSource) {
    if let Some(target) = text(v, key) {
        entity.relationships.push(NativeRelationship {
            kind: kind.into(),
            target: target.into(),
            source: source.clone(),
            execution: false,
        });
    }
}
impl SessionState {
    pub fn decode(&mut self, v: &Value, source: NativeSource) {
        self.source_names.insert(match &source {
            NativeSource::Jsonl { source, .. }
            | NativeSource::CompressedJsonl { source, .. }
            | NativeSource::Json { source, .. } => source.clone(),
            NativeSource::Sqlite { database, .. } => database.clone(),
        });
        self.recorded_ordinal = v.get("ordinal").and_then(Value::as_u64);
        self.records += 1;
        self.ordinal = v
            .get("ordinal")
            .and_then(Value::as_u64)
            .unwrap_or(self.ordinal + 1);
        match self.descriptor.provider {
            Provider::Pi => self.pi(v, source.clone()),
            Provider::Claude => self.claude(v, source.clone()),
            Provider::Codex => self.codex(v, source.clone()),
            Provider::Opencode => self.opencode(v, source.clone()),
        }
        if let Some(timestamp) = v.get("timestamp") {
            let value = timestamp
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| timestamp.to_string());
            for entity in self.entities.values_mut().filter(|e| e.source == source) {
                entity.timing.push(TimingEvidence {
                    meaning: TimestampMeaning::Persistence,
                    value: value.clone(),
                    unit: if timestamp.is_string() {
                        "rfc3339"
                    } else {
                        "millisecondsSinceEpoch"
                    }
                    .into(),
                    source: source.clone(),
                });
            }
        }
    }
    fn pi(&mut self, v: &Value, source: NativeSource) {
        let kind = text(v, "type").unwrap_or("");
        let id = entity_id(v, &source);
        self.selected_context = Some(id.clone());
        match kind {
            "session" => {}
            "message" => {
                let m = &v["message"];
                let role = text(m, "role").unwrap_or("");
                let key = if role == "toolResult" {
                    format!("tool:{}", text(m, "toolCallId").unwrap_or(&id))
                } else {
                    format!("message:{id}")
                };
                let mut e = self.entities.get(&key).cloned().unwrap_or_else(|| {
                    Entity::new(
                        &key,
                        if role == "assistant" {
                            NodeKind::Generation
                        } else if role == "toolResult" {
                            NodeKind::Tool
                        } else {
                            NodeKind::Message
                        },
                        match role {
                            "assistant" => "Assistant",
                            "toolResult" => "Tool result",
                            "user" => "User",
                            _ => "Message",
                        },
                        source.clone(),
                    )
                });
                e.source = source.clone();
                e.sources.push(source.clone());
                e.metadata.model = text(m, "model").map(str::to_owned);
                if matches!(role, "assistant" | "toolResult") {
                    duration(&mut e, m.get("durationMs"));
                    if let Some(value) = m.get("timestamp").and_then(Value::as_u64) {
                        e.timing.push(TimingEvidence {
                            meaning: if role == "assistant" {
                                TimestampMeaning::RequestStart
                            } else {
                                TimestampMeaning::ResultCreation
                            },
                            value: value.to_string(),
                            unit: "millisecondsSinceEpoch".into(),
                            source: source.clone(),
                        });
                    }
                }
                e.output = content(m.get("content"));
                e.output_source =
                    (e.output.state == ContentState::Available).then(|| source.clone());
                e.outcome = match text(m, "stopReason") {
                    Some("error" | "aborted") => Outcome::Error,
                    Some("stop" | "toolUse" | "length") => Outcome::Success,
                    _ if role == "toolResult" => {
                        if m["isError"].as_bool() == Some(true) {
                            Outcome::Error
                        } else {
                            Outcome::Success
                        }
                    }
                    _ => Outcome::Unknown,
                };
                relation(&mut e, v, "parentId", "contextParent", &source);
                self.put(e);
                if let Some(usage) = m.get("usage") {
                    self.claim(
                        &format!("entry:{id}"),
                        &key,
                        usage,
                        source.clone(),
                        if role == "assistant" {
                            "modelResponse"
                        } else {
                            "toolResult"
                        },
                        1,
                    );
                }
                if let Some(blocks) = m.get("content").and_then(Value::as_array) {
                    for b in blocks {
                        if text(b, "type") == Some("toolCall")
                            && let Some(call) = text(b, "id")
                        {
                            let k = format!("tool:{call}");
                            let mut e = Entity::new(
                                &k,
                                NodeKind::Tool,
                                text(b, "name").unwrap_or("Tool"),
                                source.clone(),
                            );
                            e.parent = Some(key.clone());
                            e.input = content(b.get("arguments"));
                            e.input_source =
                                (e.input.state == ContentState::Available).then(|| source.clone());
                            e.metadata.tool_name = text(b, "name").map(str::to_owned);
                            self.put(e);
                        }
                    }
                }
                if let Some(calls) = m.pointer("/nestedCalls/calls").and_then(Value::as_array) {
                    for call in calls {
                        if let Some(id) = text(call, "id") {
                            let nested = format!("nested:{key}:{id}");
                            let mut e = Entity::new(
                                &nested,
                                NodeKind::Tool,
                                text(call, "name").unwrap_or("Nested tool"),
                                source.clone(),
                            );
                            e.parent = Some(key.clone());
                            e.input = content(call.get("arguments"));
                            e.input_source =
                                (e.input.state == ContentState::Available).then(|| source.clone());
                            duration(&mut e, call.get("durationMs"));
                            e.outcome = match text(call, "status") {
                                Some("ok") => Outcome::Success,
                                Some("error") => Outcome::Error,
                                Some("unfinished") => Outcome::Incomplete,
                                _ => Outcome::Unknown,
                            };
                            self.put(e);
                        }
                    }
                }
                if let Some(calls) = m.get("calls").and_then(Value::as_array) {
                    for (i, call) in calls.iter().enumerate() {
                        if let Some(usage) = call.get("usage") {
                            let mut c = UsageClaim::parse(
                                Provider::Pi,
                                &key,
                                &format!("nested:{id}:{i}"),
                                usage,
                                source.clone(),
                                UsageRole::Breakdown {
                                    included_in: format!("entry:{id}"),
                                },
                            );
                            c.view.authority = "piToolResultNestedBreakdown".into();
                            self.ledger.put(c.view.id.clone(), c);
                        }
                    }
                }
            }
            "usage" | "compaction" | "branch_summary" => {
                let key = format!("entry:{id}");
                let mut e = Entity::new(&key, NodeKind::Message, kind, source.clone());
                relation(&mut e, v, "parentId", "contextParent", &source);
                e.output = content(v.get("summary"));
                e.output_source =
                    (e.output.state == ContentState::Available).then(|| source.clone());
                self.put(e);
                if let Some(u) = v.get("usage") {
                    self.claim(&key, &key, u, source, kind, 1);
                }
            }
            "model_change"
            | "thinking_level_change"
            | "custom"
            | "custom_message"
            | "session_info"
            | "label" => {}
            _ => self.diagnostic("unsupported_record", source),
        }
    }
    fn claude(&mut self, v: &Value, source: NativeSource) {
        match text(v, "type").unwrap_or("") {
            "assistant" | "user" => {
                let m = &v["message"];
                let assistant = text(v, "type") == Some("assistant");
                let id = text(m, "id")
                    .map(str::to_owned)
                    .unwrap_or_else(|| entity_id(v, &source));
                let key = format!("message:{id}");
                let mut e = Entity::new(
                    &key,
                    if assistant {
                        NodeKind::Generation
                    } else {
                        NodeKind::Message
                    },
                    if assistant { "Assistant" } else { "User" },
                    source.clone(),
                );
                e.metadata.model = text(m, "model").map(str::to_owned);
                e.output = content(m.get("content"));
                e.output_source =
                    (e.output.state == ContentState::Available).then(|| source.clone());
                relation(&mut e, v, "parentUuid", "contextParent", &source);
                relation(&mut e, v, "logicalParentUuid", "compactionContext", &source);
                self.put(e);
                if let Some(u) = m.get("usage") {
                    self.claim(&key, &key, u, source.clone(), "modelResponse", 1);
                }
                if let Some(blocks) = m.get("content").and_then(Value::as_array) {
                    for b in blocks {
                        match text(b, "type") {
                            Some("tool_use") => {
                                if let Some(id) = text(b, "id") {
                                    let k = format!("tool:{id}");
                                    let mut e = Entity::new(
                                        &k,
                                        NodeKind::Tool,
                                        text(b, "name").unwrap_or("Tool"),
                                        source.clone(),
                                    );
                                    e.parent = Some(key.clone());
                                    e.input = content(b.get("input"));
                                    e.input_source = (e.input.state == ContentState::Available)
                                        .then(|| source.clone());
                                    e.metadata.tool_name = text(b, "name").map(str::to_owned);
                                    self.put(e);
                                }
                            }
                            Some("tool_result") => {
                                if let Some(id) = text(b, "tool_use_id") {
                                    let k = format!("tool:{id}");
                                    let mut e =
                                        self.entities.get(&k).cloned().unwrap_or_else(|| {
                                            Entity::new(
                                                &k,
                                                NodeKind::Tool,
                                                "Tool result",
                                                source.clone(),
                                            )
                                        });
                                    e.sources.push(source.clone());
                                    e.output = content(b.get("content"));
                                    e.output_source = (e.output.state == ContentState::Available)
                                        .then(|| source.clone());
                                    e.outcome = if b["is_error"].as_bool() == Some(true) {
                                        Outcome::Error
                                    } else {
                                        Outcome::Success
                                    };
                                    self.put(e);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            "summary" | "result" => {
                if let Some(u) = v.get("usage") {
                    let key = entity_id(v, &source);
                    let c = UsageClaim::parse(
                        Provider::Claude,
                        "session",
                        &key,
                        u,
                        source,
                        UsageRole::Reconciliation {
                            scope: "resultSummary".into(),
                        },
                    );
                    self.ledger.put(key, c);
                }
            }
            "system" | "progress" | "file-history-snapshot" | "queue-operation" | "last-prompt" => {
            }
            _ => self.diagnostic("unsupported_record", source),
        }
    }
    fn codex(&mut self, v: &Value, source: NativeSource) {
        let kind = text(v, "type").unwrap_or("");
        let p = v.get("payload").unwrap_or(v);
        match kind {
            "session_meta" => {
                self.inherited_before = p
                    .get("forked_from_ordinal_exclusive")
                    .and_then(Value::as_u64)
                    .into_iter()
                    .chain(
                        p.get("subagent_history_start_ordinal")
                            .and_then(Value::as_u64),
                    )
                    .max();
                self.attribution_partial |= p.get("forked_from_id").is_some_and(|v| !v.is_null())
                    && self.inherited_before.is_none();
            }
            "response_item" => self.codex_item(p, source),
            "event_msg" => match text(p, "type").unwrap_or("") {
                "token_usage_record" => self.codex_usage(p, source),
                "token_count" => {
                    if let Some(u) = p.pointer("/info/total_token_usage") {
                        let context =
                            text(p, "context_window_id").or_else(|| text(p, "context_id"));
                        let key = format!("cumulative:{}", context.unwrap_or("unknown"));
                        let c = UsageClaim::parse(
                            Provider::Codex,
                            "session",
                            &key,
                            u,
                            source.clone(),
                            UsageRole::Reconciliation { scope: key.clone() },
                        );
                        self.ledger.put(key.clone(), c);
                        if let Some(context) = context {
                            if let Some((previous, prev_source)) =
                                self.cumulative.get(context).cloned()
                            {
                                if previous == *u {
                                    return;
                                }
                                let fields = [
                                    "input_tokens",
                                    "output_tokens",
                                    "cached_input_tokens",
                                    "cache_write_input_tokens",
                                    "total_tokens",
                                    "reasoning_output_tokens",
                                ];
                                let mut delta = serde_json::Map::new();
                                let mut valid = true;
                                for f in fields {
                                    match (u[f].as_u64(), previous[f].as_u64()) {
                                        (Some(a), Some(b)) if a >= b => {
                                            delta.insert(f.into(), Value::from(a - b));
                                        }
                                        (None, None) => {}
                                        _ => valid = false,
                                    }
                                }
                                if valid {
                                    let id = format!(
                                        "delta:{}:{}",
                                        context,
                                        identity(&[&u.to_string()])
                                    );
                                    self.claim(
                                        &id,
                                        "session",
                                        &Value::Object(delta),
                                        source.clone(),
                                        "cumulativeDelta",
                                        1,
                                    );
                                    if let Some(c) = self.ledger.claims.get_mut(&id) {
                                        c.view.normalization.push(Normalization {
                                            component: "allComparableCounters".into(),
                                            rule: "differenceCumulativeSameContext".into(),
                                            operands: vec![NativeCounter {
                                                name: "previousTotal".into(),
                                                value: crate::accounting::Counter::from_amount(
                                                    &previous["total_tokens"].as_u64().map_or(
                                                        crate::accounting::Amount::Missing,
                                                        crate::accounting::Amount::Known,
                                                    ),
                                                    false,
                                                )
                                                .view(),
                                                source: prev_source,
                                            }],
                                        });
                                    }
                                } else {
                                    self.diagnostic("usage_discontinuity", source.clone());
                                }
                            }
                            self.cumulative.insert(context.into(), (u.clone(), source));
                        }
                    }
                }
                "item_completed" => {
                    if let Some(item) = p.get("item") {
                        self.codex_completed(p, item, source)
                    }
                }
                "agent_message" | "user_message" | "task_started" | "task_complete"
                | "turn_aborted" | "context_compacted" | "agent_reasoning"
                | "exec_command_begin" | "exec_command_end" => {}
                _ => self.diagnostic("unsupported_record", source),
            },
            "token_usage_record" => self.codex_usage(p, source),
            "item_completed" => self.codex_completed(p, p.get("item").unwrap_or(p), source),
            "turn_context" | "compacted" => {}
            _ => self.diagnostic("unsupported_record", source),
        }
    }
    fn codex_usage(&mut self, p: &Value, source: NativeSource) {
        let Some(id) = text(p, "response_id") else {
            self.diagnostic("unsupported_record", source);
            return;
        };
        let context = text(p, "session_id").unwrap_or("unknown");
        let key = format!("response:{context}:{id}");
        if !self.entities.contains_key(&key) {
            self.put(Entity::new(
                &key,
                NodeKind::Generation,
                "Model response",
                source.clone(),
            ));
        }
        if let Some(u) = p.get("usage") {
            self.claim(&key, &key, u, source.clone(), "modelResponse", 2);
        }
        for scope in ["turn_token_usage", "thread_token_usage"] {
            if let Some(u) = p.get(scope) {
                let k = format!("{scope}:{context}:{id}");
                self.ledger.put(
                    k.clone(),
                    UsageClaim::parse(
                        Provider::Codex,
                        &key,
                        &k,
                        u,
                        source.clone(),
                        UsageRole::Reconciliation {
                            scope: scope.into(),
                        },
                    ),
                );
            }
        }
    }
    fn codex_completed(&mut self, event: &Value, item: &Value, source: NativeSource) {
        let Some(id) = text(item, "id") else {
            self.diagnostic("unsupported_record", source);
            return;
        };
        let kind = text(item, "type").unwrap_or("");
        let (key, node_kind, label) = match kind {
            "AgentMessage" | "UserMessage" | "Reasoning" | "Plan" | "ContextCompaction" => {
                (format!("item:{id}"), NodeKind::Message, kind)
            }
            "CommandExecution"
            | "DynamicToolCall"
            | "McpToolCall"
            | "CollabAgentToolCall"
            | "FunctionCallOutput" => (format!("tool:{id}"), NodeKind::Tool, kind),
            _ => {
                self.diagnostic("unsupported_record", source);
                return;
            }
        };
        let mut entity = self
            .entities
            .get(&key)
            .cloned()
            .unwrap_or_else(|| Entity::new(&key, node_kind, label, source.clone()));
        entity.source = source.clone();
        entity.sources.push(source.clone());
        entity.output = content(
            item.get("content")
                .or_else(|| item.get("aggregated_output"))
                .or_else(|| item.get("output")),
        );
        entity.output_source =
            (entity.output.state == ContentState::Available).then(|| source.clone());
        entity.input = content(item.get("arguments").or_else(|| item.get("command")));
        entity.input_source =
            (entity.input.state == ContentState::Available).then(|| source.clone());
        if let (Some(start), Some(end)) = (
            event["started_at_ms"].as_i64(),
            event["completed_at_ms"].as_i64(),
        ) && end > 0
        {
            let elapsed = end.checked_sub(start).filter(|duration| *duration >= 0);
            entity.duration = Measurement {
                state: if elapsed.is_some() {
                    MeasurementState::Derived
                } else {
                    MeasurementState::Invalid
                },
                value_ms: elapsed.map(|v| v as f64),
                meaning: "persisted item execution boundaries".into(),
            };
            if elapsed.is_none() {
                self.diagnostic("invalid_timing", source.clone());
            }
            entity.timing.push(TimingEvidence {
                meaning: TimestampMeaning::ExecutionStart,
                value: start.to_string(),
                unit: "millisecondsSinceEpoch".into(),
                source: source.clone(),
            });
            entity.timing.push(TimingEvidence {
                meaning: TimestampMeaning::ExecutionEnd,
                value: end.to_string(),
                unit: "millisecondsSinceEpoch".into(),
                source: source.clone(),
            });
        }
        self.put(entity);
    }
    fn codex_item(&mut self, p: &Value, source: NativeSource) {
        let kind = text(p, "type").unwrap_or("");
        match kind {
            "message" | "reasoning" => {
                let id = entity_id(p, &source);
                let key = format!("item:{id}");
                let mut e = Entity::new(
                    &key,
                    NodeKind::Message,
                    if text(p, "role") == Some("assistant") {
                        "Assistant"
                    } else {
                        "Message"
                    },
                    source.clone(),
                );
                e.output = content(p.get("content"));
                e.output_source =
                    (e.output.state == ContentState::Available).then(|| source.clone());
                self.put(e);
            }
            "function_call"
            | "custom_tool_call"
            | "function_call_output"
            | "custom_tool_call_output" => {
                let Some(id) = text(p, "call_id") else {
                    self.diagnostic("unsupported_record", source);
                    return;
                };
                let key = format!("tool:{id}");
                let mut e = self.entities.get(&key).cloned().unwrap_or_else(|| {
                    Entity::new(
                        &key,
                        NodeKind::Tool,
                        text(p, "name").unwrap_or("Tool"),
                        source.clone(),
                    )
                });
                e.sources.push(source.clone());
                if kind.ends_with("output") {
                    e.output = content(p.get("output"));
                    e.output_source =
                        (e.output.state == ContentState::Available).then(|| source.clone());
                } else {
                    e.input = content(p.get("arguments").or_else(|| p.get("input")));
                    e.input_source =
                        (e.input.state == ContentState::Available).then(|| source.clone());
                    e.metadata.tool_name = text(p, "name").map(str::to_owned);
                }
                self.put(e);
            }
            _ => self.diagnostic("unsupported_record", source),
        }
    }
    fn opencode(&mut self, v: &Value, source: NativeSource) {
        let kind = text(v, "type").unwrap_or("");
        let id = entity_id(v, &source);
        let key = format!("message:{id}");
        if kind == "session.usage.recorded" {
            if let Some(u) = v.get("tokens") {
                self.claim(
                    &format!("event:{id}"),
                    "session",
                    u,
                    source.with_field("/tokens"),
                    text(v, "source").unwrap_or("auxiliary"),
                    2,
                );
            }
            return;
        }
        if !matches!(
            kind,
            "assistant" | "user" | "compaction" | "shell" | "synthetic" | "skill"
        ) {
            self.diagnostic("unsupported_record", source);
            return;
        }
        let mut e = Entity::new(
            &key,
            if kind == "assistant" {
                NodeKind::Generation
            } else {
                NodeKind::Message
            },
            kind,
            source.clone(),
        );
        e.metadata.model = v
            .pointer("/model/id")
            .and_then(Value::as_str)
            .map(str::to_owned);
        e.output = content(v.get("content").or_else(|| v.get("summary")));
        e.output_source = (e.output.state == ContentState::Available).then(|| {
            source.with_field(if v.get("content").is_some() {
                "/content"
            } else {
                "/summary"
            })
        });
        if v.get("finish").is_some() {
            e.outcome = Outcome::Success;
        }
        if v.get("error").is_some() {
            e.outcome = Outcome::Error;
        }
        self.put(e);
        if let Some(u) = v.get("tokens") {
            if kind == "compaction" {
                let c = UsageClaim::parse(
                    Provider::Opencode,
                    &key,
                    &key,
                    u,
                    source.with_field("/tokens"),
                    UsageRole::Reconciliation {
                        scope: "compactionDurableUsage".into(),
                    },
                );
                self.ledger.put(key.clone(), c);
            } else {
                self.claim(
                    &key,
                    &key,
                    u,
                    source.with_field("/tokens"),
                    "modelResponse",
                    1,
                );
            }
        }
        if let Some(blocks) = v.get("content").and_then(Value::as_array) {
            for (ordinal, b) in blocks.iter().enumerate() {
                if text(b, "type") == Some("tool")
                    && let Some(id) = text(b, "id")
                {
                    let k = format!("tool:{id}");
                    let mut e = Entity::new(
                        &k,
                        NodeKind::Tool,
                        text(b, "name").unwrap_or("Tool"),
                        source.clone(),
                    );
                    e.source = source.with_field(&format!("/content/{ordinal}"));
                    e.sources[0] = e.source.clone();
                    e.parent = Some(key.clone());
                    e.metadata.tool_name = text(b, "name").map(str::to_owned);
                    e.input = content(b.pointer("/state/input"));
                    e.input_source = (e.input.state == ContentState::Available)
                        .then(|| source.with_field(&format!("/content/{ordinal}/state/input")));
                    e.output = content(b.pointer("/state/content"));
                    e.output_source = (e.output.state == ContentState::Available)
                        .then(|| source.with_field(&format!("/content/{ordinal}/state/content")));
                    e.outcome = match b.pointer("/state/status").and_then(Value::as_str) {
                        Some("completed") => Outcome::Success,
                        Some("error") => Outcome::Error,
                        Some("running" | "streaming") => Outcome::Incomplete,
                        _ => Outcome::Unknown,
                    };
                    if let (Some(a), Some(z)) = (
                        b.pointer("/time/ran").and_then(Value::as_f64),
                        b.pointer("/time/completed").and_then(Value::as_f64),
                    ) {
                        e.duration = Measurement {
                            state: if z >= a {
                                MeasurementState::Derived
                            } else {
                                MeasurementState::Invalid
                            },
                            value_ms: (z >= a).then_some(z - a),
                            meaning: "recorded tool execution boundaries".into(),
                        };
                    }
                    self.put(e);
                }
            }
        }
    }
}
