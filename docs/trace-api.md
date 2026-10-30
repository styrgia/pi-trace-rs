# Local trace API

The native service binds to `127.0.0.1:4317`. `pi-trace serve <directory>` selects an offline source, and `pi-trace watch <directory>` selects an append-only source for the process lifetime. No arguments starts an empty workspace. The shell and health route are available while a blocking worker imports the selected source. Static assets resolve from `web/dist` in the package working directory.

## Workspace

`GET /api/v1/health` retains the launcher contract. Its response is `{"status":"ok","name":"pi-trace","version":"<package version>","stage":"bootstrap"}`. The stage describes the legacy readiness contract, not recording state.

`GET /api/v1/workspace` returns one of these serialized shapes.

```ts
type Workspace =
	| { state: 'empty' }
	| { state: 'importing'; attemptId: string }
	| { state: 'ready'; attemptId: string; snapshotId: string }
	| { state: 'failed'; attemptId: string; error: StructuralError };
```

`POST /api/v1/retry` accepts no body or query. In offline mode, only a failed configured source can transition to importing. The transition reserves one worker atomically. A successful request returns `202` and `{state: 'importing', attemptId}`. Active and ready offline workspaces reject retry. Offline retry does not replace a source or preserve an old snapshot.

## Live workspace lifecycle

`pi-trace watch <directory>` follows the startup-selected append-only source. Rust owns one ingestion worker for the workspace. Health remains process readiness. A recorded `session_shutdown` does not stop following.

Watch workspace responses extend the existing variants with `live`. Offline responses omit this property.

```ts
type LiveStatus = {
    runId: string;
    revision: number | null;
    state: 'catchingUp' | 'waiting' | 'paused' | 'stopped';
    activity: {
        sourceCount: number;
        recordCount: number;
        readBytes: number;
        pendingBytes: number;
    } | null;
    error: StructuralError | null;
};
type LiveWorkspace = Workspace & { live: LiveStatus };
```

A start allocates one opaque run identity. Before first publication the workspace is importing, or failed if initialization fails. Each semantic publication advances revision starting at 1 and assigns a new snapshot ID. Idle scans and pending-only reads update activity without replacing the snapshot or advancing revision. An empty valid prefix publishes a ready snapshot. A failed update stops the run and preserves its latest valid ready snapshot. The structural error appears in `live.error`.

`POST /api/v1/live/pause` and `POST /api/v1/live/resume` accept no body or query and require an approved Origin. Success returns `200` with the current live workspace. Pause is acknowledged at a worker batch boundary. All clients share the pause. Detail and explicit content reveal remain available. Repeated pause while paused and resume while active are idempotent. Resume validates the source before acknowledging success. Validation failures return a structural `409` and preserve the last valid snapshot.

`POST /api/v1/retry` also accepts a stopped live run. Its atomic reservation returns `202` with an importing workspace, a fresh run identity and null revision. One concurrent retry succeeds. The previous run's snapshots remain addressable until the new run first publishes, then retire. If the replacement run fails before publishing, its workspace is failed with null revision and no current snapshot ID; old snapshot routes remain independently addressable. Offline retry behavior is unchanged.

The service retains only the current and immediately previous publication. `GET /api/v1/snapshots/{snapshot}/spans/{spanId}` resolves one stable span independently of the current observation page or filter. It returns the existing node detail shape, including the snapshot-local node ID needed for explicit content reveal. Span identity is meaningful within one run. Node IDs and cursors remain snapshot-scoped. Requests for retired snapshots return `unknown_snapshot` without substituting current data. A span absent from the requested snapshot returns `unknown_span`.

At most eight trace-view handlers run simultaneously. Excess requests return `503` with structural `busy`. Live controls use a bounded eight-entry queue and can also return `busy`. Offline controls return `409` with `mode_conflict`; stopped live controls return `live_stopped`. Existing Host, Origin, no-store, payload, query, response-size and explicit-reveal policies also apply to the added routes. Errors never echo request bodies, queries or source paths supplied by a caller.

The worker handles at most 1 MiB or 2,000 records per batch, checks controls between batches and polls caught-up sources every 250 ms. Import budgets remain cumulative across idle scans, pause and resume. Shutdown requests cancellation at a batch boundary and awaits worker completion. Sources remain read-only.

The synthetic `workspace-live-*` and `control-live-*` examples in `crates/pi-trace/tests/contracts/` are checked against service serialization and HTTP responses. Tests normalize generated run and snapshot IDs to stable synthetic placeholders.

## Snapshot views

