# Tasks

## 1. Establish the visual and integration contract

- [x] 1.1 Reconcile the bootstrap design/tasks with this change's ownership map, startup-order refinement, and snapshot-scoped API; verify every unfinished bootstrap scenario has one implementation owner and none is marked complete prematurely.
- [x] 1.2 Finish the HTML reference's Hallmark findings and disclosure checks, then capture desktop/narrow, light/dark, and revealed-content states; verify app-scope render/kept checks have no unresolved Floor or Reflex findings and preserve source/measurement information.
- [x] 1.3 Add fabricated JSONL reference and edge-case fixtures with expected graph, measurements, diagnostics, and source lines; verify the fixtures contain no real recording data and document how each expectation follows from the compatibility baseline.
- [x] 1.4 Define Rust application-view DTOs, their TypeScript boundary shape, and the import/load-state contracts from the design; verify concrete examples cover ready/empty/failed, unknown/invalid timing, partial usage, pagination, and unavailable/truncated content without raw bodies in metadata.

## 2. Implement bounded offline import

- [x] 2.1 Add `crates/trace-core` to the Rust workspace with one `Trace::open` entry point and JSON decoding dependencies; verify the workspace builds and the public boundary does not require callers to sequence internal parsing stages.
- [x] 2.2 Read the selected root and recognized child sources without modifying them; verify relative source references, unreadable/missing roots, escaping symlinks, and observable source changes with synthetic filesystem cases.
- [x] 2.3 Enforce 128 MiB total, 1 MiB per physical record, 100,000 records, and 1,024 sources before unbounded decoding/traversal; verify each boundary and its over-limit case, including malformed/unknown records counted against limits.
- [x] 2.4 Decode supported lifecycle/tool/usage records and retain file/line provenance; verify happy-path records and distinct malformed-middle, truncated-tail, unknown-type, and missing-end diagnostics without echoing raw content.
- [x] 2.5 Update `docs/upstream-compatibility.md` with implemented input behavior and limits, preserving `THIRD_PARTY_NOTICES.md`; verify documented source shapes against importer tests and retain MIT notice for any ported logic.

## 3. Implement semantic snapshots and queries

- [x] 3.1 Build occurrence identity for epochs, interactions, turn attempts, generations, and tools; verify repeated session/turn indices remain separate and tool-call correlation cannot cross an occurrence boundary.
- [x] 3.2 Correlate explicit child IDs and represent unresolved relationships; verify reversed parallel-child completion, ambiguous/missing identifiers, and unlinked display groups without invented execution parents.
- [x] 3.3 Implement recorded, derived, unknown, and invalid timing views plus the global timeline domain; verify different response-header/generation durations, missing TTFT/end measurements, backward clocks, and rows without valid bars.
- [x] 3.4 Implement usage coverage and aggregation; verify step-plus-summary deduplication, summary-only fallback, partial generation coverage, legitimate zero values, and unknown cost.
- [x] 3.5 Derive final outcomes independently from tool errors and expose only supported relationships; verify a completed interaction with an earlier failure, an incomplete ending, and omission of an unsupported retry target.
- [x] 3.6 Implement bounded ordered metadata search, filters, ancestor closure, overview, details, diagnostics, and content previews; verify long ancestor chains across pages, stable snapshot/query cursors, response budgets, absent fields, and valid UTF-8 truncation at 8 KiB.
- [x] 3.7 Document implemented timing, usage, hierarchy, and uncertainty meanings alongside the compatibility notes; verify examples against serialized Rust views rather than the HTML constants.

## 4. Expose the real Rust service contract

- [x] 4.1 Add empty/importing/ready/failed workspace state and one blocking import worker; verify health/shell availability during import, a ready empty recording, fatal errors without partial snapshots, and publication of a completed immutable snapshot.
- [x] 4.2 Add failed-source-only `POST /api/v1/retry`; verify repair of the configured root followed by success, simultaneous retry conflict, no-source/ready/importing rejection, and refusal of bodies or path-changing parameters.
- [x] 4.3 Add workspace, overview, observation-page, node-detail, content, and diagnostic routes from `design.md`; verify them against an actual imported fixture, snapshot IDs, query limits, and distinct JSON errors for stale snapshots, missing nodes, and unknown API paths.
- [x] 4.4 Apply Host/Origin checks and no-store responses before trace access; verify approved production/development requests, absent-Origin reads, denied absent-Origin retry, unrecognized/null origins, unrelated Hosts, and no permissive CORS.
- [x] 4.5 Verify metadata and diagnostics contain no prompt/argument/result bodies or leaked decoder snippets, and content is returned only by its explicit route; use fabricated secrets, commands, and markup to check every response family and captured logs.
- [x] 4.6 Add the typed client decoder contract checks using actual Rust-serialized fixtures, and document the versioned endpoints/errors/limits; verify incompatible DTOs fail visibly and valid optional/unknown states decode without browser-side semantic calculation.

## 5. Connect source selection to the existing package launcher

