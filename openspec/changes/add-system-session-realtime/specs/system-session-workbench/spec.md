# Spec Delta

## Purpose

Give users one local session browser and coherent live investigation of native agent sessions without manual file selection or accidental content disclosure.

## ADDED Requirements

### Requirement: Select system mode explicitly
The installed CLI, native executable, and Pi extension SHALL support an explicit `system` command. Existing no-source, `serve`, and collector `watch` behavior SHALL retain their meanings. Launcher reuse SHALL include mode and effective provider-store configuration. Configuration conflicts SHALL NOT stop or replace another owned process implicitly.

#### Scenario: Start from an unrelated working directory
- **WHEN** the extracted package starts system mode with a provider-root override containing spaces
- **THEN** source configuration resolves against the caller's directory and packaged assets resolve from the package
- **AND** no source checkout or provider executable is required

#### Scenario: Another mode already owns the service
- **WHEN** a launcher requests system mode while its existing owned process serves a collector recording
- **THEN** the launcher reports an explicit configuration conflict without reusing or terminating that process

### Requirement: Browse and select sessions through application views
The service SHALL expose bounded system status, catalog pages, and session-qualified analysis status and snapshot views. The workbench SHALL show provider, project, freshness, coverage, and failures and allow session selection. Selecting an opaque catalog identifier SHALL NOT grant access to an arbitrary source path.

#### Scenario: Switch between providers
- **WHEN** the user selects a Codex session and then a Pi Agent session
- **THEN** the workbench loads each session's Rust application views and shows its provider and evidence availability
- **AND** late responses from the Codex selection cannot replace the selected Pi Agent session

### Requirement: Update the selected session coherently
Persisted supported changes SHALL refresh the selected session without navigation or reload. Every displayed overview, row set, detail, and diagnostic set SHALL belong to the same session, run, and immutable snapshot. Expired snapshots SHALL return a structural error without substituting newer evidence.

#### Scenario: Usage changes during detail loading
- **WHEN** a selected native message gains settled usage while the browser is loading rows and detail
- **THEN** the browser publishes one validated revision with coherent row and detail values
- **AND** responses from different sessions or revisions are discarded

### Requirement: Preserve valid investigation state
Refresh within a session run SHALL preserve stable span selection, filters, expansion, inspector tab, and usable keyboard focus. Catalog refresh SHALL preserve the selected session independently of list order. Run replacement, eviction, and source removal SHALL invalidate unavailable references explicitly. Revealed bodies SHALL remain scoped to their exact evidence revision.

#### Scenario: Another session becomes most recently updated
- **WHEN** catalog order changes while a span is selected in a different session
- **THEN** the current session, span, filters, and keyboard focus remain selected
- **AND** the catalog does not switch the investigation automatically

#### Scenario: A native file rewrite replaces the selected run
- **WHEN** recovery publishes a new analysis run for the selected catalog session
- **THEN** the interface explains the reset and invalidates old span and reveal references
- **AND** it never requests replacement bodies automatically

### Requirement: Disclose native persistence and coverage limits
The workbench SHALL distinguish following persisted records from live capture. It SHALL show pending, loading, current, catching-up, stale, unavailable, unsupported, and limited states when applicable. A provider that saves only completed messages SHALL NOT appear to supply unrecorded streaming tokens or exact in-flight timing.

#### Scenario: A model streams before its message is saved
- **WHEN** a provider is generating but has not persisted a new supported record
- **THEN** the workbench retains its last observed evidence and identifies disk-backed freshness
- **AND** it does not synthesize token deltas, process status, or a running duration

### Requirement: Preserve local request and content boundaries
All added routes SHALL retain loopback binding, approved Host and Origin rules, no-store responses, bounded errors, and explicit body reveal. Mutating requests SHALL require an approved Origin. Trace-derived labels SHALL render as text. System-mode URLs and logs SHALL omit bodies and arbitrary filesystem selectors.

#### Scenario: A foreign origin requests a session
- **WHEN** an unapproved origin requests the catalog, a snapshot, content, or recovery
- **THEN** the service denies access before returning private session data or changing work

#### Scenario: A transcript contains markup and a network URL
- **WHEN** a selected session contains fabricated markup and URLs in labels or revealed bodies
- **THEN** the workbench renders text with semantic controls and visible keyboard focus
- **AND** content neither executes markup nor triggers an external request

### Requirement: Meet declared discovery and refresh latency
Under the documented local synthetic acceptance workload, a newly persisted session SHALL appear within five seconds and a complete selected-session update SHALL appear coherently within two seconds. Measurements SHALL start at a complete JSONL record or committed database transaction and identify the machine, workload, and coverage limits.

#### Scenario: Four providers update together
- **WHEN** all four providers create sessions and the selected provider commits supported updates during the declared workload
- **THEN** catalog appearance and selected row-and-inspector refresh meet their respective budgets
- **AND** the evidence reports actual observations separately from proposed budgets

### Requirement: Start and stop the installed package independently
The npm package SHALL include the web assets, native executable, and required native storage libraries for its supported bundled platform. System mode SHALL start outside the source checkout, follow supported synthetic stores, and stop owned workers without changing source data. Package startup SHALL NOT depend on build-tool or provider-daemon fallback.

#### Scenario: Run the extracted package without the checkout
- **WHEN** the package is extracted into a separate directory and launched against synthetic stores from another cwd
- **THEN** all four providers are discoverable and selected-session updates are visible
- **AND** shutdown leaves no owned worker or source mutation
