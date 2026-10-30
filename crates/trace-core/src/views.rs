use serde::{Deserialize, Serialize};

pub const RESPONSE_BYTES: usize = 1024 * 1024;
pub const CONTENT_BYTES: usize = 8192;
pub const DISPLAY_BYTES: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceRef {
    pub file: String,
    pub line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MeasurementState {
    Recorded,
    Derived,
    Unknown,
    Invalid,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Measurement {
    pub state: MeasurementState,
    pub value_ms: Option<f64>,
    pub meaning: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NodeKind {
    Session,
    Interaction,
    Turn,
    Generation,
    Tool,
    Message,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    Success,
    Error,
    Unknown,
    Incomplete,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RelationshipState {
    Root,
    Linked,
    Unresolved,
    Ambiguous,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Relationship {
    pub state: RelationshipState,
    pub execution_parent_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bar {
    pub offset_ms: f64,
    pub duration_ms: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Observation<S = SourceRef> {
    pub id: String,
    pub span_id: String,
    pub span_usage: SpanUsage,
    pub kind: NodeKind,
    pub label: String,
    pub label_truncated: bool,
    pub parent_id: Option<String>,
    pub depth: usize,
    pub has_children: bool,
    pub r#match: bool,
    pub context: bool,
    pub outcome: Outcome,
    pub duration: Measurement,
    pub bar: Option<Bar>,
    pub source: S,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UsageState {
    Complete,
    Partial,
    Fallback,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FallbackSource {
    TurnSummary,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub state: UsageState,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub cost: Option<f64>,
    pub observed_generations: usize,
    pub total_generations: usize,
    pub fallback_source: Option<FallbackSource>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    pub session_id: Option<String>,
    pub model: Option<String>,
    pub tool_name: Option<String>,
    pub turn_index: Option<u64>,
    pub step_index: Option<u64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeDetail<S = SourceRef> {
    pub snapshot_id: String,
    pub span_usage: SpanUsageDetail<S>,
    pub observation: Observation<S>,
    pub relationship: Relationship,
    pub request_to_headers: Measurement,
    pub ttft: Measurement,
    pub usage: Usage,
    pub metadata: Metadata,
    pub context: Option<ContextInspection<S>>,
    pub tool_inspection: Option<ToolInspection<S>>,
    pub sources: Vec<S>,
    pub diagnostic_ids: Vec<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContentState {
    Available,
    Unavailable,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentField {
    pub state: ContentState,
    pub text: Option<String>,
    pub truncated: bool,
    pub limit_bytes: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeContent {
    pub snapshot_id: String,
    pub node_id: String,
    pub input: ContentField,
    pub output: ContentField,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic<S = SourceRef> {
    pub id: String,
    pub code: String,
    pub message: String,
    pub source: S,
    pub node_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObservationPage<S = SourceRef> {
    pub snapshot_id: String,
    pub items: Vec<Observation<S>>,
    pub total_matches: usize,
    pub total_rows: usize,
    pub next_cursor: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticPage<S = SourceRef> {
    pub snapshot_id: String,
    pub items: Vec<Diagnostic<S>>,
    pub total: usize,
    pub next_cursor: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Timeline {
    pub start_ms: f64,
    pub end_ms: f64,
    pub duration_ms: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeasurementAvailability {
    pub duration: bool,
    pub request_to_headers: bool,
    pub ttft: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview<S = SourceRef> {
    pub snapshot_id: String,
    pub title: String,
    pub source: Option<S>,
    pub model: Option<String>,
    pub source_count: usize,
    pub record_count: usize,
    pub node_count: usize,
    pub session_count: usize,
    pub interaction_count: usize,
    pub turn_count: usize,
    pub generation_count: usize,
    pub tool_count: usize,
    pub diagnostic_count: usize,
    pub duration: Measurement,
    pub usage: Usage,
    pub token_accounting: TokenAccounting,
    pub timeline: Option<Timeline>,
    pub measurement_availability: MeasurementAvailability,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuralError<S = SourceRef> {
    pub code: String,
    pub message: String,
    pub source: Option<S>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse<S = SourceRef> {
    pub error: StructuralError<S>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Coverage {
    Complete,
    Partial,
    Unsupported,
    Missing,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Retention {
    Complete,
    ListsLimited,
    RetentionLimited,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RequestShape {
    Messages,
    ProviderInput,
    PlainText,
    Unsupported,
    Missing,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RequestField {
    Messages,
    Prompt,
    Input,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValueSize {
    pub compact_json_bytes: usize,
    pub text_chars: Option<usize>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContextKind {
    SystemInstructions,
    DeveloperInstructions,
    User,
    AssistantHistory,
    ToolCalls,
    ToolOutputs,
    ToolDefinitions,
    OpaqueReasoning,
    Other,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextCategory {
    pub kind: ContextKind,
    pub count: usize,
    pub size: ValueSize,
    pub text_coverage: Coverage,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextItem {
    pub ordinal: usize,
    pub kind: ContextKind,
    pub size: ValueSize,
    pub text_coverage: Coverage,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamedContributor {
    pub name: Option<String>,
    pub name_truncated: bool,
    pub ordinal: usize,
    pub size: ValueSize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextSection {
    pub id: usize,
    pub parent_id: Option<usize>,
    pub kind: String,
    pub item_ordinal: usize,
    pub text_chars: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextCounts {
    pub messages: Option<usize>,
    pub tool_definitions: Option<usize>,
    pub skill_descriptors: Option<usize>,
    pub unclassified_items: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OmittedCounts {
    pub items: Option<usize>,
    pub tools: Option<usize>,
    pub skills: Option<usize>,
    pub sections: Option<usize>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextInspection<S = SourceRef> {
    pub coverage: Coverage,
    pub retention: Retention,
    pub shape: RequestShape,
    pub request_field: Option<RequestField>,
    pub source: Option<S>,
    pub payload_size: Option<ValueSize>,
    pub counts: Option<ContextCounts>,
    pub categories: Vec<ContextCategory>,
    pub largest_items: Vec<ContextItem>,
    pub largest_tools: Vec<NamedContributor>,
    pub largest_skills: Vec<NamedContributor>,
    pub sections: Vec<ContextSection>,
    pub omitted: OmittedCounts,
    pub context_input_tokens: Option<u64>,
    pub context_input_tokens_exact: TokenCounter,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceTruncation {
    KnownTruncated,
    KnownComplete,
    Unknown,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewEvidence<S = SourceRef> {
    pub availability: ContentState,
    pub selected_size: Option<ValueSize>,
    pub preview_utf8_bytes: Option<usize>,
    pub source: Option<S>,
    pub local_truncated: bool,
    pub source_truncation: SourceTruncation,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResultField {
    ResultPreview,
    Result,
    Output,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OriginalLengthUnit {
    Utf16CodeUnitsOfSerializedResult,
    Unspecified,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OriginalLength<S = SourceRef> {
    #[serde(with = "decimal_u64")]
    pub value: u64,
    pub unit: OriginalLengthUnit,
    pub source: S,
}
mod decimal_u64 {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&value.to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultEvidence<S = SourceRef> {
    #[serde(flatten)]
    pub preview: PreviewEvidence<S>,
    pub recorded_field: Option<ResultField>,
    pub original_length: Option<OriginalLength<S>>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInspection<S = SourceRef> {
    pub retention: Retention,
    pub call_id: Option<String>,
    pub call_id_truncated: bool,
    pub call_id_source: Option<S>,
    pub arguments: PreviewEvidence<S>,
    pub result: ResultEvidence<S>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TokenState {
    Exact,
    Partial,
    Unknown,
    Invalid,
    NotApplicable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenCounter {
    pub state: TokenState,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UsageProvenance {
    None,
    RecordedGeneration,
    TurnSummaryFallback,
    Aggregate,
    Mixed,
    NativeResponse,
    NativeAuxiliary,
    DerivedCumulative,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenAccounting {
    pub input_tokens: TokenCounter,
    pub output_tokens: TokenCounter,
    pub cache_read_tokens: TokenCounter,
    pub cache_write_tokens: TokenCounter,
    pub total_tokens: TokenCounter,
    pub provenance: UsageProvenance,
    pub observed_generations: usize,
    pub total_generations: usize,
    pub fallback_contributions: usize,
    pub contributor_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChildAttribution {
    NotApplicable,
    Complete,
    Unresolved,
    Ambiguous,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpanUsage {
    pub own: TokenAccounting,
    pub inclusive: TokenAccounting,
    pub child_attribution: ChildAttribution,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageEvidenceEntry<S = SourceRef> {
    pub contribution_id: String,
    pub span_id: String,
    pub source: S,
    pub accounting: TokenAccounting,
    pub reported_total: TokenCounter,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpanUsageDetail<S = SourceRef> {
    #[serde(flatten)]
    pub summary: SpanUsage,
    pub evidence: Vec<UsageEvidenceEntry<S>>,
    pub contributor_count: usize,
    pub omitted_count: usize,
}
