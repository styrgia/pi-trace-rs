# Spec Delta

## Purpose

Let users investigate a local recording in a readable report whose hierarchy, timeline, details, and diagnostics describe the same Rust-analyzed snapshot.

## ADDED Requirements

### Requirement: Present a readable execution report
The workbench SHALL show a compact session summary, one hierarchy-aligned waterfall or list, and an inspector. Secondary session metadata SHALL be available by disclosure. Durations SHALL appear once per execution row, and controls for synthetic preview states SHALL remain outside the application.

#### Scenario: Inspect the reference recording on a desktop
- **WHEN** the synthetic reference recording is open at 1440 by 900 pixels with the default inspector disclosure closed
- **THEN** its fourteen observations, their durations, and the selected observation's details are readable together without a duplicate timeline overview

#### Scenario: Inspect secondary session information
- **WHEN** the user opens session information
- **THEN** the available source reference, model, token breakdown, and recording metadata appear without replacing the execution view
- **AND** missing values are identified as unknown rather than filled from the HTML example

### Requirement: Render Rust measurements without changing their meaning
The workbench SHALL present supplied timing provenance, measurement meaning, usage coverage, outcomes, and uncertain relationships. It SHALL not derive domain measurements or causal links from upstream records, row order, labels, or matching tool names.

#### Scenario: Missing or invalid timing
- **WHEN** an observation has unknown timing or an invalid derived interval
- **THEN** it remains selectable with that timing state visible and has no invented waterfall interval

#### Scenario: Failed tool followed by successful completion
- **WHEN** Rust reports a tool failure and a successful final interaction outcome without an established retry relationship
- **THEN** both facts remain visible and the inspector does not invent a successful-retry link

#### Scenario: Partial usage and unavailable first-token time
- **WHEN** Rust reports incomplete generation usage, known response-header latency, and unknown first-token time
- **THEN** the report labels the usage as incomplete and keeps header latency separate from unavailable first-token time

### Requirement: Keep one selection across execution views
Selecting an observation SHALL update its hierarchy row, waterfall or list row, and inspector consistently. Changing view, collapsing a branch, or applying a filter SHALL preserve independently available selected details. A selection outside the visible results SHALL have a visible explanation.

#### Scenario: Switch the selected failed tool between views
- **WHEN** the user selects a failed tool and switches from waterfall to list
- **THEN** the same observation remains selected with the same source reference, timing, outcome, and inspector

#### Scenario: A filter hides the selected observation
- **WHEN** the selected successful generation is outside an error filter or a collapsed branch
- **THEN** its details remain available and a notice explains why its row is outside the visible results

### Requirement: Search and filter with complete context
The workbench SHALL support bounded metadata search and error, tool, and longer-than-five-second filters using server query results. Matching observations SHALL retain ancestor context. Counts SHALL distinguish matches, context, and loaded rows. A bounded page SHALL expose an explicit continuation rather than appear complete.

#### Scenario: Find a tool in a later page
- **WHEN** a metadata search has results beyond the current page
- **THEN** the report identifies remaining results and the user can load them without losing the existing rows, ancestor context, or selection

#### Scenario: Filter an observation with no comparable duration
- **WHEN** a slow-call filter is applied to a recording containing unknown or invalid durations
- **THEN** those observations are not classified as longer than five seconds and the filter does not substitute a duration

### Requirement: Reveal recorded bodies deliberately
Prompts, arguments, and result bodies SHALL remain hidden and unfetched until the user reveals the selected observation. One reveal action SHALL request its bounded input and output previews. The report SHALL render them as text, identify unavailable and truncated fields, and discard revealed content when the snapshot changes.

#### Scenario: Open metadata without revealing bodies
- **WHEN** the user selects an observation, opens its metadata, or filters the report
- **THEN** no content-preview request occurs and no prompt, argument, or result body appears

#### Scenario: Reveal markup and long output
- **WHEN** the user reveals an observation containing markup and output exceeding the preview limit
- **THEN** the markup appears as inert text, no external request or script execution occurs, and the truncated preview states its limit

### Requirement: Distinguish service and recording states
The workbench SHALL distinguish no source selected, importing, a ready empty recording, an import failure, no query matches, and an unavailable service. It SHALL not display example metrics as fallback data. Failed imports SHALL offer retry of the configured source; connection failures SHALL offer reconnection.

#### Scenario: Start without a source
- **WHEN** the service starts without a recording directory
- **THEN** the report explains how to select a source and shows no fabricated observations or measurements

#### Scenario: Repair and retry the configured source
- **WHEN** the configured root recording is unreadable or missing, the user repairs that source, and activates Retry import
- **THEN** the workbench shows the new import attempt and displays its successful snapshot when complete
- **AND** the action does not select another filesystem path

#### Scenario: Lose and restore the backend
- **WHEN** the backend becomes unavailable and is subsequently restarted
- **THEN** Reconnect requests its current state and either restores the matching selection or explains that the previous snapshot is no longer available

### Requirement: Preserve safe view state in local links
Local view links SHALL preserve snapshot identity, selection, structural filter, execution view, inspector tab, and theme. Search text, filesystem paths, and recorded bodies SHALL not enter these links. Revealed content SHALL not persist across reloads. A stale snapshot link SHALL not silently select a node in another snapshot.

#### Scenario: Copy and reopen a view
- **WHEN** the user copies a filtered list view and opens its link while that snapshot is available
- **THEN** the same selection, structural filter, view, tab, and theme are restored with recorded bodies hidden
- **AND** search text and source paths are absent from the link

#### Scenario: Open a stale snapshot link
- **WHEN** the link names a snapshot from a previous process or import attempt
- **THEN** the report explains the stale selection and offers the current workspace without assigning the old node ID to a different recording

### Requirement: Keep diagnostics actionable and accessible
The report SHALL expose source-relative file and line references for observations and diagnostics. Navigation, filters, disclosures, continuation, and recovery actions SHALL be reachable by keyboard with visible focus. Light and dark themes and stacked narrow layouts SHALL preserve content and meaning without page-level horizontal overflow.

#### Scenario: Investigate a diagnostic by keyboard
- **WHEN** the user opens diagnostics and follows an available observation reference using the keyboard
- **THEN** the corresponding observation opens with visible focus and its original file and line reference
- **AND** a diagnostic with no corresponding observation remains readable without a fabricated target

#### Scenario: Investigate on a narrow screen
- **WHEN** the report is used at 375 pixels wide in either theme
- **THEN** execution and details remain usable in a stacked layout, controls remain reachable, and the page does not scroll horizontally
