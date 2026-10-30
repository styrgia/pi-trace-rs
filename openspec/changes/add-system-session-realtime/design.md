# Design

## Context

See [proposal.md](proposal.md) for motivation. Everything below is proposed unless identified as observed.

Observed code has a single-source offline service. `crates/pi-trace/src/service.rs` holds one `LoadState` and calls `Trace::open`. The native CLI and Node launcher accept a collector directory, and `web/src/App.svelte` stops polling after import. The collector reducer interprets `events.jsonl` records directly. Its `SourceRef` assumes one root and a physical file line.

The active `add-realtime-span-tokens` change has implemented shared decoding, repeatable projection, exact accounting, and core `FollowReader` APIs. Public live lifecycle and UI work remain pending. Its collector-only and append-only contracts do not cover native session trees, file rewrites, compressed archives, or transactional database updates.

Read-only research inspected installed program versions and upstream implementation, without opening private transcripts. Compatibility baselines are Codex 0.166.0 at `6ff1e1ca3b30fc8b914720681fcf5fe62e95f856`, Pi Agent 1.4.0 at `4a080252f06caf63a525d7873c5d94d3c23887e0`, and OpenCode 2.1.4 at `68bc26b7da1990ce10ffecfbc1e09eb13aa5d824`. Claude Code is closed source. Its installed version is 2.1.216; the official transcript-reader reference is Claude Agent SDK Python at `1ea8cb74ddcdadbfaa9b20dfa8268e18047851e7`. SDK capabilities newer than that executable are optional detected evidence.

## Goals / Non-Goals

The design separates discovery of the whole supported catalog from analysis of a bounded set of sessions. It reuses Rust projections and application views where their semantics match. Native adapters retain provider evidence instead of converting records into fabricated collector JSONL.

The first compatibility matrix covers the baselines above, Codex legacy and paginated rollout representations, Pi version-3 session trees with explicitly tested older headers, detected Claude transcript variants, and OpenCode version-2 SQLite. Older OpenCode JSON and version-1 SQLite are recognized as unsupported profiles in this change. No migration runs. Supporting every historical provider release is a separate compatibility expansion.

System mode follows persisted evidence. It does not capture unsaved streams, scan arbitrary prior custom homes, discover memory-only databases, or inspect agent process registries. It does not add provider APIs or process controls. There is no cross-session grand usage total because shared and inherited evidence would require a separate attribution contract.

## Decisions

### 1. Select the mode and effective roots at startup

Add `pi-trace-rs system`, native `pi-trace system`, and `/pi-trace system`. Keep existing invocation forms. For CLI and native invocations, repeat `--root <provider>=<path>` to replace that provider's default store set, and repeat `--disable-provider <provider>` to omit a provider. Provider names are `codex`, `pi`, `claude`, and `opencode`. Reject duplicate conflicts and unknown providers. Resolve relative roots from the caller's cwd before entering the package directory.

The Pi extension's first version accepts `system` without path arguments and inherits supported environment settings. Users needing explicit roots can launch the installed CLI. This preserves its existing full-remainder path convention for collector commands without introducing shell parsing.

| Provider | Default and inherited locations | Discovery contract |
| --- | --- | --- |
| Codex | `${CODEX_HOME:-~/.codex}` | `sessions/YYYY/MM/DD` and `archived_sessions`, including `.jsonl` and `.jsonl.zst`. A plain sibling wins over its compressed representation. |
| Pi Agent | `${PI_CODING_AGENT_DIR:-~/.pi/agent}/sessions` | Encoded project directories. `PI_CODING_AGENT_SESSION_DIR` selects a directly scanned custom session directory. An explicit root has highest precedence and supports either documented layout by header discovery. |
| Claude Code | `${CLAUDE_CONFIG_DIR:-~/.claude}/projects` | Project transcripts and recursive `<session-id>/subagents/**/agent-*.jsonl`. Read optional relationship sidecars only from the documented locations. |
| OpenCode | `${XDG_DATA_HOME:-~/.local/share}/opencode` | Supported `opencode*.db` stores with deduplicated canonical paths. Honor `OPENCODE_DB` as a relative name under this directory or an absolute selected database. Do not open `:memory:`. |

