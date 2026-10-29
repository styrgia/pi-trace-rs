# Proposal

## Why

Pi traces contain useful timing, tool and subagent data, but investigating a slow run requires navigating a generated report. A Rust analysis engine and web client web interface can make that investigation easier while keeping traces on the user's machine.

## What Changes

- Establish an MIT-licensed Rust workspace, web application and contributor workflow.
- Import recorded JSONL with source diagnostics and explicit session epochs and turn attempts.
- Normalize the trace into an immutable graph with timing and usage summaries.
- Serve a local web explorer with a timeline, hierarchy, search, error filters and a details panel.
- Start with offline import. Live capture, hosted accounts, report sharing and support for other coding agents are outside this change.

## Capabilities

### New Capabilities

- `trace-import`: Read upstream trace files and preserve correlation and diagnostics.
- `trace-analysis`: Derive graph, timing and usage views with explicit measurement semantics.
- `trace-explorer`: Inspect imported traces through a local web client web interface.

### Modified Capabilities

None. This is a new project.

## Impact

New Rust crates under `crates/`, a web client application under `web/`, local HTTP endpoints and synthetic compatibility fixtures. The upstream collector remains an external input producer. Its MIT notice and pinned compatibility reference are retained. No existing user trace or research data is included.
