# Tasks

All tasks describe future implementation. Creating these artifacts completes none of them. Follow [design.md](design.md) and both capability specifications. The proposed producer is an external append-only collector.

## 1. Share bounded semantics and repeatable projections

- [x] 1.1 Extract physical-record decoding and event reduction from `crates/trace-core/src/import.rs` for reuse by offline and live readers. Verify existing semantics and inspection tests still pass with unchanged source-line, limit, and content boundaries.
- [x] 1.2 Separate retained evidence from projection of completion, relationships, diagnostics, and usage. Add tests proving repeated projection does not mutate evidence, duplicate diagnostics, or turn temporary EOF into final completion errors.
- [x] 1.3 Introduce run-scoped source and occurrence anchors and stable span identity in the core model. Test reused indices, synthetic parents, late source discovery, and changed child relationships without identity changes.
- [x] 1.4 Document offline EOF and live-prefix semantics in `docs/upstream-compatibility.md`. Verify the documented distinction against tests for newline-free offline input and provisional live spans.

## 2. Implement canonical per-span token accounting

- [x] 2.1 Add canonical generation and turn-summary contribution identities and fallback precedence. Test duplicate completions, reused turns, zero, total-only, partial, cost-only, recognized invalid evidence, and late fallback replacement without double counting.
- [x] 2.2 Calculate own and inclusive accounting over verified descendants and overview usage from canonical contributions. Test the 150-plus-40-token example, unresolved children, cycles, late links, ordinary tools, partial coverage, and invariant overview totals across filters and pagination.
- [x] 2.3 Add checked per-counter arithmetic, exact decimal-string values, and structural invalid-value, overflow, and conflicting-total diagnostics. Test integers above JavaScript's safe-number range, maximum supported values, missing components, and preservation of independent valid evidence and unknown cost.
- [x] 2.4 Extend row and detail DTOs with bounded `spanUsage`, and overview with exact `tokenAccounting`. Add Rust-serialized contract examples covering every span kind, state, provenance, 16-entry evidence cap, omitted counts, response limits, and existing valid-input legacy usage semantics.
- [x] 2.5 Document new counter units, own versus inclusive meaning, fallback, and contributor provenance in `docs/trace-api.md` and compatibility notes. Verify each documented shape against fresh contract serialization and keep context input separate from cumulative descendant usage.

## 3. Follow append-only sources safely

- [x] 3.1 Add live source cursors with identity, complete-line checkpoint, read high-water mark, line number, pending bytes, and cumulative budgets. Test split JSON, split UTF-8, newline-free valid tails, malformed complete lines, empty prefixes, and no-op scans without duplicate reads or contributions.
- [x] 3.2 Discover late child recordings under existing root containment and traversal rules. Test child-before-link arrival, reverse completion order, missing and ambiguous identifiers, escaping links, and cycles with stable span keys and source references.
- [x] 3.3 Detect supported file replacement, truncation, disappearance, same-size rewriting, and observed boundary mismatch. Test that failed updates preserve the last published snapshot and that reset uses a new run rather than combining source generations.
- [x] 3.4 Enforce cumulative byte, record, source, traversal, inspection, preview, and response limits, including pending and malformed input. Use exact-limit and one-over tests with repeated idle scans and pause backlogs to prove budgets never reset implicitly.
- [x] 3.5 Bound worker batches and observe cancellation between them. Test a catch-up backlog and shutdown without source writes, stranded workers, or publication of a rejected update.
- [x] 3.6 Document supported append-only producer behavior, identity support, discontinuity errors, and interior-rewrite detection limits in compatibility notes. Verify each claimed failure mode through a synthetic filesystem test.

## 4. Expose live lifecycle and immutable revisions

- [x] 4.1 Extend `crates/pi-trace/src/service.rs` with mode, run, revision, and follow state under one ingestion worker. Test initial empty and importing states, waiting, last-valid ready views after failure, unchanged health, monotonic revisions, and no-op revision stability.
- [x] 4.2 Retain two addressable snapshots and add stable-span detail lookup with the eight-handler bound. Test stale snapshots, cross-revision cursors, lookup outside a filtered page, structural busy responses, and absence of snapshot substitution.
- [x] 4.3 Add shared-workspace pause and resume controls and stopped-live retry. Test pause at a batch boundary, idempotent repeated controls, two clients, resume validation, bounded catch-up, one accepted concurrent retry, and reset run identity while preserving offline retry behavior.
- [x] 4.4 Apply existing Host, Origin, no-store, explicit reveal, payload, and response policies to every added route. Extend service tests with unapproved and absent Origin controls, unsupported body or query, forged source selection, structural errors, and fabricated private-body leak checks.
- [x] 4.5 Add workspace and control serialization examples and update `docs/trace-api.md` with states, retention, routes, and errors. Verify the examples against actual service serialization and HTTP responses.