For OpenCode, a configured database path takes precedence over default filename discovery. Channel filenames are discovered within the bounded data directory rather than guessed from a running executable. An explicit OpenCode root can name a database or a data directory. An absolute environment-selected database permits only that file and its SQLite sidecars, not recursive traversal of its parent.

Pi's `--session-dir` belongs to an individual Pi invocation and cannot be recovered from an unrelated process. Users select that extra store explicitly. Never claim complete coverage outside effective configured roots. Show logical root identity, availability, format coverage, and partial indexing status. Paths and project metadata can themselves be sensitive.

Launcher process identity includes mode and sorted canonical provider-store configuration. Config changes require a new owned service start. No browser route accepts a path. Default no-source startup remains the empty offline workspace, avoiding an unexpected scan of a user's history.

### 2. Separate catalog state from session analysis

Introduce a Rust catalog service above `trace-core` rather than turning one trace into a global execution tree. Proposed modules are `crates/pi-trace/src/catalog.rs` for scheduling and routes and `crates/trace-core/src/native/` for storage profiles and pure interpretation. Keep provider-specific source access out of Svelte and Node.

The key data shapes are `Provider`, `StoreId`, `CatalogSessionId`, `SessionDescriptor`, `CatalogRevision`, `SessionRevision`, and `SessionAnalysis`. A descriptor contains native identity, project metadata, source revision, known relationships, format capabilities, ingestion status, and freshness. It contains no prompt-derived title or transcript preview.

Use an opaque identity derived from provider, canonical store namespace, and the provider's recording identity. Codex thread identity differs from recording identity after a revert, so preserve thread group, thread ID, and distinct rollout ID. Two rollouts of one thread are separate catalog entries with an explicit thread relationship. Archive moves and zstd/plain transitions of one rollout preserve catalog identity. Claude subagent and Pi fork identities remain distinct from their parent.

Project identity comes from supported metadata, not reverse decoding a lossy directory name. Conflicting native IDs without an authoritative representation rule remain separate diagnosed candidates. Never resolve them by modification time.

Catalog revision changes only for observable descriptor or coverage changes. Native source revision marks a session dirty even when analysis is not loaded. Background discovery tracks all catalog entries; full semantic parsing is lazy. Selecting a session loads its latest bounded evidence and then follows that session. Catalog usage is either a qualified last-analyzed summary with its source revision or unavailable, never an unlabelled stale total.

This design avoids holding every transcript in memory. Eagerly parsing all history would apply the existing 128 MiB limit to unrelated sessions or multiply private retained bodies by catalog size.

### 3. Normalize typed native evidence at the Rust boundary

Define pure adapter output around logical message, model execution, tool call and result, relationship, lifecycle, usage contribution, and diagnostic evidence. Retain provider-native IDs and evidence availability. Extract a shared projection boundary from the collector-specific `Builder::event` path where required. Keep collector interpretation intact and adapt both producers into common semantic structures.

A session container can provide presentation grouping with unknown timing. Omit unavailable turn and interaction levels rather than fabricating execution. Context parent, fork ancestry, root thread group, and invoking-tool parent are different relationship kinds. Only unique explicit invocation evidence contributes to execution ancestry and inclusive usage. Timing keeps recorded, derived, unknown, and invalid states.

Native provenance is a tagged union. `jsonl` includes store, relative source, physical line, and optional byte range. `compressedJsonl` includes original source, encoding, decoded line, and decoded byte range. `sqlite` includes database identity, schema profile, table, primary key, column or JSON pointer, and observed transaction revision. Existing collector routes retain their current `{file,line}` contract. New session-qualified routes expose typed native provenance, with no dummy `line: 0` or fake temporary-file path.

#### Codex profile

