# Design

## Context

See `proposal.md` for the motivation. Current implementation has one Rust executable with `/api/v1/health` and static assets, a Svelte 5 health screen, and a package launcher that owns build, start, and stop. There is no importer, analyzer, or trace API. Vite proxies `/api` to port 4317.

`design-concepts/trace-workbench` is an isolated HTML design with synthetic application views. Its compact summary, session disclosure, readable rows, single waterfall, inspector, diagnostics, and themes are the visual reference. Its observations, totals, relationships, source references, and recovery targets are not application data.

The active `bootstrap-trace-explorer` already defines import and analysis semantics and proposes the initial explorer. `pi-package-and-versioning` defines the single npm distribution and process ownership. These constraints remain applicable.

## Goals / Non-Goals

**Goals:** Deliver an existing recording through Rust import and analysis, bounded HTTP views, and a readable Svelte workbench. Keep uncertainty, content access, and snapshot identity explicit. Preserve package and process ownership.

**Non-Goals:** No browser uploads, source replacement within a running process, live capture, watcher, database, export, or hosted integration. No frontend reinterpretation of upstream events. A fixture-backed HTTP response is not the finished feature.

## Decisions

### One startup-selected source, with an immutable completed snapshot

Use the local service design rather than browser uploads or a generated all-data HTML report. Uploads introduce source replacement and bundle validation; a static report moves the whole recording into the browser. Neither is needed for this workbench.

Proposed entry points are:

```text
pi-trace-rs serve <directory>
npm start -- serve <directory>
/pi-trace serve <directory>
```

The native executable also accepts `serve <directory>`. Existing no-argument commands start an empty workspace. Resolve npm paths against the invocation directory and Pi paths against the session working directory before spawning with package-root cwd. The Pi command treats the remainder after `serve` as one path, preserving spaces. Pass argument arrays without a shell.

Repeated start for the same source shares the owned service. A different source requires an explicit stop and new start; it never attaches to or stops another service. Installation and extension discovery remain passive.

### Separate process readiness from import state

Bind the listener and serve the shell before importing on a blocking worker. Publish a graph only when import completes. This refines the bootstrap design's import-before-serving sketch: trace views still require a completed immutable snapshot, while the shell can explain an actual import error.

Use one load-state union, rather than unrelated flags:

```text
Empty
Importing { attemptId }
Ready { attemptId, snapshotId, trace }
Failed { attemptId, structuralError }
```

`trace` is an immutable shared reference. The only retry transition is `Failed -> Importing`. A successful retry publishes a fresh snapshot; no old snapshot is retained or replaced during an active investigation. A guarded transition permits one worker and no queue. Concurrent retry requests yield one accepted request and one conflict.

Health describes listener readiness, not import success. Preserve its existing response and startup marker so the launcher's current collision/readiness contract remains valid. The workbench reads recording state separately and does not use the legacy health `stage` as report status.

### Rust owns one deep import and query boundary

Add `crates/trace-core` as a workspace member. Its public entry point is the already-proposed `Trace::open(source, limits)`. Private modules own bounded source reading, wire decoding, occurrence correlation, measurements, and query indexes. Callers do not sequence parsing, correlation, and aggregation themselves. Add JSON decoding dependencies there; keep HTTP types and routing in `crates/pi-trace`.

The normalized graph contains occurrence IDs, source order, source-relative file/line references, known or unresolved relationships, typed measurements, usage coverage, and diagnostics. Session epochs, interaction occurrences, turn attempts, generations, and tools remain distinct. Explicit child IDs determine reliable child relationships. Unlinked recordings can have a presentation group, but that group is not an invented execution parent.

Use the bootstrap import/analysis scenarios as the semantic contract. In particular, preserve recorded generation timing separately from request-to-headers latency; unknown TTFT/cost and invalid intervals do not become zero. Aggregate generation usage once, use a labeled turn-summary fallback only when generation usage is absent, and preserve partial coverage.

Do not port the prototype's `recovery` field as a heuristic. Final interaction success can coexist with a failed tool. A direct retry target requires supported relationship evidence, not a later successful tool with a similar name. Likewise, default row labels cannot disclose command/path arguments that are otherwise behind content reveal. Use bounded structural metadata and occurrence labels instead.

### Keep import and view limits explicit

Initial limits are policies, not performance claims:

| Boundary | Initial limit |
| --- | --- |
| Total input | 128 MiB |
| One physical record | 1 MiB before JSON decoding |
| Physical records, including malformed/unknown records | 100,000 |
| Recognized source files, including the root | 1,024 |
| Metadata search | 256 Unicode characters |
| Observation/diagnostic page | 100 items by default, 200 maximum |
| Serialized JSON view response | 1 MiB |
| Display label/diagnostic message | 512 UTF-8 bytes |
| Opaque continuation token | 2 KiB |
| Each revealed input/output field | 8 KiB of valid UTF-8 |

