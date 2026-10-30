# Tasks

Checkboxes record verified implementation. Follow [design.md](design.md) and all three capability specifications. The evidence and compatibility limits are recorded in [system-session verification](../../../docs/system-session-verification.md); planning validation alone is not implementation evidence.

## 1. Establish shared evidence and compatibility contracts

- [x] 1.1 Verify the shared reducer, exact accounting, stable span IDs, snapshot lifecycle, and refresh prerequisites against the current `add-realtime-span-tokens` implementation. Record verified versus missing prerequisites and complete missing shared behavior in its owning change before marking dependent integration complete. Verify existing collector semantics and tests remain unchanged.
- [x] 1.2 Add provider, store, catalog session, source revision, capability, and native provenance types in Rust, with a shared projection boundary independent of collector event names. Add serialization contracts and tests for file-line, compressed decoded-line, and SQLite locators, provider-ID collisions, unknown timing, and unchanged existing collector DTOs.
- [x] 1.3 Extend canonical contributions for native authority, replacement, cumulative scopes, inherited work, auxiliary usage, native reasoning breakdowns, and derived normalization provenance. Test cache-subset subtraction, disjoint reasoning addition, nested usage, repeated evidence, zero, partial counters, overflow, and integers above JavaScript's safe range.
- [x] 1.4 Document the pinned provider profiles, supported header or schema variants, provenance shapes, and native availability limits in `docs/upstream-compatibility.md` and `docs/trace-api.md`. Add fabricated fixture inventories with upstream source references and verify every documented profile has synthetic contract coverage. Preserve MIT notices for any ported logic.

## 2. Implement the Codex native adapter

- [x] 2.1 Discover default and configured Codex homes, active and archived rollout trees, logical thread versus recording identities, and plain versus zstd representations. Test aliases, archive moves, resumed compressed history, distinct revert rollout IDs, and preservation of catalog identity across equivalent representations.
- [x] 2.2 Decode supported legacy and paginated native records, explicit session and fork metadata, tools by call ID, timestamps, and response-level usage authority. Test duplicate event representations, `token_usage_record`, cumulative-only reports, resets, inherited prefixes, and unavailable per-call attribution without invented duration or parent links.
- [x] 2.3 Add bounded plain-prefix following and streaming zstd archive reading with decoded and compressed limits. Test split UTF-8, missing newlines, corrupt frames, malformed middle records, decompression expansion, replacement, and settled live versus reopened parity with original locators.
- [x] 2.4 Add Codex root, schema, history, usage, compression, and persistence-limit documentation. Verify examples against the synthetic Codex store and fresh Rust view serialization without reading real rollouts.

## 3. Implement the Pi Agent native adapter

- [x] 3.1 Discover encoded-project stores and explicit session directories using documented precedence. Purely decode version-3 session trees and the explicitly enumerated supported older headers without calling migration APIs. Test custom roots, branches, forks, duplicate IDs, and project identity without source writes.
- [x] 3.2 Normalize assistant messages, tool results, standalone usage, compaction and branch-summary usage, and nested-call evidence. Test all contribution kinds, reasoning and cache subdivisions, historical usage versus context branch, recorded monotonic durations, persistence timestamps, and unknown in-flight progress.
- [x] 3.3 Follow append prefixes and detect supported rewrites with fresh analysis runs and last-valid recovery. Test source hashes, pending tails, migration-shaped replacement, branch changes, late results, no-op scans, and settled live versus reopened parity.
- [x] 3.4 Document Pi session layouts, supported headers, pure-reader behavior, timing units, and persistence limits. Verify the documented cases through the Pi native fixture matrix and keep collector-format support distinct.

## 4. Implement the Claude Code native adapter

