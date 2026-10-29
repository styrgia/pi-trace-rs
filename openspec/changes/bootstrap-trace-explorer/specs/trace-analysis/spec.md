# Spec Delta

## Purpose

Provide trustworthy timing, usage and graph views so users can investigate agent behavior without mistaking missing or partial measurements for exact results.

## ADDED Requirements

### Requirement: Expose a stable occurrence graph
The system SHALL expose sessions, interactions, turn attempts, generations and tools as independently selectable nodes. Node identifiers SHALL be unique within the imported snapshot and independent of reused upstream indices.

#### Scenario: Two tools in one generation
- **WHEN** a generation invokes two tools
- **THEN** the hierarchy retains both tools and their containing turn attempt with distinct identifiers

### Requirement: State timing provenance and meaning
The system SHALL distinguish recorded durations, timestamp-derived intervals and unknown timing. `llm_response.durationMs` SHALL be labeled request-to-headers latency. Generation duration SHALL use `step_end.durationMs` when available. Exact TTFT SHALL remain unavailable unless recorded by a supported source.

#### Scenario: Different header and generation timing
- **WHEN** a response records 120 ms and the completed step records 900 ms
- **THEN** the views show 120 ms request-to-headers and 900 ms generation duration separately

#### Scenario: Missing first-token measurement
- **WHEN** a trace has no exact first-token timing
- **THEN** the system displays TTFT as unavailable without substituting header latency

#### Scenario: Backward wall clock
- **WHEN** an end timestamp precedes its start timestamp
- **THEN** the derived interval is marked invalid with a diagnostic rather than converted to a positive duration

### Requirement: Count usage once
The system SHALL aggregate generation usage once per turn attempt. When no generation usage exists, a turn summary SHALL be used as a labeled fallback. Partially recorded usage SHALL remain visibly incomplete. Missing cost SHALL not be treated as a known zero cost.

#### Scenario: Both step and turn usage exist
- **WHEN** generation records total 150 tokens and the turn summary repeats those 150 tokens
- **THEN** the attempt total is 150 tokens rather than 300

#### Scenario: Summary-only input
- **WHEN** an attempt has a turn summary and no generation usage
- **THEN** the summary total is used and its fallback source is visible

#### Scenario: Partially observed steps
- **WHEN** only one of two completed generations contains usage
- **THEN** the total includes recorded generation usage once and is marked incomplete

### Requirement: Separate tool errors from final outcome
The system SHALL show tool errors independently from the final turn and interaction outcomes. An earlier recoverable tool error SHALL not by itself mark a completed interaction as failed. Incomplete endings SHALL remain unknown.

#### Scenario: Recovery after a failed tool
- **WHEN** a tool fails and a later generation completes the interaction successfully
- **THEN** the failed tool remains visible while the interaction outcome is successful

#### Scenario: Missing final generation
- **WHEN** the trace ends before the final generation completes
- **THEN** the interaction outcome is unknown or incomplete rather than successful
