# Spec Delta

## Purpose

Let users investigate imported traces through a local browser interface with consistent navigation, visible diagnostics and deliberate access to recorded content.

## ADDED Requirements

### Requirement: Inspect one selection across views
The system SHALL provide a hierarchy, timeline and details panel for an imported snapshot. Selecting a node in either navigation view SHALL update the same selection and details, including its timing meaning, usage availability and diagnostics.

#### Scenario: Select a failed tool
- **WHEN** the user selects a failed tool on the timeline
- **THEN** the hierarchy identifies that tool and the details show its error and available duration

### Requirement: Search and filter without losing context
The system SHALL support search over bounded display text and filters for tools, errors and slow calls. A selected node hidden by a filter SHALL retain its details with an indication that it is outside the current results.

#### Scenario: Filter hides selection
- **WHEN** an error filter hides the currently selected successful generation
- **THEN** its details remain visible and the user is informed that the selection is outside the filter

### Requirement: Keep recorded content deliberate and inert
The system SHALL hide prompts, arguments and result bodies by default, reveal bounded previews only on an explicit user action and render them as text. The application SHALL make no external requests and SHALL not claim automatic complete sanitization.

#### Scenario: Markup in tool output
- **WHEN** a user reveals a result containing HTML or a script closing tag
- **THEN** it appears as inert text and does not execute or issue network requests

#### Scenario: Long recorded content
- **WHEN** a revealed field exceeds the preview limit
- **THEN** the preview states that it is truncated and shows the limit

### Requirement: Restrict local trace access
The system SHALL bind the trace service to loopback, restrict accepted Host and Origin values to documented local origins, omit permissive CORS, and expose only the trace source selected at process startup. It SHALL provide no arbitrary-path browsing endpoint.

#### Scenario: Unrelated web origin
- **WHEN** a request for trace data originates from an unrelated web origin or uses an unrecognized Host
- **THEN** the service refuses to return trace content

### Requirement: Support keyboard and narrow-window use
The system SHALL make navigation, selection, filters and content reveal reachable with a keyboard, show visible focus, announce loading and errors, and keep details usable when panels stack in a narrow window.

#### Scenario: Keyboard investigation
- **WHEN** a user navigates to a tool and opens its details using only the keyboard
- **THEN** focus remains visible and the tool details are accessible without a pointer

### Requirement: Present empty and failed loads honestly
The system SHALL distinguish no trace selected, loading, import failure and an imported empty trace. A failed request SHALL show a useful recovery action and SHALL not replace unavailable trace data with fabricated results.

#### Scenario: Backend unavailable
- **WHEN** the interface cannot reach the local service
- **THEN** it shows a connection error and retry action without invented trace metrics