- [x] 4.1 Discover configured project transcripts and recursive subagent transcripts with optional relationship sidecars. Test lossy project-directory names, absent roots, sidecar absence, explicit parent relationships, ambiguous tool linkage, retention deletion, and bounded traversal without consulting the live process registry.
- [x] 4.2 Decode transcript UUIDs, API message identity, parallel tool calls and results, compaction boundaries, native usage, and recorded lifecycle evidence. Test repeated assistant revisions, shared message IDs, parallel siblings, cache subdivisions, distinct summary scopes, and missing request starts without duplicated usage or guessed links.
- [x] 4.3 Add prefix following and isolated recovery for malformed, replaced, or removed transcripts. Test split writes, late subagent discovery, no-op revisions, source diagnostics, stable logical identities, and settled live versus reopened parity.
- [x] 4.4 Document the detected transcript profiles and distinguish installed CLI evidence from optional newer SDK capabilities. Verify every supported field and relationship through fabricated transcripts and serialized contracts.

## 5. Implement the OpenCode version-2 SQLite adapter

- [x] 5.1 Add bundled SQLite support and resolve XDG roots, database filenames, channel stores, explicit files, and `OPENCODE_DB`. Detect the `session_v2`, `session_message`, and durable-event schema signatures. Test current schemas, recognized unsupported legacy stores, aliases, absolute overrides, `:memory:`, and no provider executable dependency.
- [x] 5.2 Open source databases independently in strict read-only mode with short transactions, statement cancellation, a bounded busy timeout, and sidecar validation. Test WAL-only commits, busy reads, inaccessible sidecars, no migrations or checkpointing, and unchanged database, schema, permissions, and sidecar bytes in static phases.
- [x] 5.3 Normalize message and embedded tool state, lifecycle, fork boundaries, native token components, and canonical assistant or durable usage-event contributions. Test disjoint reasoning, compaction-event duplication, session aggregates, inherited history, row provenance, checked arithmetic, and partial evidence.
- [x] 5.4 Detect changes through same-connection data-version checks and identity revalidation, then reconcile relevant rows transactionally. Test updates, deletes, equal timestamps, reordered rows, concurrent commits, replacement and schema change, cancellation, and settled live versus reopened parity without a timestamp-only cursor.
- [x] 5.5 Document the supported version-2 profile, unsupported legacy profiles, WAL requirements, typed locators, and usage rules. Verify documentation against fabricated databases and serialization; add required bundled-dependency notices without importing provider bootstrap code.

## 6. Build bounded system discovery and catalog scheduling

- [x] 6.1 Add startup root resolution and canonical store namespaces with default, inherited, explicit-root, and disabled-provider behavior. Test relative paths with spaces, multiple stores, aliases, conflicts, symlink escapes, unavailable roots, and configuration disclosure without reading credentials.
- [x] 6.2 Add catalog descriptors, revisions, stable ordering, provider and project filters, and revision-qualified pagination. Test native ID collisions, Codex recording variants, archive moves, session removal, malformed headers, unsupported profiles, expired cursors, and exact-limit plus one-over coverage reporting. Verify recent-session admission at capacity, retained investigated descriptors, cold-history omission, and deterministic identity after rediscovery.
- [x] 6.3 Implement fair bounded discovery and rechecks for new, changed, absent, and historical sources. Test a large initial backlog with four writers, isolated root failure, provider recovery, progress across batches, rotating traversal beyond the per-pass limit, and idle scans without catalog revisions.
- [x] 6.4 Add lazy session analysis, source-revision invalidation, four-session retention, two-second active demand, capacity admission, 60-second idle eviction, parse concurrency, global decoded retention, and per-session limits. Test a fifth request with all entries active, immediate inactive-LRU replacement, session switching, eviction and reload, last-valid failure states, cumulative budgets, and absence of full-history reparsing on every tick.
- [x] 6.5 Document catalog status, activity versus process state, source coverage, retention, scheduling limits, and qualified last-analyzed summaries. Verify each state through deterministic catalog tests and generated contract examples.

## 7. Expose session-qualified API and explicit startup