Every successful snapshot view includes `snapshotId`. Occurrence IDs and cursors are opaque and meaningful only within their snapshot. Stable `spanId` identifies a span within its run.

| GET route | Result |
| --- | --- |
| `/api/v1/snapshots/{snapshot}/overview` | Recording counts, usage, measurement availability, and global timeline domain |
| `/api/v1/snapshots/{snapshot}/observations` | Ordered observation page with match and ancestor-context flags |
| `/api/v1/snapshots/{snapshot}/nodes/{node}` | One independently retrievable detail without recorded bodies |
| `/api/v1/snapshots/{snapshot}/nodes/{node}/content` | Explicit bounded input and output previews |
| `/api/v1/snapshots/{snapshot}/diagnostics` | Structural source diagnostics with optional observation targets |

Observations accept `q`, `filter`, `limit`, and `cursor`. Search is limited to 256 Unicode characters and matches observation IDs, kinds, bounded display labels, session IDs, model names, and tool names. It never searches content previews. Filters are `error`, `tool`, `generation`, and `slow`. `generation` matches generation observations and retains their recorded ancestors as context. `slow` means a valid generation or tool duration greater than 5,000 ms. Context rows do not count as matches.

Pages default to 100 items and accept 1 through 200. Cursors are bounded to 2 KiB and bind to the snapshot, exact search, filter, and page size. Diagnostics accept only `limit` and `cursor`. Repeated or unsupported parameters fail. Other view routes and workspace accept no query parameters. The encoded query budget is 16 KiB.

Rust forms the complete ancestor closure in deterministic hierarchy order before pagination. Pages partition that order. Ancestors can occur on an earlier page, so clients append pages. `nextCursor: null` marks the end. `totalMatches` counts matching observations, and `totalRows` includes context. Diagnostic pages use `total`.

Every view is checked against a 1 MiB serialized JSON budget, including the full envelope and continuation token. Pages can contain fewer than the requested number of items when the byte budget is reached. An item or non-paged view that cannot fit returns `view_limit`.

## Serialized types

`crates/trace-core/src/views.rs` defines the exact Rust DTOs. `web/src/lib/trace-api.ts` validates their browser representation. All property names and enum values use camel case. Nullable fields are present as JSON null.

```ts
type SourceRef = { file: string; line: number };
type StructuralError = { code: string; message: string; source: SourceRef | null };
type Measurement = {
	state: 'recorded' | 'derived' | 'unknown' | 'invalid';
	valueMs: number | null;
	meaning: string;
};
type Usage = {
	state: 'complete' | 'partial' | 'fallback' | 'unknown';
	inputTokens: number | null;
	outputTokens: number | null;
	cacheReadTokens: number | null;
	cacheWriteTokens: number | null;
	totalTokens: number | null;
	cost: number | null;
	observedGenerations: number;
	totalGenerations: number;
	fallbackSource: 'turnSummary' | null;
};
type Observation = {
	id: string;
	spanId: string;
	spanUsage: SpanUsage;
	kind: 'session' | 'interaction' | 'turn' | 'generation' | 'tool';
	label: string;
	labelTruncated: boolean;
	parentId: string | null;
	depth: number;
	hasChildren: boolean;
	match: boolean;
	context: boolean;
	outcome: 'success' | 'error' | 'unknown' | 'incomplete';
	duration: Measurement;
	bar: { offsetMs: number; durationMs: number } | null;
	source: SourceRef;
};
type ObservationPage = {
	snapshotId: string;
	items: Observation[];
	totalMatches: number;
	totalRows: number;
	nextCursor: string | null;
};
type NodeDetail = {
	snapshotId: string;
	spanUsage: SpanUsageDetail;
	observation: Observation;
	relationship: {
		state: 'root' | 'linked' | 'unresolved' | 'ambiguous';
		executionParentId: string | null;
	};
	requestToHeaders: Measurement;
	ttft: Measurement;
	usage: Usage;
	metadata: {
		sessionId: string | null;
		model: string | null;
		toolName: string | null;
		turnIndex: number | null;
		stepIndex: number | null;
	};
	context: ContextInspection | null;
	toolInspection: ToolInspection | null;
	sources: SourceRef[];
	diagnosticIds: string[];
};
type ContentField = {
	state: 'available' | 'unavailable';
	text: string | null;
	truncated: boolean;
	limitBytes: 8192;
};
type NodeContent = {
	snapshotId: string;
	nodeId: string;
	input: ContentField;
	output: ContentField;
};
type Diagnostic = {
	id: string;
	code: string;
	message: string;
	source: SourceRef;
	nodeId: string | null;
};
type DiagnosticPage = {
	snapshotId: string;
	items: Diagnostic[];
	total: number;
	nextCursor: string | null;
};
type Overview = {
	snapshotId: string;
	title: string;
	source: SourceRef | null;
	model: string | null;
	sourceCount: number;
	recordCount: number;
	nodeCount: number;
	sessionCount: number;
	interactionCount: number;
	turnCount: number;
	generationCount: number;
	toolCount: number;
	diagnosticCount: number;
	duration: Measurement;
	usage: Usage;
	timeline: { startMs: number; endMs: number; durationMs: number } | null;
	measurementAvailability: { duration: boolean; requestToHeaders: boolean; ttft: boolean };
};
```

