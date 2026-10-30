# Proposal

## Why

Users need to inspect a running trace and see token usage on every span without reopening the recording or selecting each span. The current application imports one offline snapshot, omits usage from execution rows, and leaves tool usage unknown even when an explicitly linked child recording contains model usage.

## What Changes

- Add an explicit realtime mode that follows complete records appended to a startup-selected recording directory, including child recordings. Preserve offline import as the default.
- Publish coherent immutable revisions while a recording grows. Keep incomplete writes pending, distinguish waiting from failure, and retain the last valid revision when following stops on an error or limit.
- Refresh the workbench automatically while preserving selection, filters, expanded rows, loaded investigation context, and keyboard focus where the selected span still exists. Provide a pause control for investigation and a resume control for the latest revision.
- Include token accounting in every execution row and span detail for sessions, interactions, turns, generations, and tools. Distinguish a span's own recorded usage from its inclusive descendant usage.
- Attribute child model usage to a tool only through a confirmed unique child relationship. Calculate overview totals from unique usage contributions, independent of repeated parent totals and tree placement.
- Preserve input, output, cache-read, and cache-write counts separately. Display recorded zero, partial evidence, unknown usage, and non-applicable own usage distinctly. Keep source-file and line provenance and diagnose invalid arithmetic.
- Verify append behavior, late completions, fallback replacement, child linking, browser continuity, source boundaries, and installed-package startup with synthetic recordings.

The working assumption for this proposal is that another collector writes the supported `events.jsonl` format. The user's request does not explicitly select a producer. Adding capture to the Pi extension would require a separate recording contract and a scope revision. No capture, tokenizer estimates, component-level token allocation, pricing catalogue, or hosted service is proposed here.

## Capabilities

### New Capabilities

- `live-trace-following`: Explicit follow mode, bounded append ingestion, revision and source lifecycles, coherent API refresh, and browser pause and resume behavior.
- `span-token-accounting`: Recorded and inclusive token accounting on every span, contribution identity, fallback precedence, child attribution, completeness, provenance, and visible usage states.

### Modified Capabilities

None. `openspec list --specs --json` currently returns no published capabilities. These additions build on the offline semantics in `bootstrap-trace-explorer` and the snapshot API and UI in `implement-svelte-trace-workbench`.

Those existing changes explicitly exclude live updates. This change adds a separate opt-in mode rather than treating their completed offline work as realtime support. Their import, timing, privacy, package, and versioning guarantees remain the baseline.

## Impact

Implementation affects `crates/trace-core` ingestion, correlation projections, accounting, queries, and view contracts. It also affects `crates/pi-trace` lifecycle and routes, `scripts/start.mjs`, `scripts/lib/launcher.mjs`, `extensions/pi-trace.mjs`, the Svelte API decoder and workbench, and their synthetic tests and contract examples.

The design adds mode and revision metadata, stable live span identity, and explicit per-span accounting. Existing snapshot-qualified routes and legacy detail usage retain their meaning. Launcher process reuse must include both source and mode. The first transport uses bounded polling and needs no streaming protocol or collector dependency.

Implementation must update the README, API and compatibility documentation, and OpenSpec context after the new behavior is verified. The root package continues to own the release version. Package extraction and live startup are required before distribution. Creating this proposal implements no product behavior and authorizes no release.
