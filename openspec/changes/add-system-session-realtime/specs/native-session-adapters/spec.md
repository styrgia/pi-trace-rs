# Spec Delta

## Purpose

Interpret supported native agent stores as inspectable Rust application views while preserving each provider's recorded evidence and measurement limits.

## ADDED Requirements

### Requirement: Read native Codex rollouts
The Codex adapter SHALL discover supported active and archived rollout files and interpret recorded session metadata, response items, tools, lifecycle events, and usage evidence. It SHALL distinguish repeated representations of one event and cumulative usage from independent contributions. History indexes SHALL NOT substitute for full session evidence.

#### Scenario: Cumulative token reports repeat
- **WHEN** a synthetic rollout repeats a cumulative report and contains both response-item and event representations of the same recorded interaction
- **THEN** normalized execution and accepted usage count that evidence once
- **AND** missing per-call usage remains explicitly unavailable or derived with qualified provenance

#### Scenario: A cumulative counter resets
- **WHEN** cumulative usage decreases or its model context changes without a supported comparison boundary
- **THEN** the adapter diagnoses the discontinuity instead of producing negative usage or assigning it to an unrelated generation

### Requirement: Read native Pi Agent session trees
The Pi Agent adapter SHALL interpret supported session headers, entry IDs, parent IDs, messages, tools, and explicit usage entries. It SHALL retain branch, compaction, and fork evidence without treating context ancestry as an execution-parent link. It SHALL read older supported formats without invoking a migration that modifies the source.

#### Scenario: A session branches and compacts
- **WHEN** a synthetic Pi session contains two branches, assistant usage, a tool result, compaction usage, and a standalone usage entry
- **THEN** recorded contributions remain distinct and the selected context branch is identifiable
- **AND** branch summaries do not erase historical evidence or imply a tool launched the fork

### Requirement: Read native Claude Code transcripts
The Claude Code adapter SHALL interpret supported project transcripts and subagent transcripts, including message and tool identifiers and recorded usage. Streamed or repeated revisions of one assistant message SHALL replace the same logical evidence. Only explicit, supported relationships SHALL link subagents to parent execution.

#### Scenario: Assistant revisions repeat usage
- **WHEN** several transcript records update one assistant message and repeat its usage
- **THEN** the settled message contributes usage once under its stable logical identity
- **AND** unrelated messages and tools retain their own identities and source references

#### Scenario: A subagent transcript has no unique tool link
- **WHEN** a transcript identifies a parent session but provides no unique recorded invoking tool
- **THEN** the child is discoverable with the supported session relationship
- **AND** the adapter does not fabricate a tool parent from timestamps or directory order

### Requirement: Read supported OpenCode database schemas
The OpenCode adapter SHALL discover supported SQLite stores and read sessions and durable message or event updates through consistent read-only transactions. It SHALL declare supported schema versions and diagnose unrecognized legacy formats explicitly. Schema mismatch, a busy store, and replacement SHALL produce structural states without migrations, checkpointing, or source writes.

#### Scenario: A writer commits updated message data
- **WHEN** a synthetic supported database updates an existing message and inserts a new tool result in one committed transaction
- **THEN** the adapter publishes the complete committed change coherently and replaces the existing message contribution
- **AND** it does not expose an intermediate transaction state or duplicate usage

#### Scenario: A database uses a different schema
- **WHEN** the configured store lacks a supported schema signature
- **THEN** the provider reports an unsupported-format diagnostic
- **AND** the service neither modifies the database nor guesses table meanings

### Requirement: Retain original typed provenance
Every native observation and diagnostic SHALL retain provider, store, and source identity. Plain JSONL SHALL preserve the original file and one-based physical line. Compressed JSONL SHALL identify its encoding and decoded line. Database evidence SHALL identify the database, table, stable row key, and available field path. Non-line evidence SHALL NOT invent a file-line reference.

#### Scenario: Database evidence backs a usage counter
- **WHEN** a usage field comes from a database row's JSON value
- **THEN** its detail identifies the observed database revision, table, row key, and field path
- **AND** it has no fabricated physical line number