Measurement meanings, usage coverage, and relationships are specified in [Trace input format](upstream-compatibility.md). The browser maps valid bars to pixels and does not infer missing measurements.

## Request and tool inspection

`NodeDetail.context` applies only to generations. A generation without a request returns `coverage: missing`; null means the observation is another kind. `NodeDetail.toolInspection` applies only to tools. Neither view contains recorded body text. Explicit definition and descriptor name fields are the only catalogue text in these summaries.

Rust inspects the selected request while the complete bounded record is available. Presence precedence is `messages`, `prompt`, then `input`, including explicit null. A repeated request replaces both summary and content preview together and emits `duplicate_request`. Multiple candidate fields emit `multiple_request_fields`. Requests are never reread from disk.

Supported composition shapes are message arrays, plain strings, and an envelope containing an `input` or `messages` array with optional `system` and `tools`. The provider input array recognizes `additional_tools`, role-based message content, `function_call`, `function_call_output`, and opaque `reasoning`. Text blocks recognize string content and `text`, `input_text`, or `output_text` fields. Unrecognized components retain their compact JSON size in Other and make coverage partial. Unsupported objects have a measured payload size and unknown composition counts. Plain strings have known size and unknown role and catalogue counts.

```ts
type Coverage = 'complete' | 'partial' | 'unsupported' | 'missing';
type Retention = 'complete' | 'listsLimited' | 'retentionLimited';
type ValueSize = { compactJsonBytes: number; textChars: number | null };
type ContextKind = 'systemInstructions' | 'developerInstructions' | 'user'
	| 'assistantHistory' | 'toolCalls' | 'toolOutputs' | 'toolDefinitions'
	| 'opaqueReasoning' | 'other';
type ContextInspection = {
	coverage: Coverage;
	retention: Retention;
	shape: 'messages' | 'providerInput' | 'plainText' | 'unsupported' | 'missing';
	requestField: 'messages' | 'prompt' | 'input' | null;
	source: SourceRef | null;
	payloadSize: ValueSize | null;
	counts: {
		messages: number | null;
		toolDefinitions: number | null;
		skillDescriptors: number | null;
		unclassifiedItems: number;
	} | null;
	categories: { kind: ContextKind; count: number; size: ValueSize; textCoverage: Coverage }[];
	largestItems: { ordinal: number; kind: ContextKind; size: ValueSize; textCoverage: Coverage }[];
	largestTools: NamedContributor[];
	largestSkills: NamedContributor[];
	sections: { id: number; parentId: number | null; kind: string; itemOrdinal: number; textChars: number }[];
	omitted: { items: number | null; tools: number | null; skills: number | null; sections: number | null };
	contextInputTokens: number | null;
	contextInputTokensExact: TokenCounter;
};
type NamedContributor = {
	name: string | null;
	nameTruncated: boolean;
	ordinal: number;
	size: ValueSize;
};
type PreviewEvidence = {
	availability: 'available' | 'unavailable';
	selectedSize: ValueSize | null;
	previewUtf8Bytes: number | null;
	source: SourceRef | null;
	localTruncated: boolean;
	sourceTruncation: 'knownTruncated' | 'knownComplete' | 'unknown';
};
type ToolInspection = {
	retention: 'complete' | 'retentionLimited';
	callId: string | null;
	callIdTruncated: boolean;
	callIdSource: SourceRef | null;
	arguments: PreviewEvidence;
	result: PreviewEvidence & {
		recordedField: 'resultPreview' | 'result' | 'output' | null;
		originalLength: {
			value: string;
			unit: 'utf16CodeUnitsOfSerializedResult' | 'unspecified';
			source: SourceRef;
		} | null;
	};
};
```

JSON bytes measure exact compact serde reserialization, not HTTP wire bytes. Text characters count recognized Unicode scalar values. They do not include schema keys, tool descriptions, opaque reasoning, or unknown blocks. Category byte sizes cover disjoint serialized components, exclude envelope overhead, and do not sum to payload size. `textCoverage` separates known text from partial or uninterpretable component content. No component token attribution is available.

