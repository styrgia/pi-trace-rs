# Spec Delta

## Purpose

Expose bounded, local application views of a recording interpreted by Rust so the browser can investigate a snapshot without implementing trace semantics or reading arbitrary files.

## ADDED Requirements

### Requirement: Restrict imports to the startup-selected source
The service SHALL accept one recording directory through an explicit startup argument and expose no HTTP path-selection or file-browsing operation. Relative paths SHALL resolve against the caller's working directory before the launcher changes its working directory. No-argument startup SHALL remain supported as an empty workspace.

#### Scenario: Start from another working directory
- **WHEN** the installed launcher receives a relative recording path containing spaces from outside its package directory
- **THEN** Rust imports that caller-selected recording while application assets still resolve from the installed package

#### Scenario: Attempt to select a source through HTTP
- **WHEN** a client adds a source-selection parameter to a read query or a filesystem-path payload to a retry request
- **THEN** the service rejects the unsupported input and does not read that path

### Requirement: Expose recording state independently from service readiness
The API SHALL distinguish empty, importing, ready, and failed recording states from process health. Import attempts SHALL have distinct identifiers. Only a completed import SHALL publish an immutable snapshot. Recoverable record diagnostics SHALL accompany a ready snapshot, while fatal import errors SHALL expose no partial successful snapshot.

#### Scenario: Service is healthy while import fails
- **WHEN** the listener is available but the selected recording fails to import
- **THEN** health still describes the running service and workspace state reports a failed import with a bounded structural error
- **AND** no success metrics or partial snapshot are returned

#### Scenario: Import a valid empty recording
- **WHEN** an existing readable root recording contains no observations
- **THEN** workspace state is ready with an empty snapshot rather than no source selected

### Requirement: Retry a failed import without changing its source
An explicit retry operation SHALL be accepted only from the failed state and SHALL import the same configured source with the same limits and path checks. It SHALL transition atomically to a new importing attempt and allow at most one import worker. Retry while importing or ready SHALL return a conflict; retry without a source SHALL return a no-source error.

#### Scenario: Concurrent retry requests
- **WHEN** two clients retry the same failed import concurrently
- **THEN** exactly one request starts an attempt and the other receives an explicit conflict
- **AND** the service does not queue or run a second import

#### Scenario: Repair the selected root recording
- **WHEN** a failed source is repaired and retried successfully
- **THEN** the completed attempt publishes a fresh snapshot and its workspace status supersedes the failed attempt

### Requirement: Bind all views to one immutable snapshot
Overview, observation, detail, content, and diagnostic responses SHALL identify their snapshot. Node identifiers SHALL distinguish occurrences and be meaningful only within that snapshot. Requests for an unavailable snapshot or unknown node SHALL return explicit structured errors rather than unrelated data or the application HTML shell.

#### Scenario: Reuse a view after restart
- **WHEN** a request names a snapshot from a previous service process
- **THEN** the API identifies it as unavailable and does not resolve its node IDs against the current snapshot

#### Scenario: Request an unknown node or API route
- **WHEN** a client requests an unknown node or an unsupported API route
- **THEN** the response is a structured API error and does not return a successful HTML response

### Requirement: Return bounded ordered queries with ancestor context
Observation queries SHALL accept bounded metadata search, error/tool/slow filters, and snapshot-bound continuation. Results SHALL preserve deterministic hierarchy order and ancestor context, identify matches separately from context rows, and report remaining results. Pages SHALL obey documented row and serialized-byte limits without silently dropping rows.

#### Scenario: Ancestors exceed one page
- **WHEN** the context needed for a match extends beyond the first bounded page
- **THEN** ancestors precede descendants across appendable pages and continuation makes every result reachable without treating the first page as complete

#### Scenario: Query exceeds a limit or uses an incompatible cursor
- **WHEN** the query text, page size, or cursor exceeds its documented limit, or a cursor belongs to another snapshot or query
- **THEN** the service returns a specific invalid-query or limit error without running an unbounded request