Read session metadata and supported legacy response items or paginated completed items. Correlate function and custom tool calls by their recorded `call_id`. Keep event messages and response-item representations from creating duplicate observations. Prefer authoritative `token_usage_record.usage` keyed by response and context identity. Its cumulative turn and thread fields are corroborating scopes, not extra contributions.

For legacy `token_count`, retain reported totals with their cumulative scope. Difference only comparable monotonic counters in one explicit context, with derived provenance. Without an unambiguous response boundary, expose session-level evidence and unavailable per-generation attribution. Context-window synthetic totals do not prove billed model usage. Diagnose resets and incompatible totals.

Input includes cached input. Derive non-cached input through checked subtraction when both operands exist. Reasoning is a subset of output. Persist timestamps as their actual meaning, including append time. Do not call every outer timestamp an execution boundary.

#### Pi Agent profile

Decode session headers and entry trees directly, without `SessionManager.open`, which can migrate and rewrite sources. Retain branch and current-context information separately from all recorded historical work. `parentSession` alone is fork provenance, not an invoking tool.

Usage can come from assistant messages, tool-result usage, standalone usage entries, compaction, and branch summaries. Preserve entry identities. Tool-result nested calls can describe usage already aggregated in the result, so show that breakdown without summing it twice. Reasoning is included in output and `cacheWrite1h` in cache-write.

Assistant message timestamps denote request starts and optional `durationMs` is a measured monotonic duration. Tool-result timestamps denote result creation. Outer entry timestamps denote persistence. Apply each meaning only to its documented variant. Pi saves completed assistant messages, so unsaved streaming output remains unavailable.

#### Claude Code profile

Decode transcript UUIDs, API message IDs, parent references, user and assistant messages, and tool blocks. Correlate parallel tools with `tool_use.id` and `tool_result.tool_use_id`. Deduplicate assistant usage by API message identity, including streamed or repeated transcript revisions. Do not assume one `parentUuid` chain includes all parallel activity.

Treat optional subagent sidecars and newer reader fields as detected capabilities. A parent session without a unique tool relationship remains a session relationship. `logicalParentUuid` around compaction is not an execution edge. Keep per-message usage, result totals, model-tree totals, and cumulative costs as distinct scopes. Cache creation subdivisions never add to their own aggregate again.

#### OpenCode profile

Detect the version-2 schema signature before querying `session_v2`, `session_message`, and durable `event` records. Message IDs and sequence establish logical order; update timestamps alone are not a lossless cursor. Tools are typed elements in message data with native identities and state. Preserve fork boundaries and inherited prefixes.

Assistant model usage and durable `session.usage.recorded` events cover their documented contribution kinds. Compaction messages repeat the durable usage event and do not contribute twice. Session aggregate counters are reconciliation evidence, not additional usage. Native token components are disjoint non-cached input, visible output, reasoning, cache read, and cache write. Normalize output as checked visible-output plus reasoning, with derived provenance and the native breakdown retained.

#### Shared accounting

Use canonical logical contributions with explicit authority, replacement, inheritance, and scope. Reuse checked exact counters and bounded evidence from the active accounting change. Add normalization provenance for native response, auxiliary usage, and derived components rather than mislabelling all native contributions as collector generation usage. Preserve native reasoning breakdowns and subset relationships without adding a new amount to an already inclusive output counter.

Copied fork context stays inspectable but does not become newly incurred work. If native metadata cannot identify the copied boundary, mark attribution partial. Keep selected-context size separate from billed historical usage. Never sum parent inclusive rows or session aggregates to calculate a total.

### 4. Follow each storage contract and isolate recovery

For native append-only JSONL prefixes, reuse bounded line buffering and provisional projection. Charge decoded bytes, physical records, pending tails, and malformed input once. Distinguish temporary EOF from closed historical data. For Codex compressed archives, stream zstd through the same decoded-input limits, plus a compressed-input bound. Corrupt frames produce original-source diagnostics. A completed archive permits its final unterminated record under offline EOF semantics; an active plain tail waits for newline.