Sections use a fixed delimiter lexer only for system and developer text. Supported names are `tools`, `rules`, `docs`, `project_context`, `skills`, `available_skills`, `cwd`, `acp_delegate`, and `ponytail`. Matching `skill` descriptors under `available_skills` require an explicit `name`; descriptions and locations are not retained. Section spans and skill descriptor spans include delimiters. Nested sections overlap and are non-additive. Malformed or crossing delimiters, unsupported descriptor syntax, and nesting beyond 32 emit `context_sections_malformed`, make coverage partial, and leave the complete skill and section counts unknown. A catalogue advertises capabilities; it does not establish installation or execution.

Largest items, tool definitions, and skill descriptors retain at most 16 entries each. Sections retain at most 32 entries. Ties retain source order. Names and display-only call IDs retain at most 512 UTF-8 bytes with truncation flags. Full tool IDs still own internal correlation. Tool definitions rank by whole-definition compact JSON bytes; skill descriptors rank by whole-descriptor Unicode scalar characters. Ordinals are zero-based within each list's input domain; every entry refers to the enclosing request's actual source record.

A serialized inspection is capped at 64 KiB per node. The sum of retained compact serialized inspection payloads is capped at 16 MiB across the snapshot. This accounting excludes fixed node and allocation overhead; it is not a process-memory guarantee. Optional lists are dropped first with omitted counts. If the remaining summary cannot fit, a fixed `retentionLimited` state preserves applicability and coverage without retaining its summary payload. Tool inspection has its own explicit limited state. These metadata budgets do not fail an otherwise valid import. Missing measurements and unknown omitted counts remain null; exhaustion does not mean empty context. Existing 1 MiB import-record and response limits still apply.

`contextInputTokensExact` is the lossless decimal-counter view of a Rust checked sum of this generation's recorded input, cache-read, and cache-write counts. The numeric `contextInputTokens` field remains for compatibility and can round in JavaScript. Every operand must be present; no turn-summary fallback or session total substitutes for one. Output tokens remain separate. Session and container usage is cumulative and can repeatedly include cached context.

Tool argument/result evidence separates the whole recorded selected value from the bytes retained for explicit reveal. `localTruncated` records only Pi Trace's 8 KiB byte cap. `sourceTruncation` describes collector evidence, with unknown completeness when original length is absent or contradictory. The pinned collector appends its marker after a 500, 3,000, or 8,000 UTF-16-unit prefix; a literal marker outside that convention does not establish truncation. Under this supported unversioned format, `resultTotalLength` is the UTF-16 code-unit length of the serialized result. Explicit foreign collector or schema markers keep the number with an unspecified unit. `originalLength.value` uses a canonical unsigned decimal string so every recorded u64 value remains exact in JavaScript. This original length does not measure the model-visible `function_call_output` in a subsequent request. The existing content field's `truncated` combines known source and local truncation for compatibility.

## Exact span accounting

Every observation includes `spanId` and `spanUsage`. `spanId` is opaque and stable within its ingestion run, including synthetic parents and late child links. A new import run allocates new span identities. The existing `id`, node routes, and cursors remain snapshot-qualified.

Rows use this summary. Details add bounded contributor evidence under their own `spanUsage`. Overview includes `tokenAccounting`, which counts each canonical contribution across all imported sources once.

```ts
type TokenCounter =
	| { state: 'exact' | 'partial'; value: string }
	| { state: 'unknown' | 'invalid' | 'notApplicable'; value: null };
type TokenAccounting = {
	inputTokens: TokenCounter;
	outputTokens: TokenCounter;
	cacheReadTokens: TokenCounter;
	cacheWriteTokens: TokenCounter;
	totalTokens: TokenCounter;
	provenance: 'none' | 'recordedGeneration' | 'turnSummaryFallback' | 'aggregate' | 'mixed';
	observedGenerations: number;
	totalGenerations: number;
	fallbackContributions: number;
	contributorCount: number;
};
type SpanUsage = {
	own: TokenAccounting;
	inclusive: TokenAccounting;
	childAttribution: 'notApplicable' | 'complete' | 'unresolved' | 'ambiguous';
};
type SpanUsageDetail = SpanUsage & {
	evidence: {
		contributionId: string;
		spanId: string;
		source: SourceRef;
		accounting: TokenAccounting;
		reportedTotal: TokenCounter;
	}[];
	contributorCount: number;
	omittedCount: number;
};
```

