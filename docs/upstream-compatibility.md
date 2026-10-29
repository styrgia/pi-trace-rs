# Upstream compatibility

Compatibility target: [pi-trace-extension at 5441bcca041e3b3a203aee91c68e8ea809e6aa27](https://github.com/npxcnency-ux/pi-trace-extension/tree/5441bcca041e3b3a203aee91c68e8ea809e6aa27). The importer is planned and does not exist in the bootstrap.

## Input and event identity

The [collector](https://github.com/npxcnency-ux/pi-trace-extension/blob/5441bcca041e3b3a203aee91c68e8ea809e6aa27/extensions/trace/index.ts) writes a root `events.jsonl` and child files under `subagents/<child-id>/events.jsonl`. Common fields are `ts` in Unix milliseconds, `sessionId`, `type` and optional `turnIndex` and `stepIndex`. The format has no global event ID or schema version.

Lifecycle records include `session_start`, `interaction_start`, `turn_start`, `step_start`, `llm_request`, `llm_response`, `step_end`, `tool_start`, `tool_end`, `file_change`, `turn_summary`, `turn_end` and `session_shutdown`. Other event types must remain recoverable diagnostics. Tool events use `toolCallId`. A subagent result can carry `childTraceId`.

A repeated session start creates a new epoch, even when indices restart. A repeated turn start can represent a retry. Neither case permits overwriting an earlier occurrence. Ordering in a source file is retained when timestamps are unreliable.

## Measurements

- `llm_response.durationMs` measures request-to-HTTP-headers. It does not measure the whole generation.
- `step_end.durationMs` covers the generation step through assistant message completion.
- Exact first-token latency is not persisted by this baseline.
- `step_end.usage` and `turn_summary.usage` overlap. They cannot simply be added.
- Missing usage, cost or end events remain unknown or incomplete.

## Content handling

The collector's raw writer uses JSON serialization. Redaction performed by the [Python renderer](https://github.com/npxcnency-ux/pi-trace-extension/blob/5441bcca041e3b3a203aee91c68e8ea809e6aa27/extensions/trace/trace_to_html.py) does not establish that raw JSONL is sanitized. Some content fields are truncated upstream, but that is not a global bound on every event field.

Never commit real recordings as fixtures. Test data must fabricate IDs, paths, prompts and credentials. A future content preview must use text rendering and explicit limits.

## Capture boundary

[Pi extensions](https://github.com/earendil-works/pi/blob/e7a0df27c09d11bf4161d3605246cbf57a129cb2/packages/coding-agent/docs/extensions.md) run as TypeScript. This Rust application initially consumes recordings from an external collector. Pi's RPC mode could support a future adapter, but it is a different event source and must not be described as having identical provider-hook capture.
