# Spec Delta

## Purpose

Let users inspect an externally recorded trace while it grows, with coherent revisions, bounded resource use, and explicit uncertainty about unfinished work.

## ADDED Requirements

### Requirement: Select realtime mode explicitly
The CLI and Pi command SHALL accept `watch <directory>` to follow the startup-selected source. Existing `serve <directory>` and no-source startup SHALL retain offline behavior. Launcher reuse SHALL require both the same resolved source and the same mode.

#### Scenario: Follow a directory from another working directory
- **WHEN** the installed CLI or Pi command receives a relative watch directory containing spaces
- **THEN** it resolves the complete directory argument against the caller's working directory and opens the packaged app in realtime mode

#### Scenario: Reuse an owned process
- **WHEN** an owned offline process is running and the same directory is requested in watch mode
- **THEN** the launcher reports a mode conflict without reusing, stopping, or replacing that process

### Requirement: Consume complete appended records once
Realtime ingestion SHALL accept append-only sources and interpret each complete newline-terminated physical record once per run. Temporary EOF SHALL retain an unfinished record without decoding it or advancing its source-line identity. Repeated scans SHALL not repeat observations or token contributions.

#### Scenario: A write splits JSON and UTF-8
- **WHEN** a producer appends a record in several writes that split a multibyte character and the final newline
- **THEN** the previous revision remains valid while the record is pending
- **AND** completing the line adds exactly one interpreted record with its original file and line

#### Scenario: Complete JSON has no line terminator yet
- **WHEN** the current file tail is valid JSON without a terminating newline
- **THEN** realtime mode keeps it pending until the newline arrives
- **AND** offline import retains its existing final-record behavior

### Requirement: Preserve malformed-record diagnostics
A malformed complete record, unsupported event, or non-object record SHALL produce its existing structural source diagnostic while surrounding valid records remain inspectable. Pending live tails SHALL not produce permanent malformed-record diagnostics. Diagnostics SHALL omit raw record text.

#### Scenario: A malformed middle line precedes valid usage
- **WHEN** a complete malformed line containing fabricated private text is followed by a valid generation completion
- **THEN** the revision includes the valid completion and a diagnostic identifying the malformed file and line
- **AND** normal views and diagnostics contain no private text from that malformed line

### Requirement: Keep open work provisional
Temporary EOF, inactivity, and a UI pause SHALL not imply recording completion. Open spans SHALL retain unknown end timing and provisional completion evidence. Recorded durations and valid timestamp-derived intervals SHALL remain distinguishable. No current-clock duration or exact TTFT SHALL be invented.

#### Scenario: A generation completes after several idle scans
- **WHEN** a generation start is followed by idle scans and then a completed generation record
- **THEN** the open generation keeps its identity and changes from provisional to its recorded completion state
- **AND** repeated projections do not accumulate missing-end diagnostics or duplicate usage

### Requirement: Discover children without inventing relationships
Realtime mode SHALL discover supported child sources within the selected root and the existing traversal limits. A late source or relationship record SHALL update attribution only through the supported explicit, unique, cycle-free link rules. Completion order SHALL not establish parentage.

#### Scenario: A child appears before its explicit parent link
- **WHEN** a child recording is readable before the parent tool records its child identifier
- **THEN** the child remains inspectable without a fabricated execution parent
- **AND** a later valid link changes the relationship without duplicating the child or its usage

### Requirement: Publish immutable coherent revisions
Every live run SHALL have a distinct identity and monotonically increasing published revisions. A revision SHALL identify one immutable snapshot shared by its overview, observations, details, content, and diagnostics. A no-op scan SHALL not publish a new revision. A rejected update SHALL not change the last valid snapshot.

#### Scenario: Data changes during a browser refresh
- **WHEN** another revision is published while a client loads several views
- **THEN** each returned view identifies its requested snapshot
- **AND** the client never combines different snapshots into one apparent current revision

#### Scenario: A requested revision has expired
- **WHEN** a client requests a snapshot outside the documented bounded retention window
- **THEN** it receives the structured unknown-snapshot error and can discover the current run and revision through workspace status
- **AND** the service does not resolve its old node or cursor against a newer snapshot

### Requirement: Preserve span identity within a run
Each span SHALL expose an opaque identity that remains stable throughout its live run despite appends, late child discovery, or relationship changes. Restart and explicit reset SHALL create a new run identity. Snapshot cursors and node references SHALL remain qualified by their snapshot.

#### Scenario: Earlier source content grows after a child is selected
- **WHEN** a child span is selected and another source receives additional occurrences with reused upstream indices
- **THEN** the next revision can resolve that same selected span without relying on its old global insertion position

### Requirement: Expose freshness separately from health
Workspace status SHALL identify realtime mode, run, latest revision, and whether following is catching up, waiting, paused, or stopped. Process health SHALL retain its existing contract. A live failure after publication SHALL preserve readable last-valid views and expose a structural error alongside their stale status.