Each counter measures recorded tokens in its named component. Values use canonical unsigned decimal strings from `0` through `18446744073709551615`. They never pass through JavaScript Number. `exact` has complete evidence for that counter. `partial` is a known subtotal with incomplete coverage. `unknown` has no known amount. `invalid` has rejected or conflicting arithmetic evidence. `notApplicable` means the format records no own model usage for that span.

Generations own their first accepted completion usage. A turn owns an eligible summary fallback only while none of its local generations has observed usage. Partial, cost-only, and recognized invalid generation fields suppress that fallback. Late generation usage replaces it, so totals can decrease. Summary fallback amounts are partial, retain their source, and never fill individual missing generation components. Other containers and ordinary tools have not-applicable own usage.

Inclusive counters combine own usage with uniquely linked descendants. Nested row totals are non-additive. For a 150-token generation with a tool linked to a 40-token child recording, generation own is 150, generation inclusive is 190, tool inclusive is 40, and overview is 190. Unresolved child recordings remain in overview. A missing or ambiguous child declaration qualifies inclusive coverage without attributing a guessed amount. Ordinary tools without declared child execution have not-applicable inclusive usage. Empty containers retain unknown usage.

A valid explicit total can be exact while components remain unknown. Without an explicit total, Rust derives one only when all four components exist and their checked sum fits u64. Zero remains recorded zero. `invalid_usage_value`, `usage_overflow`, `conflicting_usage_total`, and `aggregate_usage_overflow` diagnostics retain relative file and physical line references. They omit raw values and bodies. Invalid or overflowing totals are null while independent valid components survive. Evidence retains the reported explicit total separately from the accepted total.

Detail evidence contains at most 16 contributions. `contributorCount` counts every accepted inclusive contribution, and `omittedCount` reports those absent from the evidence sample. Rows omit evidence lists. Overview is independent of filtering, paging, collapse, and child attachment. All these additions retain the existing 1 MiB serialized response limit.

Legacy detail `usage` and overview `usage` keep their local numeric compatibility representation for valid input. Their numbers can round in JavaScript above its safe integer range. Exact presentation uses the new decimal counters. Cost remains independent and nullable. Generation `contextInputTokens` remains local to input, cache-read, and cache-write evidence. Inclusive usage does not describe a single model context and cannot substitute for it.

## Errors and request policy

API errors are JSON `{error: StructuralError}`. Codes and messages are structural and do not contain record bodies or decoder snippets.

| HTTP status | Codes |
| --- | --- |
| 400 | `invalid_query`, `invalid_path`, `query_limit`, `cursor_limit`, `invalid_cursor`, `retry_payload`, `control_payload` |
| 403 | `host_denied`, `origin_denied` |
| 404 | `unknown_snapshot`, `unknown_node`, `unknown_span`, `unknown_api_route` |
| 405 | `method_not_allowed` |
| 409 | `workspace_unready`, `no_source`, `retry_conflict`, `mode_conflict`, `live_stopped`, `live_unavailable`, `workspace_closing` |
| 413 | `view_limit` |
| 503 | `busy` |

A failed workspace contains the import error, such as `source_unreadable`, `source_escape`, `source_changed`, `source_cycle`, `source_name`, `source_limit`, `source_entry_limit`, `input_byte_limit`, `record_byte_limit`, or `record_count_limit`. It contains no partial snapshot.

Allowed Host values are `127.0.0.1:4317` and `localhost:4317`. Allowed Origin values are `http://127.0.0.1:4317`, `http://localhost:4317`, and `http://127.0.0.1:5173`. Read requests can omit Origin. Retry requires an allowed Origin. Present unrecognized origins, including `null`, are denied before trace access or work starts. The Vite proxy rewrites Host to the Rust target. No permissive CORS header is added.

Responses carry `Cache-Control: no-store`, including previews and errors. The server logs its fixed readiness marker and does not log trace records, query text, previews, or requested source paths. Content routes do not persist preview responses. Clients request previews only after explicit reveal.

## Contract evidence

`crates/trace-core/tests/contracts` contains actual Rust serialization of imported synthetic views. Core tests assign fixed synthetic snapshot IDs before serialization to keep examples reproducible. `crates/pi-trace/tests/contracts` contains Rust serialization of all workspace variants. The contract tests compare checked-in bytes with fresh serialization. `UPDATE_CONTRACTS=1 cargo test --workspace` regenerates the examples.

`cargo test --workspace` covers exact and exceeded custom import limits, source changes, escaping links, occurrence reuse, child relationships, usage, timing, content bounds, pagination, response budgets, retry races, and request boundaries. It does not establish browser usability or installed-package acceptance. The package excludes these fixtures through crate-local `.npmignore` files.