- [x] 7.1 Add system status, catalog pages, session workspace, session-qualified snapshot views, typed native provenance, and per-session retry to `crates/pi-trace`. Test coherent revision binding, known-session-only source access, retired snapshots, concurrent loads, handler limits, and preservation of existing offline and collector-watch contracts.
- [x] 7.2 Apply Host, Origin, loopback, no-store, bounded error, and content-reveal rules to every added route. Test foreign and absent Origins according to the existing read policy, Origin-required recovery, forged IDs and paths, unsupported queries, private-body leaks, and mode-conflict controls.
- [x] 7.3 Add native and Node `system` parsing, repeated root and disabled-provider options, canonical configuration identity, caller-cwd resolution, and ownership-safe shutdown. Extend CLI and launcher tests for spaces, conflicts, same-config reuse, unrelated processes, cancellation, and unchanged invocation forms.
- [x] 7.4 Add `/pi-trace system` with inherited configuration, clear help, and existing owner-session disposal. Verify extension-host tests prove passive registration, no capture hooks, no shell path parsing, visible startup conflicts, and no stranded workers.
- [x] 7.5 Update API, README, and packaging command documentation for system mode and effective root scope. Verify examples against the running synthetic service and retain explicit proposed versus verified behavior until integration passes.

## 8. Add the session browser and coherent live investigation

- [x] 8.1 Add strict decoders for system, catalog, session, capability, typed provenance, and native usage contracts in `web/src/lib/trace-api.ts`. Extend `tests/trace-api.test.mjs` from actual Rust serialization for every variant, required null, bound, exact counter, stale revision, and provider namespace.
- [x] 8.2 Add catalog state and UI with provider and project filters, pagination, coverage, freshness, and explicit failures. Verify controlled-response behavior for reorder, filtered-out selection, new sessions, unavailable roots, and untrusted labels without deriving prompt previews.
- [x] 8.3 Add cancellable catalog and selected-session polling, coherent snapshot replacement, visibility resync, timeout and backoff, and session/run/span-scoped selection. Verify late-response rejection, two clients selecting different providers, retained rows, expired snapshots, rewrite resets, and bodies hidden after changed evidence.
- [x] 8.4 Drive all four native stores through the real HTTP service and browser. Save screenshots and network evidence for catalog discovery, tool and usage updates, switching during requests, malformed or stale sources, and retention deletion in both themes and at 375 px. Verify semantic controls, keyboard focus, text rendering, no external requests, and deliberate reveal.
- [x] 8.5 Add a reproducible system-session verification document with fixture identity, expected per-provider values, browser commands, and evidence paths. Verify documented observations against the real run and distinguish persisted freshness, core semantics, HTTP, browser, and package evidence.

## 9. Verify integrated behavior and distribution readiness

- [x] 9.1 Run the declared 30-second four-provider workload with 1,000 catalog entries, four changing sessions, and ten committed records per second. Record machine and producer timestamps, catalog appearance, and coherent selected rows and inspector. Verify all sampled new sessions appear within five seconds and selected updates within two seconds; report initial-sweep completeness separately.
- [x] 9.2 Measure idle and active CPU, retained memory, parse counts, revision stability, and fair progress during a historical backlog. Verify configured cache, decoded-retention, database-transaction, and worker bounds, and prove polling does not repeatedly parse all unselected history. Record results without claiming a baseline speedup.
- [x] 9.3 Run `npm run check` on the final implementation, including synchronized versions, Node tests, Rust formatting, clippy and tests, Svelte checks and build, and strict validation of all changes. Record the complete exit result and any pre-existing unrelated failures separately.
- [x] 9.4 Inspect `npm pack` output, extract outside the checkout, and start system mode from another cwd against synthetic roots containing spaces. Verify all four providers, Codex zstd and SQLite availability, a new session and update, source immutability, configuration conflicts, and ownership-safe shutdown without provider executables or build fallback.
- [x] 9.5 Reconcile README, compatibility notes, API docs, verification evidence, package exclusions, MIT notices, and OpenSpec context with verified runtime behavior. Run `npm run version:check` before any separately authorized commit. Confirm no raw sessions, prompts, credentials, or local store copies enter Git or the npm archive.

## Workflow follow-up

- Present implementation acceptance and remaining compatibility limits before archiving.
- Archive or synchronize capabilities only through a separately requested workflow.
- Use root-owned versioning for a separately authorized release. This proposal authorizes no commit, tag, push, or publication.
