# Spec Delta

## Purpose

Expose trustworthy token usage for every trace span and its verified descendants, with visible coverage and source provenance and without counting nested usage more than once.

## ADDED Requirements

### Requirement: Expose accounting on every span
Every session, interaction, turn, generation, and tool SHALL expose token accounting in execution rows and detail views. Accounting SHALL distinguish the span's own contribution from its inclusive descendant total. The existing detail usage field SHALL retain its documented local meaning.

#### Scenario: Inspect a recording with all supported span kinds
- **WHEN** the recording contains sessions, interactions, turns, generations, and tools
- **THEN** each row displays an inclusive token value or an explicit usage state without requiring selection
- **AND** selecting any row exposes own and inclusive accounting with the same snapshot identity

### Requirement: Preserve recorded token components
Accounting SHALL preserve input, output, cache-read, and cache-write separately. A contributor's total SHALL use a valid explicit total or the checked sum of all four components. Missing components SHALL not become zero. Recorded zero SHALL remain visible. Context input SHALL continue to use generation-local recorded input and cache counts.

#### Scenario: Missing cache counts
- **WHEN** a generation records input 100 and output 20 but has no cache counts or explicit total
- **THEN** its known input and output remain visible, its total remains unknown, and its breakdown is partial

#### Scenario: Recorded zero and total-only evidence
- **WHEN** one generation records four zero components and another records only an explicit total of 70
- **THEN** the first displays a known zero total
- **AND** the second retains the known total of 70 with an incomplete component breakdown

### Requirement: Give each usage contribution one identity
A generation occurrence SHALL contribute usage at most once. Reused upstream indices SHALL not merge distinct occurrences. Repeated scans, projections, generation completions, relationship updates, and browser reconnection SHALL not multiply accepted contributions. Supported duplicate-record precedence SHALL remain explicit and diagnostic.

#### Scenario: Duplicate completion after reconnect
- **WHEN** a generation completion recording 150 tokens is encountered again through scanning or a duplicate completion record
- **THEN** the accepted generation contribution remains 150 tokens and duplicate input retains its structural diagnostic

#### Scenario: A turn index is reused
- **WHEN** two distinct turn occurrences reuse the same upstream indices and each records 150 tokens
- **THEN** the recording total includes both contributions and equals 300 tokens

### Requirement: Apply summary fallback without overlapping generation usage
A turn summary SHALL contribute only when that turn has no observed generation usage. Partial, cost-only, and recognized invalid generation evidence SHALL suppress overlapping summary usage. Late generation evidence SHALL replace a previously eligible fallback in the next revision. Fallback amounts SHALL retain their source and partial state.

#### Scenario: Generation usage replaces a live fallback
- **WHEN** a summary initially contributes 150 tokens and a later generation completion for that turn records 120 tokens
- **THEN** the next revision contributes 120 tokens rather than 270
- **AND** the interface shows the changed provenance without requiring totals to increase monotonically

#### Scenario: Partial generation usage accompanies a complete summary
- **WHEN** a generation records only output 20 and the turn summary records a complete total of 150
- **THEN** the turn uses only the partial generation evidence and does not fill its missing fields from the summary

#### Scenario: A summary accompanies invalid generation usage
- **WHEN** a generation records an invalid token value and the turn summary contains a valid total
- **THEN** the invalid generation evidence remains visible with a diagnostic and the summary does not hide it through fallback

### Requirement: Roll up only verified child execution
Inclusive accounting SHALL include each uniquely linked descendant contribution once. A tool SHALL receive child model usage only through supported explicit, unique, cycle-free relationships. A missing or ambiguous child relationship SHALL remain unattributed and visibly incomplete rather than receiving a guessed amount.

#### Scenario: Linked subagent usage
- **WHEN** a parent generation records 150 tokens and its tool uniquely links a child session recording 40 tokens
- **THEN** the tool's inclusive total is 40 and the containing generation, turn, and session each show an inclusive total of 190
- **AND** the parent generation's own contribution remains 150

#### Scenario: A child cannot be attributed
- **WHEN** a tool declares a child identifier with multiple candidate recordings
- **THEN** the tool reports unresolved attribution instead of selecting a child by timestamp or completion order
- **AND** the candidate recordings remain independently inspectable