## System session API

Explicit `pi-trace system` selects native provider stores at startup. The installed launcher and Pi extension expose the same mode. Existing collector routes and their `{file,line}` provenance retain their contracts. In system mode, `GET /api/v1/workspace` returns `{"state":"system"}` for mode discovery. Health remains process readiness.

| Method and route | Result |
| --- | --- |
| `GET /api/v1/system` | Effective store configuration, coverage, limits and structural workload counters |
| `GET /api/v1/sessions` | One revision-qualified catalog page |
| `GET /api/v1/sessions/{session}/workspace` | Request or reuse bounded analysis for a known catalog session |
| `GET /api/v1/sessions/{session}/snapshots/{snapshot}/overview` | Native overview |
| `GET /api/v1/sessions/{session}/snapshots/{snapshot}/observations` | Native observation page |
| `GET /api/v1/sessions/{session}/snapshots/{snapshot}/nodes/{node}` | Native node detail |
| `GET /api/v1/sessions/{session}/snapshots/{snapshot}/spans/{span}` | Native detail selected by stable run-scoped span |
| `GET /api/v1/sessions/{session}/snapshots/{snapshot}/nodes/{node}/content` | Deliberately requested bounded content |
| `GET /api/v1/sessions/{session}/snapshots/{snapshot}/diagnostics` | Native diagnostic page |
| `POST /api/v1/sessions/{session}/retry` | Retry that known session's configured source, returning `202` |

All identifiers are opaque. The service resolves session IDs through the configured catalog; request strings never become filesystem paths. Snapshot routes require the requested retained snapshot and never substitute a newer one. Existing global retry, pause and resume controls return a mode conflict in system mode.

Catalog queries accept only `provider`, `project`, `cursor` and `limit`. Providers are `codex`, `pi`, `claude` and `opencode`. Project matching is exact supported metadata. Page size is 1 through 100, with 100 as the default. Duplicate or unsupported fields fail. The encoded query limit is 16 KiB. Cursors bind catalog revision, position and filters. Observation and diagnostic routes retain the shared snapshot query rules above. Other system routes accept no query or request body.

### Catalog and session shapes

Nullable properties are present. `Coverage` below is the shared evidence-coverage enum from the existing API. `CatalogCoverage` describes discovery separately.

```ts
type Provider = 'codex' | 'pi' | 'claude' | 'opencode';
type NativeError = { code: string; message: string; source: NativeSource | null };
type NativeCapabilities = {
	profile: string;
	messages: Coverage;
	usage: Coverage;
	executionParents: Coverage;
	timing: Coverage;
	persistence: 'persistedRecordsOnly';
};
type NativeRelationship = {
	kind: string;
	target: string;
	source: NativeSource;
	execution: boolean;
};
type CatalogCoverage = {
	state: 'complete' | 'partial' | 'limited';
	limits: string[];
	retainedEntries: number;
	omittedCount: number | null;
};
type StoreStatus = {
	id: string;
	provider: Provider;
	root: string;
	state: 'pending' | 'indexing' | 'current' | 'partial' | 'absent' | 'unavailable';
	error: NativeError | null;
	visited: number;
	sweepComplete: boolean;
};
type CatalogItem = {
	id: string;
	provider: Provider;
	storeId: string;
	nativeId: string;
	recordingId: string;
	project: string | null;
	sourceRevision: string;
	capabilities: NativeCapabilities;
	source: NativeSource;
	relationships: NativeRelationship[];
	activityMs: number | null;
	ingestionState: 'discovered' | 'unavailable';
	lastObservedMs: number;
	processState: 'unknown';
};
type CatalogPage = {
	revision: string;
	items: CatalogItem[];
	nextCursor: string | null;
	coverage: CatalogCoverage;
};
type SessionWorkspace = {
	state: 'loading' | 'ready' | 'stale' | 'failed';
	sessionId: string;
	runId: string | null;
	snapshotId: string | null;
	revision: number;
	progress: 'pending' | 'current' | 'catchingUp';
	error: NativeError | null;
	sourceRevision: string;
	capabilities: NativeCapabilities;
	processState: 'unknown';
};
```

System status has `mode: "system"`, string `catalogRevision`, `stores: StoreStatus[]`, `disabledProviders: Provider[]`, `coverage: CatalogCoverage`, `limits` and `metrics`. Each store's `root` is its canonical startup-selected directory or exact database file. Roots and project metadata can be sensitive. They are guarded metadata, never browser source selectors.