Track source identity and native logical identity separately. Recognized archive or encoding changes rebind the source after validation. A Pi migration or other supported rewrite retains catalog identity but starts a new analysis run after a coherent bounded reread. Unknown replacement, truncation, or disappearance marks only that session stale or unavailable. Retain the previous valid snapshot during recovery. Do not concatenate rewritten evidence with old checkpoints.

Use bundled SQLite through `rusqlite` in read-only mode, with short read transactions and a bounded busy timeout. Do not invoke OpenCode's database initialization, migrations, chmod, checkpoint, or write pragmas. Validate required WAL sidecars before opening. If a consistent read requires creating or modifying source files, report unavailable rather than repair the store. Never set `immutable=1` on a changing database.

Keep one read connection per discovered database within the configured connection cap. Compare `PRAGMA data_version` on that same connection and revalidate database identity and schema after replacement. A changed store triggers a bounded catalog query and a consistent selected-session reconciliation, including upserts, deletions, same-timestamp changes, and durable-event changes. Commit a session revision only after all its relevant rows are read from one transaction. Do not maintain a last-`time_updated` cursor that can miss rows.

SQLite statements use an execution budget and cancellation. Release read transactions promptly so Pi Trace does not retain a growing WAL. A locked store backs off independently. No persistent private index or trace copy is added.

### 5. Bound discovery and selected-session work separately

Proposed initial defaults are 16 configured stores, 10,000 retained catalog descriptors, 100,000 visited entries per discovery pass, 100 sessions per catalog page, and 512 visited entries per scheduling batch. A store exceeding a bound reports partial coverage and unknown omitted count when exhaustive counting would breach that same bound.

At catalog capacity, retain currently investigated descriptors first, then prefer most recently observed source activity. Break equal activity times by deterministic session identity. A newly observed session can replace the oldest unselected cold descriptor. This is catalog omission under a visible retention limit, not source deletion or analysis eviction. Rediscovery reconstructs the same identity. Catalog cursors remain revision-qualified as admission changes list membership. Discovery continues from a rotating per-store traversal boundary on the next pass instead of repeatedly stopping at the same first 100,000 entries.

Scan roots and changed known sources every second while caught up. Reconcile directory listings in rotating batches, including historical locations. Continue an initial sweep without sleeping between batches, but alternate it with selected-session work. Recheck absent roots every five seconds. Prefer bounded polling for the first implementation; platform file watchers can later accelerate invalidation without becoming the only discovery mechanism.

Retain at most four analyzed sessions. A request within the previous two seconds marks an entry active. At capacity, evict the least-recently-used inactive entry without waiting for its idle expiry. If all four are active, return structural `analysis_capacity` with a bounded retry hint and create no fifth worker. Independently evict unused entries after 60 seconds. Switching sessions stops the old browser demand immediately, allowing its entry to become inactive. Catalog rows outlive analysis eviction. Follow recently requested analyses every 250 ms, one worker per loaded session and at most two concurrent parse or projection jobs. Each worker batch has the existing 1 MiB or 2,000-record bound. Database connections are capped at 16, with short transactions and capped exponential retries up to five seconds.

Per-session default limits remain 128 MiB of decoded evidence, 1 MiB per record or decoded row, 100,000 records, and 1,024 related sources. Add a 256 MiB compressed-input limit per session. Retained decoded evidence across analyzed sessions is capped at 256 MiB. These are input and retention bounds, not a guaranteed process-RSS limit. A limit yields a named session or catalog state, never silent omission. Repeated scans do not replenish an analysis run's byte budget; rewrites create a new bounded run and release the old analysis after bounded snapshot retirement.

Retain current and previous snapshots for each loaded session and two catalog revisions. Snapshot retirement never substitutes new evidence for an old request. Keep existing eight concurrent view-handler and 1 MiB response limits. Each eviction invalidates its old run explicitly. No catalog activity creates an analysis worker for every historical session.

### 6. Add session-qualified application routes and browser state

Add these routes in system mode, with opaque identifiers and bounded documented query fields:

- `GET /api/v1/system` returns mode, catalog revision, provider-store status, progress, limits, and coverage.
- `GET /api/v1/sessions` returns catalog pages filtered by provider and project with revision-qualified cursors.
- `GET /api/v1/sessions/{sessionId}/workspace` requests or reuses bounded analysis and returns source, run, revision, snapshot, capability, and freshness state.
- `GET /api/v1/sessions/{sessionId}/snapshots/{snapshotId}/...` provides overview, observations, span detail, diagnostics, and deliberate content views with typed native provenance.
- `POST /api/v1/sessions/{sessionId}/retry` retries only that known session's configured source and requires an approved Origin.

Existing offline routes keep their contracts. Collector-watch controls are mode-specific and return a mode conflict in system mode. System mode initially refreshes continuously without adding pause semantics or indefinite snapshot pinning. All routes retain Host, Origin, loopback, no-store, structural error, and explicit content boundaries.

In Svelte, add a catalog region alongside the existing workbench. Poll lightweight system status every second and selected-session workspace every 500 ms, with one request in flight per channel, cancellation, visibility resynchronization, and bounded backoff. Fetch catalog pages only when their revision or filters change. Keep catalog metadata separate from full trace DTOs.

Key browser work by session, run, snapshot, span, and request generation. Preserve stable selection within a run and rebuild rows coherently for a new snapshot. Old selection can survive catalog reordering or page exclusion through session lookup. A removed session shows unavailable; a new run explains invalidation. Old-revision errors cause explicit resynchronization. Keep the existing 2,000 retained-row bound and source-safe URL state.

Refresh never reveals replacement prompt or result bodies. Native labels use provider and structural identifiers, not the first prompt. Render all provider content as text, use native semantic controls and visible focus, and verify both themes and a 375 px viewport. Display storage activity independently of confirmed native lifecycle evidence and explicitly show that data follows persisted records.

### 7. Deliver verifiable units and keep evidence scopes separate

The Sequence Work into Verifiable Units principle sets delivery order. First freeze native fixture and provenance contracts, then add adapters individually, then catalog scheduling, routes, launcher behavior, and browser integration. Each adapter must pass its own synthetic semantic and live-update checks before joining the catalog. Shared prerequisites from `add-realtime-span-tokens` must be verified in their owning change before dependent integration is marked complete.

Use synthetic stores only. Test root aliases, unsupported schemas, malformed input, split UTF-8, archive moves, compression, rewrites, retention deletion, fork inheritance, compaction, parallel tools, partial usage, and exact counters. SQLite coverage includes WAL-only commits, upserts, deletes, same-millisecond updates, schema replacement, contention, and inaccessible sidecars. Compare settled live and reopened native projections by logical evidence identity, without treating transport IDs as semantics.

The proposed latency workload has 1,000 catalog sessions across four providers, four changing sessions, at most 10 MiB and 10,000 observations per changing session, and ten committed records per second in total. Run for 30 seconds on a recorded local machine. Measure persisted commit to catalog appearance within five seconds and commit to coherent selected rows and inspector within two seconds for every sampled update. Test initial discovery separately and disclose whether its sweep is complete. These are proposed budgets, not measured performance.

Record idle and active CPU, retained memory, parse counts, and catalog progress during a historical backlog. Verify that unselected history is not fully reparsed on every tick and that idle polls create no semantic revisions. Record source hashes in static phases and expected writer changes during live phases. Separate core, HTTP, browser, workload, and extracted-package evidence.

## Risks / Trade-offs

- Native formats change without a stable interchange contract. Pin profiles and source references, test future variants, and report unsupported evidence explicitly.
- Disk persistence can lag execution and omit unsaved streams. Show persisted freshness and provider capability rather than inferred process status.
- Four analyzed sessions can still retain substantial model and projection state. Enforce global decoded retention and measure actual memory during acceptance.
- Catalog bounds can exclude some history. Keep coverage and the exact limiting category visible, and allow restart with explicit configuration.
- SQLite readers can affect WAL retention or fail without usable sidecars. Use short cancellable transactions and preserve the strict read-only failure path.
- New native provenance differs from collector file-line DTOs. Keep existing routes stable and validate both contracts from Rust serialization.
- Completion of the existing live change can alter shared code while this proposal is pending. Reconcile the verified prerequisites before apply without treating its whole task list as completed.