Read root `events.jsonl` and supported child recording locations within the configured root. Enforce limits before unbounded reads, allocations, or traversal, reject escaping symlinks, and never modify sources. Detect observable source changes during reading and fail that attempt. This remains an offline snapshot, not an atomic capture of actively written files.

A smaller page is valid when its byte budget is reached, provided it includes continuation. An individual item that cannot fit returns an explicit view-limit error. Preserve source references intact rather than silently shortening file names or losing line numbers. Truncated display labels carry a truncation indicator; record bodies never enter error messages.

### Expose a small snapshot-scoped JSON API

All routes below are proposed additions. Keep `/api/v1/health` compatible.

| Route | Result |
| --- | --- |
| `GET /api/v1/workspace` | Empty/importing/ready/failed state, attempt ID, and current snapshot ID when ready |
| `POST /api/v1/retry` | Retry the failed configured source, with no body or source parameter |
| `GET /api/v1/snapshots/{snapshot}/overview` | Session summary, global timeline domain, measurement availability, and counts |
| `GET /api/v1/snapshots/{snapshot}/observations` | Ordered bounded observation rows, match/context flags, counts, and continuation |
| `GET /api/v1/snapshots/{snapshot}/nodes/{node}` | Metadata, measurements, source references, and diagnostic references without bodies |
| `GET /api/v1/snapshots/{snapshot}/nodes/{node}/content` | Explicit bounded input/output previews and availability/truncation metadata |
| `GET /api/v1/snapshots/{snapshot}/diagnostics` | Bounded structural diagnostics and continuation |

Observation query parameters are `q`, `filter`, `cursor`, and `limit`. Filter values are `error`, `tool`, and `slow`; slow means a valid comparable generation/tool duration greater than 5,000 ms. Rust searches bounded display metadata, never hidden bodies. It constructs the matching set plus ancestor closure in deterministic hierarchy order. Pages partition that ordered set; ancestors precede descendants across pages, and the UI appends pages. Each page reports total matches, total context-inclusive rows, and whether more remain. Cursors bind to the snapshot and query.

All snapshot responses carry `snapshotId`. Unknown snapshots/nodes return JSON `404` errors with distinct codes; invalid queries return `400`. Unready snapshot access and conflicting retry return `409`; a retry without a source returns a specific `409` code. Accepted retry returns `202` with its attempt ID. Unknown `/api` routes return JSON errors instead of the static HTML fallback. Errors have a bounded code, structural message, and optional source reference.

Keep recorded/derived/unknown/invalid timing and complete/partial/fallback/unknown usage explicit in response types. Rust supplies offsets and scale; Svelte only maps supplied valid values to pixels. Unknown intervals remain rows without fabricated bars. No response promises stable IDs across process restarts.

Define serialized DTOs in Rust, typed runtime decoders in `web/src/lib/trace-api.ts`, and contract fixtures produced through actual Rust serialization. This follows the existing `health.ts` boundary pattern without adding a schema/code-generation platform. Contract tests cover enums, optional/null fields, IDs, errors, and byte limits.

### Apply local request and content boundaries before serving data

Bind to `127.0.0.1`. Accept Host values `127.0.0.1:4317` and `localhost:4317`; accepted browser origins are their HTTP origins and `http://127.0.0.1:5173` for Vite development. Configure the Vite API proxy to rewrite Host to the Rust target. No permissive CORS is added.

Read requests can omit Origin for local clients and ordinary same-origin reads. Reject every present unrecognized Origin, including `null`. Retry requires a present allowed Origin; command-line HTTP callers can set that documented header explicitly. Retry accepts no payload or source-changing parameters. Validate these boundaries before data access or starting work.

Trace responses use `Cache-Control: no-store`. Default metadata and structural diagnostics omit raw bodies, command snippets derived from arguments, and decoder errors that echo input. Metadata can still be sensitive; no blanket sanitization claim is made. The content route returns bounded text only after explicit reveal. Do not prefetch, persist, or log those previews.

### Give Svelte presentation state, not trace semantics

`App.svelte` owns workspace loading and its discriminated load state. A workbench owner keeps selected ID, search, filter, collapsed branches, execution view, inspector tab, and theme. Components own meaningful regions: session summary/disclosure, toolbar, shared execution rows with waterfall/list cells, inspector, diagnostics, and load states. Avoid a component for every label or a parallel hierarchy/timeline selection store.

URL state includes snapshot, selection, structural filter, view, tab, and theme. Search text stays in memory so copied links cannot carry sensitive query fragments. Revealed previews stay in memory and reset on snapshot change or reload. Restoring a stale link shows a notice and an explicit route to the current workspace.

Abort superseded requests and ignore responses from obsolete query, selection, attempt, or snapshot identities. Poll workspace state only while importing; a failed import stops polling and offers Retry import. An unreachable service offers Reconnect through read requests. Query changes discard old query pages, while independently loaded selected metadata remains available with an outside-results notice. Filters retain matching ancestor context even when those branches were previously collapsed.