Catalog ordering uses descending source activity, then deterministic session identity. `lastObservedMs` records when a descriptor was admitted or changed. `activityMs` describes supported source activity. Neither establishes process liveness or successful execution. Catalog entries contain no prompt-derived title, transcript preview or tool body. Catalog usage totals are not exposed; selected analysis supplies qualified usage views.

Only a clean exhaustive sweep confirms a missing session. Diagnostics or failed traversal preserve unseen known descriptors as unavailable. Valid rediscovery restores them. Coverage limit names include `catalog_entries`, `discovery_entries` and `database_connections`. A complete unrestricted sweep reports omitted count zero. Unknown omission counts remain null.

A stale workspace can have a retained snapshot with `error: null` while recovery is in progress. Consumers retain that evidence until a coherent replacement arrives or the service explicitly invalidates it. A new run invalidates old occurrence and reveal references. Within a run, the service retains the current and previous snapshot. Catalog revisions also retain the current and previous version.

### Native provenance

Native sources use four tagged variants. JSONL line numbers and byte offsets are decimal strings, not JavaScript numbers. JSON document pointers use RFC 6901. SQLite keys retain their native type.

```ts
type ByteRange = { start: string; end: string };
type NativeScope = { provider: Provider; storeId: string };
type NativeSource = NativeScope & (
	| { kind: 'jsonl'; source: string; line: string; byteRange: ByteRange | null }
	| { kind: 'compressedJsonl'; source: string; encoding: 'zstd';
		decodedLine: string; decodedByteRange: ByteRange | null }
	| { kind: 'json'; source: string; pointer: string | null }
	| { kind: 'sqlite'; database: string; schemaProfile: string; table: string;
		primaryKey: { column: string; value:
			| { kind: 'text'; value: string }
			| { kind: 'integer'; value: string }
			| { kind: 'blob'; hex: string } }[];
		field: { kind: 'column'; column: string }
			| { kind: 'json'; column: string; pointer: string } | null;
		observedRevision: string }
);
```

File and database names are relative to their configured store. Compressed provenance identifies the original archive and decoded location, never a temporary file or compressed byte offset presented as a decoded position. Plain JSON sidecars do not acquire invented JSONL lines. Directory discovery failures have nullable provenance. Store status supplies their provider and store identity without fabricating a file record.

Native overview, observations, diagnostics and detail reuse the shared application structures with `NativeSource` at every source-bearing field. Native detail adds required `nativeUsage`, `relationships`, `timingEvidence` and nullable `selectedContext`. Content retains the shared explicit-reveal shape and bounds.

```ts
type NativeCounter = { name: string; value: TokenCounter; source: NativeSource };
type NativeUsageEvidence = {
	id: string;
	role: { kind: 'contribution'; category: string }
		| { kind: 'reconciliation'; scope: string }
		| { kind: 'inherited'; origin: string }
		| { kind: 'breakdown'; includedIn: string };
	authority: string;
	accepted: boolean;
	source: NativeSource;
	nativeCounters: NativeCounter[];
	normalization: { component: string; rule: string; operands: NativeCounter[] }[];
	accounting: NativeTokenAccounting;
};
type NativeTokenAccounting = Omit<TokenAccounting, 'provenance'> & {
	provenance: TokenAccounting['provenance']
		| 'nativeResponse' | 'nativeAuxiliary' | 'derivedCumulative';
};
type NativeDetailAdditions = {
	nativeUsage: NativeUsageEvidence[];
	relationships: NativeRelationship[];
	timingEvidence: NativeTimingEvidence[];
	selectedContext: string | null;
};
type NativeTimingEvidence = {
	meaning: 'persistence' | 'requestStart' | 'resultCreation' | 'executionStart' | 'executionEnd';
	value: string;
	unit: string;
	source: NativeSource;
};
```

Exact token states and checked u64 decimal strings retain the shared meanings. Native auxiliary or derived cumulative contributions do not imply model generations or collector fallbacks. Native `contributorCount` can exceed `observedGenerations + fallbackContributions`; `observedGenerations <= totalGenerations` still holds. Collector accounting keeps its existing stricter equality. The fabricated auxiliary fixture has three accepted contributions and 30 tokens, with zero generations and zero collector fallbacks.

Authority, replacement, inherited evidence and reconciliation are Rust decisions. A breakdown or session aggregate does not become an extra contribution merely because it has token counters. Native normalization records its operands and rule, including cached-input subtraction, disjoint reasoning addition, subset explanations and comparable same-context cumulative differences. Browser code performs no provider token arithmetic. Inclusive rows remain non-additive, and no cross-session grand usage total is supplied.

