# Spec Delta

## Purpose

Read existing Pi trace recordings into a consistent local representation while keeping incomplete input and uncertain correlations visible.

## ADDED Requirements

### Requirement: Read upstream trace files
The system SHALL import a selected local trace directory containing `events.jsonl` and optional `subagents/<child-id>/events.jsonl` files, using the documented upstream compatibility baseline.

#### Scenario: Root and child recordings
- **WHEN** a directory contains a root trace and two child traces
- **THEN** all three sources are imported with their relative file names and line provenance

#### Scenario: Missing root recording
- **WHEN** the selected directory has no readable root `events.jsonl`
- **THEN** import fails with a readable source error and does not expose a partial successful result

### Requirement: Preserve session and attempt identity
The system SHALL keep repeated session starts and repeated turn indices as separate occurrences, preserving source order and preventing correlation across session epochs or turn attempts.

#### Scenario: Restarted indices
- **WHEN** a new `session_start` is followed by indices already used in the same file
- **THEN** earlier events remain distinct and tool events are not linked across that restart

#### Scenario: Retried turn
- **WHEN** another turn start reuses a turn index in one epoch
- **THEN** both attempts remain separately inspectable

### Requirement: Diagnose recoverable input problems
The system SHALL report file and line diagnostics for malformed middle records, an incomplete final record, unknown event types and missing end events. Valid surrounding records SHALL remain available. Unknown types SHALL not terminate import.

#### Scenario: Malformed middle record
- **WHEN** a malformed record occurs between valid records
- **THEN** valid records on both sides remain available and the malformed line is identified

#### Scenario: Interrupted final write
- **WHEN** the final line is an incomplete JSON record
- **THEN** it is identified as an incomplete tail separately from malformed middle records

#### Scenario: Forward-compatible event
- **WHEN** a syntactically valid record contains an unknown type
- **THEN** import continues and reports the unsupported event with source provenance

### Requirement: Bound imports and source access
The system SHALL reject inputs exceeding documented limits with an explicit error. Import SHALL read only the selected trace directory, refuse symlinks escaping it, and avoid writing into the source directory.

#### Scenario: Oversized record
- **WHEN** a source record exceeds the configured record limit
- **THEN** import reports the limit and source location without consuming an unbounded record

#### Scenario: Escaping child link
- **WHEN** a child trace path resolves outside the selected directory
- **THEN** import refuses that source and reports the path violation

### Requirement: Correlate tools and child traces conservatively
The system SHALL match tool events by `toolCallId` within their occurrence and prefer explicit `childTraceId` links. Unresolved or ambiguous links SHALL remain visible rather than being presented as exact parentage.

#### Scenario: Parallel children finish in reverse order
- **WHEN** child traces complete in a different order from their tool calls and explicit child IDs exist
- **THEN** each child is linked to the tool with its matching ID

#### Scenario: No reliable child identifier
- **WHEN** two possible children overlap and no explicit link distinguishes them
- **THEN** neither receives a falsely certain parent link and the ambiguity is reported