- [x] 5.1 Add native `serve <directory>` and no-source startup while preserving package-relative assets and health readiness; verify arguments with spaces, invalid syntax, a failed selected source, and no-argument empty mode against the real executable.
- [x] 5.2 Forward source arguments through `scripts/start.mjs` and `scripts/lib/launcher.mjs`, resolving paths against the invocation directory before package cwd is applied; verify same-source repeated start, different-source refusal, and existing collision/cancellation/cleanup tests.
- [x] 5.3 Add `/pi-trace serve <directory>` using the Pi session working directory and shell-free arguments; verify path spaces, start/build/stop ownership, passive registration, and session shutdown with an extension-host fixture and a real launch smoke check.
- [x] 5.4 Document the new source-selection commands and failed-import retry/reconnect workflow in README/packaging guidance; verify the commands from another working directory and keep current capabilities distinct from remaining planned work.

## 6. Build the Svelte workbench against live Rust views

- [x] 6.1 Replace the starter layout with workbench tokens, compact summary/session disclosure, toolbar, and meaningful region components; verify reference desktop/narrow captures, both themes, readable type, and absence of duplicate overview or product preview-state controls.
- [x] 6.2 Load real workspace/overview/query data through typed requests, configure the Vite proxy Host rewrite, and cancel obsolete responses; verify the production same-origin page and Vite development page against Rust with no embedded observation fixture.
- [x] 6.3 Implement shared hierarchy/waterfall/list selection, collapse, metadata search, structural filters, counts, and appendable continuation; verify filtered/collapsed selection notices, ancestor context across pages, and no silent partial result.
- [x] 6.4 Implement inspector measurements, metadata, diagnostics, and one explicit input/output reveal action with text bindings; verify no content request before reveal, inert markup, unavailable/truncated fields, source-line navigation, and no unsupported retry links.
- [x] 6.5 Implement actual no-source/importing/empty/import-failure/no-match/service-unavailable states, failed-import retry, and reconnection; verify repair/retry success and that obsolete attempts or snapshot responses never overwrite current report state.
- [x] 6.6 Implement local view links and memory-only search/reveal state; verify selection/filter/view/tab/theme restoration, absent query/source/body text in links, hidden content after reload, and an explicit stale-snapshot notice.
- [x] 6.7 Complete keyboard navigation, separate disclosure/selection controls, visible focus, status announcements, and responsive stacking; verify the full investigation without a pointer at desktop and 375 px in both themes, then run app-scope Hallmark checks on the real Svelte route.
- [x] 6.8 Update investigation documentation with implemented controls and measurement limits; verify the documented sequence against the live Rust-backed UI and remove bootstrap-only UI claims only after the corresponding behavior works.

## 7. Verify the integrated application and package

- [x] 7.1 Run the complete recording-to-browser investigation with the real importer, including recovered tool failure, malformed input, uncertain children, pagination, content reveal, stale links, and backend loss; record screenshots/request evidence and verify no external requests or source-file modifications.
- [x] 7.2 Observe import and view behavior on a declared bounded synthetic workload; record environment, workload, limits, and results, keeping correctness, measurements, and performance claims separate.
- [x] 7.3 Run `npm run check` and audit every scenario in both new specs and the mapped bootstrap specs; verify only observed behavior is checked complete and retain unrelated working-tree changes.
- [x] 7.4 Inspect `npm pack` contents and run the extracted package outside the source checkout with its matching bundled binary/assets; verify no build tools or checkout are needed, source selection works from another directory, no-argument mode works, and fixtures/traces/agent audit files are excluded.
- [x] 7.5 Review the implementation diff, notices, documentation, and release metadata; before any implementation commit, use the repository's `npm version` workflow when a release bump is needed and run `npm run version:check`, without creating a commit, tag, or publication implicitly.

## Workflow follow-up

- Archive this change only after its scenarios and mapped bootstrap work pass. Do not archive incomplete bootstrap work merely because the Svelte screen renders.
- Publish or push only under a separate explicit maintainer request. Packaging verification is not publication.

## 8. Inspect tool calls and request context

The user requested this extension after the initial 39 tasks passed. Those results remain the baseline; the tasks below are additional acceptance work.

- [x] 8.1 Extract bounded, body-free request composition in Rust before preview truncation; verify supported request shapes, tagged instruction sections, tool and skill descriptor sizes, unknown/partial states, non-overlapping accounting, source lines, and explicit retention/display limits using synthetic records.
- [x] 8.2 Retain tool invocation evidence and size provenance; verify the tool name, call identifier, argument/result availability, preview sizes, and recorded original UTF-16 result length without confusing the latter with model-visible text or leaking arguments into metadata.
- [x] 8.3 Extend Rust application views and runtime TypeScript decoders with serialization contracts; verify incompatible fields fail visibly, request-specific input/cache usage stays distinct from cumulative session usage, and no per-component token estimate is invented.
- [x] 8.4 Add discoverable Context inspection and tool-specific details using the shared selection and existing explicit reveal; verify generation navigation, largest components/descriptors, inert arguments/results, unavailable/truncated states, safe links, and cancellation/reset behavior.
- [x] 8.5 Investigate the fresh Pi recording through the actual Rust-backed browser, report which tools ran and what occupies context, and capture tool/context screens; verify keyboard use, both themes, 375 px layout, no automatic body fetches, and unchanged source recordings.
- [x] 8.6 Update investigation/API documentation and Unreleased notes, run npm run check, inspect and launch the extracted package, and complete independent review; preserve unrelated work, notices, release version, and the running Pi session.