### Scheduling and retained-data limits

The status `limits` object exposes the following configured defaults. Byte values are integer bytes, with MiB values shown here for readability.

| Fields | Defaults |
| --- | --- |
| `stores`, `databaseConnections` | 16 each, global |
| `catalogEntries`, `catalogRevisions`, `catalogPage` | 10,000 descriptors, two revisions, 100 items |
| `discoveryEntriesPerPass`, `discoveryBatch` | 100,000 visited entries and 512 entries |
| `analyses`, `activeDemandMs`, `idleEvictionMs` | Four entries, 2,000 ms active demand, 60,000 ms idle expiry |
| `parseJobs`, `batchBytes`, `batchRecords` | Two concurrent jobs, 1 MiB, 2,000 records |
| `sessionDecodedBytes`, `sessionRecords`, `recordBytes`, `relatedSources` | 128 MiB, 100,000 records, 1 MiB, 1,024 sources |
| `compressedBytes`, `retainedDecodedBytes` | 256 MiB compressed per session and 256 MiB global retained input equivalents |

Discovery and analysis have separate lifetimes. Unselected history receives bounded metadata discovery, not full semantic parsing on every tick. A request within two seconds keeps an analysis active. At four retained analyses, the least recently requested inactive entry is evicted immediately. If all four are active, a fifth request returns `analysis_capacity` without starting another reader. Idle entries expire after 60 seconds. Catalog descriptors outlive analysis eviction.

At catalog capacity, active investigations remain addressable and recent source activity can replace cold history. Caught-up discovery runs every second; absent roots are retried every five seconds. Bounded traversal resumes its existing position after the per-pass limit. Initial backlog rotates across stores and alternates with selected work. Active analyses follow every 250 ms when caught up, and catch-up batches continue without that ordinary delay. Transient failures back off independently, capped at five seconds.

The global retention charge is conservative. It counts decoded-input-equivalent bytes separately for each reader, current snapshot and previous snapshot. An old large snapshot remains charged after a smaller database replacement until retirement. Before I/O, each job reserves new decoded bytes and projection headroom, including concurrent reservations. The 128 MiB per-session input ceiling therefore does not guarantee admission when retained revisions consume the shared budget. This is not a process-RSS limit or a measurement of allocated heap bytes.

Metrics contain `parseJobs`, `readRecords`, `readBytes`, `discoveryBatches`, `loadedEntries`, `retainedBytes`, `activeParseJobs`, `maxActiveParseJobs` and `databaseConnections`. Reader counters report actual decoder work, including rereads after replacement. They exclude discovery probes, metadata calls, SQLite scalar queries and continuity probes, so `readBytes` is not total disk I/O. `retainedBytes` reports the conservative retention charge. Metrics contain no record bodies.

### System errors and browser continuity

Added structural errors include `unknown_session`, `unknown_snapshot`, `expired_catalog_revision`, `analysis_capacity`, `analysis_busy`, `source_unavailable` and named input or retention limits. Unknown sessions and snapshots return `404`; expired catalog revisions and busy recovery return `409`. Analysis capacity returns `503`, body `retryAfterMs: 2000` and `Retry-After: 2`. Other native error responses carry `retryAfterMs: null`. Shared middleware errors retain the existing envelope.

Added routes retain the existing Host, Origin, no-store, eight-handler and 1 MiB response policies. Session retry requires an approved Origin and an empty body. Foreign origins fail before private data or work is exposed. Read requests can omit Origin. File paths and raw bodies are not accepted in query parameters. Trace-derived labels and roots render as text.

The browser polls system status every second and selected workspace every 500 ms with separate cancellation and bounded backoff. Native requests share a per-tab serial queue with at most eight pending requests. Cancellation removes queued work, and the ten-second request timeout includes queue wait. The service retains its global eight-handler limit. Catalog pages refresh on revision or filter changes. Selection remains independent of order and page membership. Session, run, snapshot and request generation qualify every displayed frame. A new revision hides revealed bodies without automatically fetching replacement content.

System HTTP goldens are in `tests/contracts/system-*.json`; native reader and provenance goldens are in `crates/trace-core/tests/contracts/native-*.json`. Ordinary tests compare immutable bytes. `UPDATE_CONTRACTS=1` regenerates system HTTP examples, and `UPDATE_NATIVE_CONTRACTS=1` regenerates native examples. Only temporary identity and freshness values are normalized. [System-session verification](system-session-verification.md) records separate real-browser, latency and extracted-package evidence.