#### Scenario: Select a node outside the filtered page
- **WHEN** a valid snapshot node is absent from the current filtered page
- **THEN** its detail remains independently retrievable by snapshot and node ID

### Requirement: Supply trustworthy semantic application views
Rust SHALL provide observation identity, known or unresolved relationships, source-relative file and line references, outcomes, timing meaning and validity, and usage coverage. Those values SHALL satisfy the active bootstrap trace-import and trace-analysis requirements. The API SHALL not substitute the prototype's example totals, parents, durations, or recovery links.

#### Scenario: Import repeated indices and ambiguous children
- **WHEN** a recording repeats session or turn indices and contains child traces with ambiguous parentage
- **THEN** responses retain distinct occurrences and unresolved relationships with their source diagnostics rather than overwriting or inventing links

#### Scenario: Return distinct timing and usage meanings
- **WHEN** response-header and completed-generation durations differ and a turn summary repeats generation token usage
- **THEN** views expose the distinct timing meanings and a total that counts usage once
- **AND** absent cost and first-token measurements remain unknown

### Requirement: Keep bodies outside metadata and diagnostics
Normal view responses SHALL omit prompt, argument, and result bodies, including labels copied from hidden arguments. Diagnostics SHALL include structural error categories and source references without raw record snippets or decoder messages containing bodies. Bounded display metadata SHALL not be advertised as sanitized content.

#### Scenario: Hidden argument contains a command or credential
- **WHEN** an argument body contains a command, private path, or fabricated credential
- **THEN** overview, row, detail metadata, and diagnostic responses do not disclose that body through a convenience label or error message

#### Scenario: Malformed input contains private text
- **WHEN** a malformed middle record or incomplete final record contains fabricated private text
- **THEN** the API reports the file, line, and distinct problem category without echoing the record

### Requirement: Bound explicit content previews
An explicit content request SHALL return only the requested node's input and output previews with availability, UTF-8 byte limits, and truncation indicators. Each field SHALL be limited to 8 KiB. Trace responses SHALL prevent HTTP caching, and no metadata request or query SHALL implicitly fetch or include these previews.

#### Scenario: Preview exceeds the byte limit
- **WHEN** a recorded field exceeds 8 KiB or the boundary falls inside a multibyte character
- **THEN** its preview remains valid text within the byte limit and is marked truncated

#### Scenario: Content was not recorded
- **WHEN** a node has no recorded input or output field
- **THEN** the response identifies that field as unavailable rather than inventing empty successful content

### Requirement: Enforce local request boundaries
The service SHALL bind to loopback, validate documented Host and Origin values before returning trace data, and omit permissive CORS. Read requests without Origin SHALL follow the documented local-client policy. Retry SHALL require an allowed Origin. Trace-derived text SHALL never trigger external requests or be copied into unbounded logs.

#### Scenario: Unrelated origin or Host
- **WHEN** a request uses an unrecognized Host or unrelated Origin
- **THEN** the service denies trace access and retry before returning recording data or starting an import

#### Scenario: Approved development proxy and local client
- **WHEN** the configured Vite proxy forwards an approved local browser request, or a local read client sends an allowed Host without Origin
- **THEN** the API serves the bounded view under the documented policy
- **AND** an absent-Origin retry remains denied

### Requirement: Enforce import limits before publishing views
Imports SHALL enforce the documented input-byte, record-byte, record-count, and source-count limits before unbounded decoding or traversal. Sources SHALL remain within the selected directory and SHALL not be modified. Limit failures, unreadable roots, and escaping symlinks SHALL produce failed imports with source diagnostics.

#### Scenario: Exceed input or source limits
- **WHEN** a synthetic recording exceeds a configured input, record, or source-count limit
- **THEN** import stops with the named limit and publishes no partial snapshot

#### Scenario: Resolve an escaping child recording
- **WHEN** a child path resolves outside the configured root
- **THEN** the source is refused without reading outside the root or writing into the source directory