## 5. Add mode-aware CLI and Pi launch commands

- [x] 5.1 Parse `watch <directory>` in the native CLI and `scripts/start.mjs` without changing existing invocation forms. Extend native and Node CLI tests for invalid syntax, complete paths with spaces, caller cwd, and offline startup.
- [x] 5.2 Include mode in `scripts/lib/launcher.mjs` owned-process identity and forward it without a shell. Test concurrent same-mode reuse, mode conflicts, different sources, port collisions, startup cancellation, and ownership-safe stop.
- [x] 5.3 Add `/pi-trace watch <directory>` to `extensions/pi-trace.mjs` and its help. Extend extension-host tests to prove full-remainder path handling, passive registration without capture hooks, visible errors, and worker shutdown with the owning session.
- [x] 5.4 Update README and packaging command examples for watch, shared pause, and external capture. Verify commands from an external working directory and retain explicit offline and realtime descriptions.

## 6. Decode exact usage and refresh coherent client views

- [x] 6.1 Extend `web/src/lib/trace-api.ts` for live workspace metadata, span identity, exact counters, provenance, bounded evidence, and controls. Extend `tests/trace-api.test.mjs` using actual Rust contracts to test lossless large integers, required nulls, invalid variants, budgets, and snapshot mismatches.
- [x] 6.2 Replace import-only polling with a cancellable live refresh controller in `web/src/App.svelte` and supporting state. Test one in-flight poll, unchanged-revision no-ops, timeouts, backoff, visibility resync, pause, stale snapshot recovery, and new-run invalidation with controlled responses.
- [x] 6.3 Refresh overview, pages, diagnostics, and selected detail as one validated snapshot. Test stale-response rejection, span-based selection, expansion and filter continuity, cursor replacement, the 2,000-row retained-view bound, and independent detail lookup for an unloaded selected span.
- [x] 6.4 Reset revealed content and request identity when selected-span evidence changes or a run resets. Test that metadata refresh, selection restoration, and reconnect never trigger an implicit content request or add body data to URL state.

## 7. Present realtime state and tokens in the workbench

- [x] 7.1 Add inclusive token labels to list and waterfall rows and own and inclusive component breakdowns to the inspector. Verify in a real browser that every span kind has a value or explicit state, nested totals are labelled non-additive, large integers remain exact, and context-input meaning remains local.
- [x] 7.2 Add live freshness, catching-up, waiting, paused, stopped, retry, and reconnect presentation with native pause and resume controls. Verify keyboard focus and the shared pause state in two tabs while a synthetic writer appends records.
- [x] 7.3 Drive selected-parent updates, fallback replacement, child arrival, pagination, filters, hidden content, source failure, and run reset through the real service in both themes and at a 375 px viewport. Save screenshots and network evidence showing coherent totals, preserved selection, visible uncertainty, text rendering, and deliberate body reveal.
- [x] 7.4 Record reproducible browser commands, fixture identity, expected values, and evidence paths in a live verification document. Verify its claims against the browser run and preserve MIT attribution for any ported upstream logic.

## 8. Verify integration and the extracted package

- [x] 8.1 Compare settled live output with offline import for synthetic recordings covering duplicate, partial, fallback, child, and timing cases. Normalize only run, snapshot, transport, and occurrence identifiers. Verify equal source-anchored semantics and unchanged source hashes.
- [x] 8.2 Run the declared 30-second, ten-records-per-second acceptance workload with at most 10 MiB and 10,000 spans. Record the machine, flush and browser timestamps, idle and active CPU, and memory. Verify every sampled coherent update appears within two seconds and retained history and derived diagnostics stay bounded across revisions.
- [x] 8.3 Run `npm run check` on the final implementation and record the complete exit result. Verify version synchronization, Node tests, Rust formatting, clippy and tests, Svelte checks and build, and strict validation of all OpenSpec changes.
- [x] 8.4 Inspect `npm pack` output and extract the package outside the source checkout. Start watch mode from another cwd without source or build-tool fallback, append synthetic records, and verify exact span totals, pause, resume, paths with spaces, process reuse conflicts, shutdown, and unchanged source files. Record installed-package evidence separately from browser and source-checkout checks.
- [x] 8.5 Reconcile README, API and compatibility notes, verification documentation, and `openspec/config.yaml` with verified final behavior. Confirm synthetic fixtures and raw-content exclusions in package output, and run `npm run version:check` before any separately authorized commit.

## Workflow follow-up

- Present verified implementation and remaining limitations before archiving this change.
- Archive and synchronize capabilities only through a separately requested OpenSpec workflow.
- For a separately authorized release, use the root package's `npm version` workflow and preserve `THIRD_PARTY_NOTICES.md`. This plan does not authorize a commit, tag, push, or publication.
