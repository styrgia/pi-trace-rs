use crate::{ImportLimits, views};
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Codex,
    Pi,
    Claude,
    Opencode,
}
impl Provider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Pi => "pi",
            Self::Claude => "claude",
            Self::Opencode => "opencode",
        }
    }
}
impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl std::str::FromStr for Provider {
    type Err = String;
    fn from_str(v: &str) -> Result<Self, String> {
        match v {
            "codex" => Ok(Self::Codex),
            "pi" => Ok(Self::Pi),
            "claude" => Ok(Self::Claude),
            "opencode" => Ok(Self::Opencode),
            _ => Err("Unsupported provider.".into()),
        }
    }
}
macro_rules! id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub(crate) String);
        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}
id!(StoreId);
id!(CatalogSessionId);
id!(SourceRevision);
pub(crate) fn identity(parts: &[&str]) -> String {
    let mut h = 0xcbf29ce484222325u64;
    for p in parts {
        for b in (p.len() as u64).to_le_bytes().into_iter().chain(p.bytes()) {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3)
        }
    }
    format!("{h:016x}")
}
impl StoreId {
    pub fn for_path(provider: Provider, path: &Path) -> Self {
        Self(identity(&[provider.as_str(), &path.to_string_lossy()]))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ByteRange {
    pub start: String,
    pub end: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RowKey {
    Text { value: String },
    Integer { value: String },
    Blob { hex: String },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyColumn {
    pub column: String,
    pub value: RowKey,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FieldPath {
    Column { column: String },
    Json { column: String, pointer: String },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum NativeSource {
    Json {
        provider: Provider,
        store_id: StoreId,
        source: String,
        pointer: Option<String>,
    },
    Jsonl {
        provider: Provider,
        store_id: StoreId,
        source: String,
        line: String,
        byte_range: Option<ByteRange>,
    },
    CompressedJsonl {
        provider: Provider,
        store_id: StoreId,
        source: String,
        encoding: String,
        decoded_line: String,
        decoded_byte_range: Option<ByteRange>,
    },
    Sqlite {
        provider: Provider,
        store_id: StoreId,
        database: String,
        schema_profile: String,
        table: String,
        primary_key: Vec<KeyColumn>,
        field: Option<FieldPath>,
        observed_revision: SourceRevision,
    },
}
impl NativeSource {
    pub fn provider(&self) -> Provider {
        match self {
            Self::Json { provider, .. }
            | Self::Jsonl { provider, .. }
            | Self::CompressedJsonl { provider, .. }
            | Self::Sqlite { provider, .. } => *provider,
        }
    }
    pub fn with_field(&self, pointer: &str) -> Self {
        let mut s = self.clone();
        if let Self::Sqlite { field, .. } = &mut s {
            *field = Some(FieldPath::Json {
                column: "data".into(),
                pointer: match field.as_ref() {
                    Some(FieldPath::Json { pointer: base, .. }) => format!("{base}{pointer}"),
                    _ => pointer.into(),
                },
            });
        }
        s
    }
}
pub type NativeError = Box<views::StructuralError<NativeSource>>;
pub type NativeDiagnostic = views::Diagnostic<NativeSource>;
pub(crate) fn failure(code: &str, message: &str, source: Option<NativeSource>) -> NativeError {
    Box::new(views::StructuralError {
        code: code.into(),
        message: message.into(),
        source,
    })
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub profile: String,
    pub messages: views::Coverage,
    pub usage: views::Coverage,
    pub execution_parents: views::Coverage,
    pub timing: views::Coverage,
    pub persistence: String,
}
impl Capabilities {
    pub(crate) fn profile(profile: &str) -> Self {
        Self {
            profile: profile.into(),
            messages: views::Coverage::Complete,
            usage: views::Coverage::Partial,
            execution_parents: views::Coverage::Partial,
            timing: views::Coverage::Partial,
            persistence: "persistedRecordsOnly".into(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeRelationship {
    pub kind: String,
    pub target: String,
    pub source: NativeSource,
    pub execution: bool,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDescriptor {
    pub id: CatalogSessionId,
    pub provider: Provider,
    pub store_id: StoreId,
    pub native_id: String,
    pub recording_id: String,
    pub project: Option<String>,
    pub source_revision: SourceRevision,
    pub capabilities: Capabilities,
    pub source: NativeSource,
    pub relationships: Vec<NativeRelationship>,
    pub activity_ms: Option<u64>,
    #[serde(skip)]
    pub(crate) path: PathBuf,
}
#[derive(Debug, Clone, Copy)]
pub struct NativeLimits {
    pub import: ImportLimits,
    pub compressed_bytes: usize,
    pub batch_bytes: usize,
    pub batch_records: usize,
    pub statement_millis: u64,
}
impl Default for NativeLimits {
    fn default() -> Self {
        Self {
            import: ImportLimits::default(),
            compressed_bytes: 256 * 1024 * 1024,
            batch_bytes: 1024 * 1024,
            batch_records: 2000,
            statement_millis: 100,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NativeProgress {
    Current,
    Pending,
    CatchingUp,
}
pub struct SessionUpdate {
    pub snapshot: Option<Arc<super::SessionSnapshot>>,
    pub progress: NativeProgress,
    pub source_revision: SourceRevision,
    pub retained_decoded_bytes: usize,
    pub reset: bool,
}
pub struct DiscoveryBatch {
    pub sessions: Vec<SessionDescriptor>,
    pub visited: usize,
    pub sweep_complete: bool,
    pub limited: bool,
    pub diagnostics: Vec<DiscoveryDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TimestampMeaning {
    Persistence,
    RequestStart,
    ResultCreation,
    ExecutionStart,
    ExecutionEnd,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimingEvidence {
    pub meaning: TimestampMeaning,
    pub value: String,
    pub unit: String,
    pub source: NativeSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryDiagnostic {
    pub id: String,
    pub code: String,
    pub message: String,
    pub provider: Provider,
    pub store_id: StoreId,
    pub source: Option<NativeSource>,
}