### Requirement: Normalize evidence without inventing execution
Rust SHALL normalize supported native records into application views while retaining provider-specific capability and availability states. It SHALL create execution parents only from explicit, unique, cycle-free evidence. Missing request starts, hierarchy levels, tool outcomes, or timing SHALL remain unknown rather than being synthesized from neighboring record order.

#### Scenario: Only an assistant completion was persisted
- **WHEN** a provider persists an assistant message after completion without a request-start timestamp
- **THEN** the message is inspectable with its recorded timestamp and usage
- **AND** generation duration and first-token latency remain unknown

### Requirement: Preserve provider token meanings
Normalized usage SHALL distinguish non-cached input, output, cache-read, and cache-write tokens and retain native reasoning breakdowns and inclusion rules. A derived normalization SHALL disclose its operands and provenance. Missing fields SHALL remain unknown, recorded zero SHALL remain zero, and totals SHALL avoid adding a subset to its containing counter.

#### Scenario: Reasoning is included in one provider's output
- **WHEN** one provider reports reasoning as part of output and another reports visible output and reasoning as disjoint counters
- **THEN** normalized output includes reasoning once in both cases
- **AND** native recorded counters and the normalization rule remain inspectable

#### Scenario: Cached input is a subset of total input
- **WHEN** a provider records input including cached tokens and supplies a cache-read subset
- **THEN** non-cached input uses checked subtraction with derived provenance
- **AND** an absent cache-write counter stays unknown rather than becoming zero

### Requirement: Account for native logical contributions once
Usage SHALL follow provider-specific logical contribution identity and replacement rules. Repeated messages, cumulative totals, copied fork prefixes, session aggregates, and compaction summaries SHALL NOT be added as independent usage when they repeat accepted evidence. Historical billed work and selected-context contents SHALL remain distinct.

#### Scenario: OpenCode compaction repeats its usage event
- **WHEN** a supported store contains assistant usage, a compaction summary, a durable usage event, and session aggregate counters
- **THEN** the adapter applies the documented authority and deduplication rules to count each recorded contribution once
- **AND** a total assembled from incompatible or incomplete evidence is marked partial or unknown

#### Scenario: A fork contains copied history
- **WHEN** explicit fork metadata identifies a copied prefix and later new work
- **THEN** the copied prefix remains inspectable as inherited history
- **AND** it does not become newly incurred usage in the fork's own execution totals

### Requirement: Preserve incomplete and malformed native input
JSONL live readers SHALL retain unfinished bytes until a complete record is available. Malformed complete records and unsupported variants SHALL produce structural diagnostics with original provenance while valid surrounding evidence remains inspectable. Diagnostics SHALL omit raw record text. Stored timestamps SHALL NOT substitute for unrecorded durations.

#### Scenario: A write splits UTF-8 and JSON
- **WHEN** a native transcript record arrives in several writes before its final newline
- **THEN** its unfinished tail remains pending without a permanent malformed-record diagnostic
- **AND** completing the record interprets it once at its original physical line

### Requirement: Respect native store update semantics
Append, record replacement, transactional upsert, archive moves, and supported source rewrites SHALL follow their provider's declared continuity rules. A rewrite SHALL produce a new analysis run after coherent reread, with stale data visible during recovery. Unrecognized discontinuity SHALL isolate the session rather than merge incompatible evidence.

#### Scenario: Pi rewrites a session during migration
- **WHEN** a supported Pi session changes through a detected rewrite
- **THEN** the catalog keeps its session identity while analysis restarts under a new run
- **AND** old and new occurrence identities or usage are never combined into one run

### Requirement: Keep ingestion local and read-only
Native ingestion SHALL use local persisted evidence without provider credentials, network calls, source migrations, or control of agent processes. It SHALL function without the provider executable or daemon. Original store bytes and database state SHALL remain unchanged by discovery, inspection, recovery, and shutdown.

#### Scenario: Inspect synthetic stores without provider processes
- **WHEN** all four supported stores are present and provider executables and daemons are unavailable
- **THEN** catalog discovery and selected-session updates work through local persisted data
- **AND** source hashes, schema, and journal state show no writes by Pi Trace