Use Svelte text bindings for every trace-derived value, never `{@html}`. Use separate native disclosure and selection buttons, avoiding nested buttons. Keyboard navigation, loading announcements, focus after selection, accessible continuation, diagnostics, and a narrow stacked inspector are part of the report.

### Transfer the readable visual system, not prototype implementation details

Retain the Dose green-grey palette, system fonts, light/dark tokens, and centered capped work surface. Start with 14 px body text, 13 px execution labels, and at least 12 px inspector body/code text with comfortable line spacing. Secondary accents stay quiet; selection and failures use text/icons as well as color.

Keep the compact summary, session metadata disclosure, contextual structural labels, one duration column, and one explicit I/O reveal action. At 1440 by 900, the fourteen-observation reference fixture fits with the inspector. At narrow sizes the inspector stacks and the page has no horizontal overflow. Do not copy the prototype's preview-state selector, promotional footer, fixed counts, or demo-only recovery relations into product flows.

Before component transfer, recheck the prototype's Hallmark bookkeeping and contrast/disclosure behavior, then capture reference states. The prototype's 2026-10-29 render/kept sweep reports zero Floor failures, Reflex findings, and warnings after the status selector, disclosure colors, and preservation records were corrected. Spacing notes remain nonblocking for this scoped polish. Run Hallmark in app scope again when transferring the components and verify both default and revealed report states.

### One implementation owner and one acceptance map

This change owns the integrated implementation checklist. The older bootstrap requirements remain the semantic baseline; they are not competing deltas or a second implementation path. When applying this approved change, annotate the overlapping bootstrap design/tasks to point to this execution plan, without checking work complete in advance.

| Bootstrap work | Acceptance owned here |
| --- | --- |
| 2.1–2.5, import and source diagnostics | Core import tasks and semantic fixture checks |
| 3.1–3.5, graph/timing/usage/outcomes | Core analysis tasks and serialized view checks |
| 4.1–4.2, source selection and local API | Service, retry, request-boundary, and launcher tasks |
| 4.3–4.7, investigation UI | Svelte workbench and browser scenarios |
| 5.1–5.3, integrated acceptance | Import-to-browser run, declared-workload observations, and scenario audit |

Packaging policy stays with `pi-package-and-versioning`; necessary source-argument and launch adaptations are implemented here. Root `package.json.version` remains canonical. New `trace-core` sources are covered by the existing `crates/` package inclusion, which must still be inspected in the real archive.

### Synthesis and rejected additions

Two independent design memos compared startup selection, browser uploads, and static export. Both chose the startup-selected Rust snapshot. Candidate A is the base for the bounded contract, semantic boundaries, and implementation ownership. Candidate B contributes the constrained failed-import retry and memory-only search query. An independent cross-judge preferred the same base and those two refinements.

The synthesis rejects general reload/source replacement, schema-generation infrastructure, and a fixture-only delivery. It keeps existing health readiness compatible and defines bounded continuation for long ancestor chains. Separate configured Codex models produced the candidates and review; the parent-family comparison was unavailable.

## Risks / Trade-offs

- Import and analysis are substantial new work. A rendered Svelte shell or mock API cannot close acceptance; verify a real JSONL fixture through Rust and the browser.
- Starting the shell before import changes the startup sketch. Keep health and recording state separate, and never publish partial success.
- Paginated hierarchy can obscure context. Use deterministic appendable traversal, explicit match/context counts, and snapshot-bound continuation.
- Trace-derived metadata is also sensitive. Bound it, omit hidden bodies, retain local request checks, and avoid external assets or logs containing records.
- Original prototype labels sometimes depend on fabricated metadata. Prefer truthful generic occurrence labels over exposing argument bodies for visual similarity.
- File reads cannot establish an atomic snapshot of a running collector. Use offline recordings and reject observable changes without claiming live support.

## Migration Plan

Apply in verified slices: finalize visual references and contracts; implement core import/analysis; add service views and startup selection; build the Svelte workbench; run integrated and package acceptance. Preserve the existing health route and no-argument launcher behavior throughout.

Test with fabricated recordings only. Include recovered failures, repeated epochs/attempts, reversed parallel children, ambiguous links, malformed middle/tail records, invalid timing, partial usage, unavailable fields, markup, and path/size limits. Test real failed-import repair/retry and simultaneous retries. Browser acceptance uses the actual Rust importer and checks both themes, keyboard use, filtered selection, pagination, stale links, and service loss.

Run `npm run check`. Before distribution, inspect `npm pack`, extract outside the checkout, and run the same recording through the bundled binary and assets without source/build tools. Keep bounded-workload measurements separate from correctness and packaging proof. Publication remains a separate maintainer action.

Rollback restores the previous application and launcher together. No source recording or persistent database is migrated or modified. Mark either change's tasks complete only after their mapped scenarios pass; do not archive unfinished bootstrap work.
