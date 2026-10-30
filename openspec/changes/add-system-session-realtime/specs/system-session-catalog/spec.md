# Spec Delta

## Purpose

Let users find saved and changing local coding-agent sessions across projects through one bounded catalog with explicit coverage and freshness.

## ADDED Requirements

### Requirement: Discover all four supported providers
System mode SHALL discover saved and newly created Codex, Pi Agent, Claude Code, and OpenCode sessions within the current user's configured local stores. It SHALL include all projects in those stores. It SHALL report each provider's availability and supported-format status independently.

#### Scenario: Four providers and several projects
- **WHEN** supported synthetic stores contain historical and newly created sessions from all four providers across several projects
- **THEN** every supported session enters the catalog without manual export or a browser reload
- **AND** provider and project identity remain available for filtering

#### Scenario: A provider is absent
- **WHEN** one provider has no local store
- **THEN** the catalog reports that provider as absent and continues to discover the other providers

### Requirement: Constrain discovery to startup-selected stores
Provider roots SHALL resolve from documented defaults, supported environment overrides, or explicit startup configuration. The service SHALL disclose the effective configuration and deduplicate aliases. HTTP requests SHALL NOT select arbitrary paths. Discovery SHALL NOT traverse other user homes or escape a configured store through symlinks.

#### Scenario: Override a root with spaces
- **WHEN** startup selects a supported provider root containing spaces and two aliases resolve to the same store
- **THEN** discovery uses the resolved store once and retains one catalog identity for its sessions

#### Scenario: An escaping link or browser path is supplied
- **WHEN** a store entry escapes its configured boundary or an HTTP client submits a filesystem path
- **THEN** the service refuses that source selection without reading the target
- **AND** other configured stores remain usable

### Requirement: Preserve catalog session identity
Catalog identity SHALL distinguish provider, store namespace, and native session identity. Native IDs from different providers or independent stores SHALL NOT collide. A supported archive move, resume, or update within the same store SHALL retain session identity. Conflicting copies SHALL receive a structural diagnostic rather than a silent merge.

#### Scenario: Providers reuse a native identifier
- **WHEN** Codex and Claude Code sessions have the same native identifier
- **THEN** the catalog exposes two distinct sessions and selecting one never returns the other's evidence

#### Scenario: A session is archived or resumed
- **WHEN** a provider moves a session to its supported archive location or resumes it in the same store
- **THEN** the session retains its catalog identity and its location or revision changes explicitly

### Requirement: Expose bounded catalog pagination and completeness
The catalog SHALL expose provider and project filters, stable ordering, bounded pages, and revision-qualified cursors. Discovery SHALL disclose progress, configured limits, known omissions, and partial coverage. A limited result SHALL NOT claim that it contains all sessions. Trace loading SHALL have separate limits from catalog discovery.

#### Scenario: History exceeds the discovery limit
- **WHEN** a synthetic store exceeds a configured catalog entry or traversal limit
- **THEN** discovered sessions remain pageable and coverage reports the named limit and whether the omitted count is known
- **AND** the service does not silently label the catalog complete

#### Scenario: The catalog changes between pages
- **WHEN** new sessions change catalog order before a client uses an old cursor
- **THEN** the service returns a coherent page from the retained catalog revision or a structural expired-revision error
- **AND** the client never receives an apparently continuous page assembled from different revisions

### Requirement: Distinguish storage activity from execution state
Catalog views SHALL distinguish recorded lifecycle evidence, observed storage activity, ingestion state, and unknown process state. Recent writes, temporary EOF, inactivity, and file existence SHALL NOT establish that an agent process is running or a session is complete. Views SHALL expose last observation and source-update freshness separately.

#### Scenario: A quiet session has no completion evidence
- **WHEN** a session stops changing without a recorded completion marker
- **THEN** the catalog shows the last observed update and unknown execution completion
- **AND** it does not invent a successful, failed, or terminated process state

### Requirement: Isolate discovery and ingestion failures
An unreadable root, unsupported schema, malformed session, source discontinuity, or exhausted per-session limit SHALL affect only the relevant store or session. Last-valid data SHALL remain readable with stale status. Other providers and sessions SHALL continue to update. Recovery SHALL preserve source and store boundaries.

#### Scenario: One session is damaged
- **WHEN** a growing Claude Code session becomes unreadable while Pi Agent and Codex receive valid updates
- **THEN** that session reports its structural error and retained revision as stale
- **AND** the other sessions continue to update

#### Scenario: A missing store appears later
- **WHEN** an absent configured provider store becomes available while system mode is running
- **THEN** a subsequent discovery cycle indexes it without restarting the browser

### Requirement: Bound global work and preserve discovery fairness
System mode SHALL bound discovery work, parsing batches, concurrent session loads, retained traces, response size, and retry rate. Historical backlog and one busy provider SHALL NOT indefinitely prevent new-session discovery or selected-session refresh. Analysis eviction SHALL release analysis state without deleting a catalog session or modifying a source.

#### Scenario: A large archive and a new active session coexist
- **WHEN** bounded historical discovery is in progress and another provider creates a small supported session
- **THEN** discovery and selected-session work both make progress under the documented scheduling limits
- **AND** the new session appears within the declared acceptance workload budget

### Requirement: Admit recent sessions under a catalog bound
When catalog retention is full, the system SHALL retain currently investigated sessions and prefer recently observed source activity over cold unselected history. Omitted descriptors SHALL contribute to visible limited coverage. Rediscovery SHALL preserve their deterministic session identity. Bounded traversal SHALL resume fairly rather than repeatedly visit only the same prefix.

#### Scenario: A new session arrives at catalog capacity
- **WHEN** retained cold history fills the configured catalog bound and another provider creates a supported session
- **THEN** the new descriptor can replace an unselected cold descriptor and coverage remains explicitly limited
- **AND** the currently investigated session remains addressable without switching selection

### Requirement: Keep catalog metadata separate from bodies
Catalog responses SHALL contain bounded structural session metadata and diagnostic references. They SHALL omit prompts, transcript-derived previews, tool arguments, tool output, and reasoning text. Session and project labels SHALL render as untrusted text. Metadata SHALL NOT be described as sanitized content.

#### Scenario: A first prompt contains fabricated credentials
- **WHEN** a synthetic session's first prompt and tool output contain unique fabricated secrets
- **THEN** catalog responses, filter options, labels, diagnostics, URLs, and logs omit those bodies
- **AND** the catalog never derives a convenience title from the first prompt
