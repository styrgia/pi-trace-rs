# Trace input format

`trace-core` imports offline recordings from the collector at commit `5441bcca041e3b3a203aee91c68e8ea809e6aa27` of [pi-trace-extension](https://github.com/npxcnency-ux/pi-trace-extension/tree/5441bcca041e3b3a203aee91c68e8ea809e6aa27). Offline import reads JSONL through `Trace::open(source, ImportLimits)`. Explicit `watch <directory>` uses `FollowReader` to follow complete append-only records. Pi Trace does not capture Pi events.

## Sources and bounds

The selected directory must contain a readable regular `events.jsonl`. Child recordings use `subagents/<child-id>/events.jsonl`. The same layout can recur inside a child directory. Source references retain the full relative path and one-based physical line number. Empty root files produce ready empty snapshots.

Default import limits are 128 MiB of physical input, 1 MiB per physical record, 100,000 physical records, and 1,024 recognized sources. Byte counts include line terminators. Malformed, blank, and unsupported records count toward the same limits. The reader checks byte budgets before extending its record buffer and before JSON decoding.

Source discovery also visits at most 1,023 entries across recognized `subagents` directories. Ignored files and directories without `events.jsonl` consume this traversal budget. This bounds discovery even when a directory contains many unrelated entries. A custom `ImportLimits.sources` sets both the recognized-source cap and an entry budget of `sources - 1`.

Paths that resolve outside the selected root fail import. Repeated aliases or cycles through recording directories also fail. The importer compares source identities, lengths, modification times, available change times, and the source set before publication. Observable changes fail the attempt. These checks do not establish an atomic snapshot of an actively written directory. No import operation modifies its sources.

## EOF and incremental prefixes

Offline import treats EOF as final. A nonempty final physical record is decoded even without a newline. Valid JSON is accepted. A JSON decoder EOF error produces `incomplete_tail`; other invalid JSON produces `malformed_record`. Open observations receive `missing_end` diagnostics.

The shared ingestion core also supports incremental prefixes. Its per-source decoder retains an unterminated tail as bytes, including split UTF-8 sequences. A complete JSON object without a newline remains pending. Temporary EOF and empty reads neither decode that tail nor advance its physical line. Only the offline adapter explicitly finishes a tail at EOF.

Prefix projection leaves open observations provisional, with unknown end times and no final `missing_end` diagnosis. Repeated projection preserves retained evidence and diagnostic identities. A later completion updates its generation or tool. Turn and session outcomes retain the facts available at their completion marker: a summary after `turn_end` does not establish an earlier success, and a late `turn_end` does not rewrite `session_shutdown` evidence. No current-clock durations are derived.

The native CLI, installed launcher and Pi extension expose this behavior through `watch <directory>`. Existing `serve <directory>` remains an offline import.

## Append-only reader core

`FollowReader` follows the configured root through bounded calls to `update`. Watch mode connects one reader to the workspace ingestion worker. Each reader has a fresh run identity. Each source retains its occurrence state, physical line number, complete-line byte checkpoint, read high-water offset, pending bytes, observed metadata, and a 64-byte boundary sample. Activity reports accepted source, physical record, read-byte, and pending-byte counts.

An update reads at most 1 MiB or 2,000 complete records. A backlog remains visible through `catching_up`; the caller can check cancellation or controls before the next batch. This core API does not itself start a worker. No-op updates perform discovery and metadata checks without rereading JSONL data or creating snapshots. Pending-only updates commit their bytes and budgets without publishing a semantic snapshot. The initial empty prefix and new empty sources each publish once.

The supported producer appends complete records and never modifies previously observed bytes. Source identity checks use Unix device and inode metadata. Platforms without that supported check return `source_identity_unsupported`. The reader re-resolves the configured root to detect retargeted symlinks, validates containment during discovery, uses nonblocking no-follow file opens on Unix, and checks opened descriptors on Linux. Replacement, disappearance, truncation below an observed size, same-size metadata changes, and observed boundary mismatches stop the run. A rejected batch advances no retained evidence, checkpoints, or budgets. The reader then remains stopped; recovery creates a new reader and run.

These are observable continuity checks, not proof that arbitrary concurrent rewriting did not occur. Metadata-preserving changes outside the sampled boundary can evade detection, including same-size writes whose timestamps are indistinguishable at filesystem granularity and interior writes combined with append growth. Such writers violate this append-only producer contract and require a different recording protocol for reliable support.

Limits are cumulative for the reader lifetime. Physical input includes pending tails, blank lines, malformed JSON, and unsupported events. Repeated idle scans do not replenish any budget. Source traversal charges the union of observed entry paths; deleting ignored entries does not restore capacity. Existing entries are still inspected so a child directory can acquire a recording later. Retained inspection, preview, and response limits are shared with offline projection. This does not imply an upper bound on total process RSS.

Synthetic tests in `src/follow.rs` and `tests/follow.rs` exercise split UTF-8 and JSON, provisional EOF, child-before-link arrival, reverse link completion, source replacement during a staged update, truncation of an unread backlog, same-size metadata changes, boundary mismatch, root retargeting, escaping and cyclic paths, cumulative limits, inspection retention across batches, and explicit reset identities. They verify core behavior rather than browser latency, installed-package operation, or worker shutdown.

## Supported records

Common fields are `type`, `ts` in Unix milliseconds, `sessionId`, and optional `turnIndex` and `stepIndex`. The format has no global event ID or schema version. The following fields affect the application views.

| Event | Interpretation |
| --- | --- |
| `session_start` | New session epoch. String `model` and `sessionId` are bounded metadata. |
| `interaction_start` | New interaction occurrence. `prompt` is revealed content only. |
| `turn_start` | New turn attempt, even when `turnIndex` repeats. |
| `step_start` | New generation occurrence, even when an index repeats. |
| `llm_request` | `input` is the generation's revealed input. |
| `llm_response` | `durationMs` is request-to-HTTP-headers latency. |
| `step_end` | `durationMs` is completed-generation duration. `text` is revealed output. `usage` and `stopReason` describe this generation. |
| `tool_start` | `toolCallId`, `stepIndex`, and bounded `toolName` identify the tool occurrence. `args` are revealed input. |
| `tool_end` | `toolCallId` and `stepIndex` correlate completion. `durationMs` and `isError` describe the tool. `resultPreview` is revealed output. |
| `turn_summary` | `usage` is a fallback only when no generation usage was observed. `stopReason` supplies final outcome evidence. `userPrompt` and `finalText` are revealed content. |
| `turn_end` | Completion marker for the current turn. |
| `file_change` | Adds a source reference to the current turn. File paths and change bodies remain outside metadata. |
| `session_shutdown` | Session completion marker. |

The reader also accepts explicit `inputTokens`, `outputTokens`, and `totalTokens` usage fields. Content aliases are `messages` or `prompt` for generation input, `message` or `output` for generation output, `arguments` or `input` for tool input, and `result` or `output` for tool output. These aliases do not create additional observations or usage.

Malformed records, incomplete final JSON, non-object records, unknown event types, missing starts, and missing completions produce structural diagnostics. Surrounding valid records remain inspectable. Error messages never include decoder text or raw record snippets.

## Occurrences and relationships

Session starts reset occurrence correlation. Interaction and turn starts create distinct occurrences rather than replacing reused indices. Outstanding tool calls remain associated with their original generation when a later step starts. Tool completions use the recorded step index when present and require a unique outstanding tool ID within the current turn. Ambiguous or unmatched completions produce diagnostics.

A tool's `subagent.results[].childTraceId` links the child recording under that tool's source directory. The reader also accepts `childTraceId`, `result.childTraceId`, and `result.details.childTraceId`. A separate linking pass resolves these identifiers after all sources are imported. Completion order does not determine parentage. Multiple candidates and cycles remain ambiguous. A child without a recorded identifier remains unresolved at the top level. The reader does not reproduce the collector's timestamp or FIFO heuristics.

Rows expose presentation ancestry as `parentId`. Details separately expose `relationship.state` and `executionParentId`. Only `linked` relationships supply an execution parent. Synthetic enclosing observations for missing starts retain a source reference and a `missing_start` diagnostic. Their start times remain unknown.

## Timing and outcomes

Measurements have `recorded`, `derived`, `unknown`, or `invalid` state. `step_end.durationMs` takes precedence for generation duration. `llm_response.durationMs` never replaces it. Exact TTFT is unknown because this baseline does not record it.

When no duration is recorded, paired finite start and end timestamps produce a derived interval. A backward or overflowing interval produces a diagnostic. Its duration is invalid unless an independent valid recorded duration exists. It never produces a waterfall bar. An incomplete observation with no end time retains unknown duration.

Rust supplies the global valid timeline span and each valid bar's offset and duration. An observation can have a recorded duration without a bar when its position is unknown or invalid.

A failed tool retains its own error outcome. Turn summaries with `stop` or `length`, followed by `turn_end` and completed generation evidence, establish successful turn and interaction completion. `error` and `aborted` establish errors. `toolUse` completes the turn but leaves the interaction incomplete until a later final turn. Missing generation completion or missing final evidence does not become success. There are no inferred retry or recovery links.

## Usage

Baseline usage fields are `input`, `output`, `cacheRead`, `cacheWrite`, and numeric `cost`. Each field remains nullable in views. A token total is calculated only when all four token components exist, or supplied by an explicit `totalTokens`. Missing fields are never replaced with zero. A recorded zero remains zero.

Generation usage is counted once per turn. A repeated generation completion does not add usage. When any generation usage field is present, overlapping turn-summary usage is excluded. Coverage reports observed and total generation counts. Missing generations or token components produce `partial` usage. Missing cost remains null independently of token coverage.

When no generation usage exists, the turn summary is a fallback with `fallbackSource: "turnSummary"`. A partial summary stays `partial` with that fallback source. A complete summary uses `fallback` state. Overview usage sums turn attempts across sources once. Session and interaction usage aggregate their own turns before child linking, so attaching a child does not count its usage twice.

The exact accounting view is additive to legacy usage. It gives each accepted generation or eligible turn-summary contribution a run-scoped identity. Recognized invalid usage suppresses fallback and produces structural diagnostics. Each counter uses checked u64 arithmetic and a decimal-string API value, preserving zero and integers above JavaScript's safe-number range. A conflicting explicit total is retained as reported evidence while its accepted total becomes invalid. The maximum supported exact counter is `18446744073709551615`.

Own accounting identifies recorded generation usage or an eligible summary fallback. Inclusive accounting combines disjoint canonical contributions over verified execution links, including child model execution under a tool. Overview sums contributions independently of tree position. Nested inclusive values must not be added together. Missing evidence, known partial subtotals, invalid values, and non-applicable tool usage remain distinct. Details sample at most 16 contributions with source references and omitted counts. Generation-local context input is separately exposed as `contextInputTokensExact`, a checked sum of recorded input, cache-read and cache-write counters. See [exact span accounting](trace-api.md#exact-span-accounting) for the serialized contract and [live verification](live-verification.md) for acceptance evidence.

## Content and metadata

Display labels and string metadata are bounded to 512 UTF-8 bytes. Tool labels come from `toolName`, never from command or path arguments. Labels carry a truncation indicator. Source references remain intact.

Each explicit input or output preview is at most 8 KiB of valid UTF-8. Missing fields are `unavailable`, not empty successful content. `resultTotalLength` uses the collector's UTF-16 length convention to retain upstream truncation evidence. The preview limit also applies when the recording itself did not truncate the value.

Metadata can still be sensitive. These limits are not a sanitization claim. Raw recordings, prompts, credentials, and local session files stay outside Git. The checked-in recordings in `crates/trace-core/tests/fixtures` are fabricated and excluded from the npm archive. The MIT notice remains in `THIRD_PARTY_NOTICES.md`.

## Capture boundary

[Pi extensions](https://github.com/earendil-works/pi/blob/e7a0df27c09d11bf4161d3605246cbf57a129cb2/packages/coding-agent/docs/extensions.md) run in JavaScript or TypeScript. This Rust application consumes external collector recordings. Pi's RPC mode would be a separate event source with different capture fidelity.


## Native source configuration

Explicit `system` mode reads native local stores independently of collector `events.jsonl` input. It includes supported saved and newly persisted sessions beneath startup-selected roots. It does not scan other OS users, remote hosts, cloud history or arbitrary earlier custom homes. No-source startup continues to open an empty offline workspace.

The CLI and native executable accept repeated `--root <provider>=<path>` and `--disable-provider <provider>` options. An explicit root set replaces that provider's defaults. Multiple distinct roots for the same provider are allowed; canonical aliases deduplicate. A configured provider cannot also be disabled. Unknown providers, empty roots and more than 16 stores fail before service startup. Relative paths resolve from the caller's working directory.

| Provider | Default and inherited root | Supported discovery scope |
| --- | --- | --- |
| `codex` | `${CODEX_HOME:-~/.codex}` | `sessions` and `archived_sessions` rollout trees, including `.jsonl` and `.jsonl.zst` |
| `pi` | `${PI_CODING_AGENT_DIR:-~/.pi/agent}/sessions`; `PI_CODING_AGENT_SESSION_DIR` overrides that session directory | Encoded project layouts or explicitly selected session directories, identified from headers |
| `claude` | `${CLAUDE_CONFIG_DIR:-~/.claude}/projects` | Project transcripts and recursive subagent transcripts, with detected optional metadata sidecars |
| `opencode` | `${XDG_DATA_HOME:-~/.local/share}/opencode`; `OPENCODE_DB` can select one database | `opencode*.db` files in a selected data directory, or one explicitly selected database file |

Relative `OPENCODE_DB` values must remain inside the OpenCode data directory after lexical and canonical resolution. An absolute value selects exactly that file. An explicit OpenCode root can name a data directory or a database file; an explicitly selected file does not require a `.db` extension. OpenCode `:memory:` is rejected. Literal directories with that name remain valid for other providers. Pi's per-process `--session-dir` cannot be recovered from another invocation; select it with an explicit root or supported inherited setting.

The Node launcher resolves and sorts every enabled provider's roots before starting Rust in the package directory. It forwards explicit roots for every enabled provider and explicit disable flags. Owned-process identity includes mode and effective canonical provider configuration. Changing configuration requires stopping the owned service; a conflict does not replace another process. The Pi extension accepts `/pi-trace system` without options and inherits supported settings. Explicit provider options belong to the terminal CLI.

Guarded system status discloses canonical effective roots, provider coverage and availability. Paths and project metadata can be sensitive. No browser route selects a root, and source traversal does not follow escaping links.

## Native compatibility profiles

These pins define supported profiles. They are not claims about every historical or future provider version. The source-checkout fixture inventory at `crates/trace-core/tests/fixtures/NATIVE.md` records source references and synthetic cases.

| Provider baseline | Pinned source | Supported profile |
| --- | --- | --- |
| Codex 0.166.0 | [`6ff1e1ca3b30fc8b914720681fcf5fe62e95f856`](https://github.com/openai/codex/tree/6ff1e1ca3b30fc8b914720681fcf5fe62e95f856) | Legacy and paginated rollout records, completed items, active and archived JSONL, streaming zstd and explicit history-base references |
| Pi Agent 1.4.0 | [`4a080252f06caf63a525d7873c5d94d3c23887e0`](https://github.com/earendil-works/pi/tree/4a080252f06caf63a525d7873c5d94d3c23887e0) | Session headers with absent version or versions 1, 2 and 3, decoded without migration APIs |
| Detected Claude transcript, CLI baseline 2.1.216 | [Official Python SDK reader at `1ea8cb74ddcdadbfaa9b20dfa8268e18047851e7`](https://github.com/anthropics/claude-agent-sdk-python/blob/1ea8cb74ddcdadbfaa9b20dfa8268e18047851e7/src/claude_agent_sdk/_internal/sessions.py) | Transcript UUID and API-message records, recursive subagents and optional `.meta.json` sidecars up to 64 KiB |
| OpenCode 2.1.4 | [`68bc26b7da1990ce10ffecfbc1e09eb13aa5d824`](https://github.com/anomalyco/opencode/tree/68bc26b7da1990ce10ffecfbc1e09eb13aa5d824) | Version-2 `session_v2`, `session_message` and durable `event` tables |

Codex identity distinguishes a logical thread from a recording or revert rollout. Archive moves and equivalent plain or compressed representations preserve catalog identity; a plain sibling takes precedence over its compressed representation. Physical history-prefix offsets remain separate from logical fork or subagent boundaries. A physical prefix ending inside a record or beyond its source is rejected. Same-thread reverted history remains billed work; explicitly inherited fork evidence does not become new work. Missing attribution boundaries remain partial.

Pi headers provide project metadata. Lossy directory names are not decoded into invented project identities. Tree parent links and selected context remain separate from historical billed work. Assistant, tool-result, standalone usage, compaction and branch-summary evidence can contribute without inventing generation spans. Unsupported older tree identities remain unavailable. Native files are never migrated or rewritten.

Claude assistant revisions use recorded API-message identity to avoid counting repeated usage. Tool-use and tool-result IDs support explicit correlation, including parallel tools. Ambiguous reused IDs do not become guessed execution parents. Optional sidecar relationships are detected SDK capabilities; their presence is not assumed for every installed CLI. Sidecar-only changes can refresh relationships without reparsing transcript history.

OpenCode tools and message state come from typed persisted elements. Assistant and durable usage evidence reconcile compaction copies rather than add them twice. Session aggregate columns remain reconciliation evidence. Fork boundaries qualify inherited prefixes. Legacy JSON stores are outside the supported profile. Incompatible SQLite schemas are reported as unsupported; no migration or provider bootstrap runs.

## Native timing and usage evidence

Native timing preserves recorded meaning. Persistence timestamps, request starts, result creation and execution boundaries are distinct. A Pi assistant's recorded monotonic duration does not turn its outer persistence timestamp into an execution start. An assistant completion without a request start leaves duration and TTFT unknown. Completed execution intervals use checked subtraction; invalid or overflowing intervals remain invalid. Storage freshness never establishes a running process or completed session.

Codex input includes cached input, so non-cached input uses checked subtraction. Reasoning remains a subset of output. The canonical cache-write field is `cache_write_input_tokens`; an absent field stays unknown. Response-level `token_usage_record` evidence has authority over repeated representations. Comparable monotonic cumulative reports can produce same-context derived deltas. Unknown boundaries, resets and incompatible totals remain qualified rather than guessed.

Pi reasoning is included in output, and `cacheWrite1h` is included in cache-write. Claude cache-creation subdivisions are included in their aggregate. OpenCode records visible output and reasoning as disjoint components, so normalized output uses their checked sum. Breakdown counters do not add to their containing counter again.

Native contribution roles distinguish accepted contribution, reconciliation, inherited evidence and breakdown. Auxiliary or cumulative contributions do not imply model generations or collector fallbacks. Selected context identifies the provider's context selection; it is not cumulative billed usage. Accepted exact counters use checked u64 arithmetic and decimal-string JSON values. Recorded zero, unknown, partial, invalid and not-applicable values remain distinct. The [native API reference](trace-api.md#native-provenance) preserves native counters, normalization operands, authority and original provenance.

## Native persistence, recovery and read-only storage

Plain JSONL following retains incomplete bytes until a complete record arrives, including split UTF-8. Malformed complete records produce structural diagnostics with original locators while surrounding supported evidence remains inspectable. Compressed historical input uses bounded streaming zstd decoding and can accept its final unterminated record under historical EOF semantics. Corrupt compressed input retains the original archive locator. Header and record byte ranges are actual ranges; unknown offsets are null.

Supported replacements and rewrites produce a fresh analysis run rather than concatenate incompatible evidence. Observed inode replacement, truncation, equal-length metadata change and a mismatch at the retained 4 KiB consumed tail trigger recovery. These checks do not detect arbitrary earlier edits that preserve every observed metadata field and retained boundary. An interior rewrite combined with append growth can escape those observations. The implementation does not claim adversarial rewrite detection.

A source failure affects its session or store. Last-valid snapshots remain readable as stale evidence. Failed discovery never proves deletion: unseen known sessions survive incomplete sweeps until a later clean exhaustive pass confirms absence. Recovery stays bound to configured sources.

SQLite access uses bundled `rusqlite` and SQLite in read-only URI mode with no-follow opens. The supported unix VFS uses `readonly_shm=1`; it does not use `immutable=1` for a live database. A WAL database requires usable existing sidecars, including a readable SHM file. Missing sidecars cause a structural failure rather than creation, migration, permission repair or checkpointing. The application opens databases through the bundled library and does not invoke provider executables.

One shared connection per discovered database owns data-version checks and transaction state. File identity, schema checks and same-connection data version detect relevant changes without a timestamp-only cursor. A selected-session reconciliation uses successive bounded batches inside one coherent read transaction. Defaults are a 25 ms busy timeout, a 100 ms statement deadline and a one-second transaction lease. Other reads can receive transient `database_busy` during that lease. Commit occurs after the selected evidence is coherent; cancellation, errors, reader drop and deadlines roll back. A later committed writer revision is read on the next reconciliation.

The source-immutability tests use a separate synthetic writer process and compare database, WAL and SHM bytes during static phases. This avoids a same-process writer masking a writable shared-memory mapping. Application code does not run provider initialization, migrations, chmod, write pragmas or checkpoints. Runtime use needs neither a provider daemon nor an external `sqlite3` command.

## Native bounds and evidence scope

Native defaults are 128 MiB decoded input per session, 100,000 records, 1 MiB per record or decoded row, 1,024 related sources and 256 MiB compressed input. Each batch handles at most 1 MiB or 2,000 records. Catalog discovery, retained analyses and global database connections have separate limits in [the API reference](trace-api.md#scheduling-and-retained-data-limits).

The 256 MiB global budget conservatively counts input-equivalent bytes for readers and retained current and previous snapshots. An old large revision remains charged until retirement. Concurrent parse jobs reserve decoded input and projection headroom before I/O. This budget can reject work before the per-session ceiling and does not guarantee process RSS.

Synthetic core, HTTP, launcher and controlled UI tests establish their own behaviors. They do not establish live-browser usability, five-second catalog appearance, two-second selected-view latency, CPU or memory workload results, or standalone package operation. Their recorded observations and limits are in [system-session verification](system-session-verification.md). Raw provider homes, prompts and credentials remain outside Git. Existing MIT attribution and bundled dependency notices remain in `THIRD_PARTY_NOTICES.md`.