### Requirement: Count overview usage independently of tree totals
Recording overview usage SHALL sum canonical contributions across all imported sources once, including sources whose parent is unresolved. Inclusive row totals SHALL be labelled as nested, non-additive values. Filtering, collapse, pagination, and child attachment SHALL not change the recording total.

#### Scenario: An existing child becomes linked
- **WHEN** an already imported 40-token child becomes linked beneath a 150-token parent
- **THEN** the recording total remains 190 while the parent's inclusive total changes from 150 to 190
- **AND** the interface does not sum the displayed parent and child totals into 230

### Requirement: Distinguish coverage from applicability and attribution
Accounting SHALL distinguish exact values, known partial subtotals, absent evidence, invalid values, and non-applicable own usage. Measurement coverage and child attribution SHALL remain independently visible. A container without recorded contributors SHALL not imply a known zero merely because it has no completed children.

#### Scenario: Ordinary tool without recorded model usage
- **WHEN** an ordinary tool has arguments and a result but no supported direct model contribution or child model execution
- **THEN** its own and inclusive accounting are marked not applicable with an explanation of the recording format
- **AND** its argument or result length is not displayed as tokens

#### Scenario: Some generation usage is missing
- **WHEN** one of two generation occurrences records 150 total tokens and the other records no usage
- **THEN** the containing span exposes 150 as a known partial subtotal with coverage of one observed generation out of two
- **AND** it does not describe 150 as the complete total

### Requirement: Retain bounded usage provenance
Span details SHALL identify recorded, turn-summary fallback, and aggregate accounting. Source evidence SHALL retain relative file and physical line references. Contributor evidence SHALL be bounded with explicit contributor and omitted counts. Row metadata SHALL omit record bodies and unbounded contributor lists.

#### Scenario: A large parent aggregates many child generations
- **WHEN** a parent includes more contributions than fit its detail evidence limit
- **THEN** its aggregate remains calculated from all accepted contributions
- **AND** the detail reports its bounded source evidence and omitted count without presenting that evidence list as complete

### Requirement: Preserve numeric correctness
New accounting values SHALL preserve supported integer precision between Rust, JSON, and the browser. Invalid token values, checked-addition overflow, and disagreement between a complete component sum and an explicit total SHALL produce source diagnostics. An affected counter SHALL not be presented as exact, while independent valid component evidence SHALL remain inspectable.

#### Scenario: An integer exceeds JavaScript's safe-number range
- **WHEN** a supported recorded token count exceeds the maximum exactly representable JavaScript Number but fits the supported unsigned integer range
- **THEN** its new accounting view and browser label retain the exact integer without rounding

#### Scenario: Components conflict with an explicit total
- **WHEN** four recorded components sum to 100 but the same contribution records an explicit total of 90
- **THEN** accounting reports the inconsistent total with its source diagnostic and retains both pieces of numeric evidence
- **AND** neither the span nor the overview presents the affected total as exact

#### Scenario: Aggregation overflows
- **WHEN** individually valid recorded values exceed the supported integer range when combined
- **THEN** the affected aggregate counter is invalid and has no wrapped or exact numeric total
- **AND** other independently valid component counters remain available

### Requirement: Keep token accounting within recorded evidence
Token accounting SHALL not estimate tokens from characters, bytes, preview lengths, or catalogue sizes. It SHALL not distribute model usage among tools, skills, prompt sections, or tool results without supported recorded attribution. Absent cost SHALL remain unknown independently of token coverage.

#### Scenario: A large tool result enters a later model request
- **WHEN** a tool result has a recorded serialized length and a later generation reports input usage
- **THEN** the result length remains in its documented unit and the generation retains its recorded input count
- **AND** the interface does not assign some of those input tokens to the earlier tool result

### Requirement: Present usage consistently during live updates
Rows, inspector accounting, and overview SHALL describe one revision. Partial, fallback, invalid, unknown, and not-applicable states SHALL have visible text and accessible labels. A change in provenance or coverage SHALL remain visible when live evidence replaces a fallback or resolves a child link.

#### Scenario: A selected parent changes attribution
- **WHEN** a live revision resolves an explicit child link for a selected parent
- **THEN** its execution row and inspector update to matching inclusive totals and attribution state
- **AND** the overview retains its single-count total without resetting selection or fetching hidden bodies