## Migration Plan

1. Verify shared reducer, accounting, snapshot, and refresh prerequisites from the active live change. Keep unrelated WIP intact.
2. Add pure native profiles, provenance DTOs, synthetic fixtures, bundled zstd and SQLite, and adapter-specific settled/live parity checks.
3. Add bounded catalog scheduling and session-qualified service routes. Preserve collector routes and source selection boundaries.
4. Add explicit system startup and the catalog UI. Verify cross-provider switching, coherent refresh, failures, and body boundaries through the real service and browser.
5. Run `npm run check`, update compatibility and API documentation to verified behavior, and inspect `npm pack`. Extract outside the checkout and verify system startup and updates for all four providers without a build fallback.

Rollback uses the previous package or existing collector modes. Sources are read-only and no private persistent index needs migration. Version bumps, commits, archiving, and publication remain separate workflows.

## Source references

- Codex [home resolution](https://github.com/openai/codex/blob/6ff1e1ca3b30fc8b914720681fcf5fe62e95f856/codex-rs/utils/home-dir/src/lib.rs), [compressed rollouts](https://github.com/openai/codex/blob/6ff1e1ca3b30fc8b914720681fcf5fe62e95f856/codex-rs/rollout/src/compression.rs), [recording identity](https://github.com/openai/codex/blob/6ff1e1ca3b30fc8b914720681fcf5fe62e95f856/codex-rs/rollout/src/rollout_file_name.rs), and [usage protocol](https://github.com/openai/codex/blob/6ff1e1ca3b30fc8b914720681fcf5fe62e95f856/codex-rs/protocol/src/protocol.rs).
- Pi [session manager](https://github.com/earendil-works/pi/blob/4a080252f06caf63a525d7873c5d94d3c23887e0/packages/coding-agent/src/core/session-manager.ts), [session persistence and stats](https://github.com/earendil-works/pi/blob/4a080252f06caf63a525d7873c5d94d3c23887e0/packages/coding-agent/src/core/agent-session.ts), and [native message types](https://github.com/earendil-works/pi/blob/4a080252f06caf63a525d7873c5d94d3c23887e0/packages/ai/src/types.ts).
- Claude [official transcript reader](https://github.com/anthropics/claude-agent-sdk-python/blob/1ea8cb74ddcdadbfaa9b20dfa8268e18047851e7/src/claude_agent_sdk/_internal/sessions.py), [usage scopes](https://code.claude.com/docs/en/agent-sdk/cost-tracking), and [application data and retention](https://code.claude.com/docs/en/claude-directory).
- OpenCode [database roots](https://github.com/anomalyco/opencode/blob/68bc26b7da1990ce10ffecfbc1e09eb13aa5d824/packages/util/src/global-roots.ts), [database naming](https://github.com/anomalyco/opencode/blob/68bc26b7da1990ce10ffecfbc1e09eb13aa5d824/packages/cli/src/database-path.ts), [current session SQL](https://github.com/anomalyco/opencode/blob/68bc26b7da1990ce10ffecfbc1e09eb13aa5d824/packages/core/src/session/sql.ts), [token units](https://github.com/anomalyco/opencode/blob/68bc26b7da1990ce10ffecfbc1e09eb13aa5d824/packages/schema/src/token-usage.ts), and [durable usage events](https://github.com/anomalyco/opencode/blob/68bc26b7da1990ce10ffecfbc1e09eb13aa5d824/packages/schema/src/session-event.ts).
- SQLite [read-only WAL behavior](https://www.sqlite.org/wal.html#read_only_databases), [connection-local data-version comparison](https://www.sqlite.org/pragma.html#pragma_data_version), and [URI options](https://www.sqlite.org/uri.html).