#### Scenario: A read fails after valid revisions
- **WHEN** an active source becomes unreadable after a snapshot was published
- **THEN** the service remains healthy, following stops, and the workbench labels the retained revision as stale
- **AND** the error includes available source provenance without record bodies

### Requirement: Pause updates for investigation
An explicit pause SHALL suspend ingestion and publication for the local workspace after the current bounded update finishes. It SHALL retain the current snapshot for browsing and disclose the shared pause state to all clients. Resume SHALL validate source continuity and catch up from the last committed offsets without stopping the producer.

#### Scenario: Two browser tabs share a paused workspace
- **WHEN** one tab pauses updates and the external producer appends more records
- **THEN** both tabs report paused updates and retain the same queryable revision
- **AND** repeated pause requests do not start workers or publish revisions

#### Scenario: Resume after a bounded backlog
- **WHEN** a paused workspace resumes with valid appended records within its remaining limits
- **THEN** following catches up without duplicate usage and publishes the latest accepted revision
- **AND** the interface never describes pause as stopping trace capture

### Requirement: Stop on observable source discontinuity
Detected truncation, file replacement, same-size rewriting, source escape, or unsupported source identity SHALL stop the live run with a structural diagnostic. The system SHALL not merge a discontinuous source into the previous run. Explicit retry after a stopped run SHALL reread the same configured root under a new run identity.

#### Scenario: The root recording is replaced while paused
- **WHEN** the selected recording file has a different identity or a size below its committed offset when resume validates it
- **THEN** resume stops with a source-change error and the last valid revision remains readable
- **AND** retry creates a new run rather than combining old spans with replacement content

### Requirement: Enforce cumulative live limits
Existing input-byte, record-byte, record-count, source-count, traversal, metadata, preview, and response limits SHALL apply to the complete live run. Pending bytes and malformed or unknown records SHALL count toward their relevant limits. Idle scans SHALL not replenish budgets. At most one ingestion worker SHALL run per workspace.

#### Scenario: An unfinished record exceeds the byte limit
- **WHEN** repeated tail writes exceed the record-byte limit before a newline arrives
- **THEN** ingestion stops before retaining an over-limit record and reports the named limit
- **AND** the last valid snapshot remains unchanged

#### Scenario: Concurrent recovery requests
- **WHEN** multiple clients retry a stopped live workspace concurrently
- **THEN** one new attempt reserves the worker and the others receive an explicit conflict
- **AND** retry does not change the selected source

### Requirement: Preserve investigation state across refreshes
Automatic refresh SHALL preserve valid selection, filters, expansion, view, inspector tab, and focus within the same run. Pagination SHALL restart against the new snapshot without mixing cursors. Replaced selected-node evidence SHALL invalidate revealed content without automatically requesting bodies. A new run SHALL invalidate previous selection explicitly.

#### Scenario: Usage arrives while a span is selected
- **WHEN** a selected span gains completion and usage evidence
- **THEN** its row and inspector refresh coherently while keyboard focus and metadata filters remain usable
- **AND** previously revealed bodies return to a hidden state without an implicit content request

#### Scenario: A retained investigation spans several pages
- **WHEN** the current view has appended multiple pages and a revision changes hierarchy order
- **THEN** the client replaces those pages coherently under the new snapshot and retains the selected span independently
- **AND** it reports any retained-view or paging bound rather than silently claiming all rows are loaded

### Requirement: Apply local request and lifecycle boundaries
Live status, span lookup, and control requests SHALL retain loopback binding, approved Host and Origin rules, no-store responses, bounded errors, and explicit content reveal. Mutating controls SHALL require an approved Origin and reject unsupported payloads. Stopping the owned process SHALL cancel live work without changing source files.

#### Scenario: An unrelated origin attempts to resume
- **WHEN** a request with an unapproved Origin invokes a live control
- **THEN** the request is denied before source access or worker state changes

#### Scenario: Shut down the installed application
- **WHEN** the CLI is interrupted or the owning Pi session shuts down
- **THEN** its worker and owned service stop without modifying the recording or leaving an ingestion process behind

### Requirement: Bound follow latency under a declared workload
In a visible active workbench, complete supported appends within the documented acceptance workload SHALL become visible within two seconds. Acceptance SHALL measure producer newline flush to coherent row and inspector update. Waiting, pause, catch-up, and failure SHALL remain visible when that steady-state expectation does not apply.

#### Scenario: Append a generation completion at steady state
- **WHEN** a synthetic producer flushes one complete generation completion in the declared local acceptance workload
- **THEN** the new usage appears coherently within two seconds without navigation or reload
- **AND** the recorded evidence names the workload and machine rather than claiming a universal latency guarantee
