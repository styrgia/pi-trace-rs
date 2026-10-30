use super::{
    database::{Database, DatabaseStage, SharedDatabase},
    session::{SessionSnapshot, SessionState},
    types::*,
};
use serde_json::Value;
use std::{
    collections::{HashMap, VecDeque},
    fs::{self, File, ReadDir},
    io::{BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::UNIX_EPOCH,
};
fn io_error() -> NativeError {
    failure(
        "source_unavailable",
        "The configured source is unavailable or cannot be read safely.",
        None,
    )
}
#[derive(Clone, PartialEq, Eq)]
struct Stamp {
    identity: String,
    len: u64,
    modified: u128,
}
fn stamp(path: &Path) -> Result<Stamp, NativeError> {
    let m = fs::metadata(path).map_err(|_| io_error())?;
    #[cfg(unix)]
    let identity = {
        use std::os::unix::fs::MetadataExt;
        format!("{}:{}", m.dev(), m.ino())
    };
    #[cfg(not(unix))]
    let identity = format!("{:?}", m.created().ok());
    Ok(Stamp {
        identity,
        len: m.len(),
        modified: m
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_nanos()),
    })
}
fn revision(stamp: &Stamp) -> SourceRevision {
    SourceRevision(identity(&[
        &stamp.identity,
        &stamp.len.to_string(),
        &stamp.modified.to_string(),
    ]))
}
fn line_source(
    provider: Provider,
    store: &StoreId,
    source: &str,
    line: u64,
    start: u64,
    end: u64,
    compressed: bool,
) -> NativeSource {
    let range = Some(ByteRange {
        start: start.to_string(),
        end: end.to_string(),
    });
    if compressed {
        NativeSource::CompressedJsonl {
            provider,
            store_id: store.clone(),
            source: source.into(),
            encoding: "zstd".into(),
            decoded_line: line.to_string(),
            decoded_byte_range: range,
        }
    } else {
        NativeSource::Jsonl {
            provider,
            store_id: store.clone(),
            source: source.into(),
            line: line.to_string(),
            byte_range: range,
        }
    }
}
fn safe_file(path: &Path) -> Result<File, NativeError> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(|_| io_error())?;
    if !file.metadata().map_err(|_| io_error())?.is_file() {
        return Err(io_error());
    }
    Ok(file)
}
fn sidecar_path(path: &Path) -> Option<PathBuf> {
    path.file_name()
        .is_some_and(|n| n.to_string_lossy().starts_with("agent-"))
        .then(|| path.with_extension("meta.json"))
}
fn file_revision(provider: Provider, path: &Path, current: &Stamp) -> SourceRevision {
    let main = revision(current);
    if provider != Provider::Claude {
        return main;
    }
    let side = sidecar_path(path)
        .and_then(|p| stamp(&p).ok())
        .map(|s| revision(&s).0)
        .unwrap_or_else(|| "absent".into());
    SourceRevision(identity(&[main.as_str(), &side]))
}
fn read_stream(path: &Path, limits: NativeLimits) -> Result<Box<dyn Read + Send>, NativeError> {
    let file = safe_file(path)?;
    if path.extension().is_some_and(|x| x == "zst") {
        if file.metadata().map_err(|_| io_error())?.len() > limits.compressed_bytes as u64 {
            return Err(failure(
                "compressed_limit",
                "Compressed source exceeds the input limit.",
                None,
            ));
        }
        let reader = zstd::stream::read::Decoder::new(file).map_err(|_| {
            failure(
                "corrupt_compression",
                "The compressed source cannot be decoded.",
                None,
            )
        })?;
        Ok(Box::new(reader))
    } else {
        Ok(Box::new(file))
    }
}
fn header(
    provider: Provider,
    path: &Path,
    limits: NativeLimits,
) -> Result<(Value, u64, u64, u64), NativeError> {
    let mut stream = BufReader::new(read_stream(path, limits)?);
    let mut bytes = Vec::new();
    let mut byte = [0];
    let mut line = 1u64;
    let mut start = 0u64;
    let mut offset = 0u64;
    let bound = limits.import.record_bytes.min(64 * 1024);
    for _ in 0..bound {
        if stream.read(&mut byte).map_err(|_| {
            failure(
                "corrupt_compression",
                "The source header cannot be decoded.",
                None,
            )
        })? == 0
        {
            break;
        }
        bytes.push(byte[0]);
        offset += 1;
        if byte[0] == b'\n' {
            if let Ok(v) = serde_json::from_slice::<Value>(&bytes)
                && (provider != Provider::Claude || v.get("sessionId").is_some())
            {
                return Ok((v, line, start, offset));
            }
            if provider != Provider::Claude {
                break;
            }
            bytes.clear();
            line += 1;
            start = offset;
        }
    }
    serde_json::from_slice(&bytes)
        .map(|v| (v, line, start, offset))
        .map_err(|_| {
            failure(
                "malformed_header",
                "A supported bounded session header was not found.",
                None,
            )
        })
}
pub struct NativeStore {
    provider: Provider,
    root: PathBuf,
    id: StoreId,
    limits: NativeLimits,
    queue: VecDeque<PathBuf>,
    directories: Vec<(PathBuf, ReadDir)>,
    pending_directory_errors: VecDeque<PathBuf>,
    databases: HashMap<PathBuf, SharedDatabase>,
    db_pending: VecDeque<(PathBuf, String)>,
    database_capacity: usize,
}
impl NativeStore {
    pub fn open(
        provider: Provider,
        root: &Path,
        limits: NativeLimits,
    ) -> Result<Self, NativeError> {
        if root.as_os_str() == ":memory:" {
            return Err(failure(
                "unsupported_store",
                "Memory-only databases cannot be discovered.",
                None,
            ));
        }
        let root = fs::canonicalize(root).map_err(|_| io_error())?;
        let id = StoreId::for_path(provider, &root);
        let mut s = Self {
            provider,
            root,
            id,
            limits,
            queue: VecDeque::new(),
            directories: Vec::new(),
            pending_directory_errors: VecDeque::new(),
            databases: HashMap::new(),
            db_pending: VecDeque::new(),
            database_capacity: 16,
        };
        s.restart();
        Ok(s)
    }
    pub fn database_connections(&self) -> usize {
        self.databases.len()
    }
    pub fn set_database_capacity(&mut self, max: usize) {
        self.database_capacity = max.min(16);
    }
    pub fn store_id(&self) -> &StoreId {
        &self.id
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    fn restart(&mut self) {
        if self.provider == Provider::Codex && self.root.is_dir() {
            for name in ["sessions", "archived_sessions"] {
                self.queue.push_back(self.root.join(name));
            }
        } else {
            self.queue.push_back(self.root.clone());
        }
    }
    fn next_path(&mut self) -> Option<Result<PathBuf, PathBuf>> {
        if let Some(p) = self.pending_directory_errors.pop_front() {
            return Some(Err(p));
        }
        if let Some(p) = self.queue.pop_front() {
            return Some(Ok(p));
        }
        while let Some((path, dir)) = self.directories.last_mut() {
            match dir.next() {
                Some(Ok(e)) => return Some(Ok(e.path())),
                Some(Err(_)) => return Some(Err(path.clone())),
                None => {
                    self.directories.pop();
                }
            }
        }
        None
    }
    fn directory_diagnostic(&self, path: &Path) -> DiscoveryDiagnostic {
        DiscoveryDiagnostic {
            id: identity(&[
                self.id.as_str(),
                &self.relative(path),
                "directory_unavailable",
            ]),
            code: "directory_unavailable".into(),
            message: "A configured directory or directory entry could not be inspected.".into(),
            provider: self.provider,
            store_id: self.id.clone(),
            source: None,
        }
    }
    pub fn discover(&mut self, budget: usize) -> Result<DiscoveryBatch, NativeError> {
        let budget = budget.clamp(1, 100_000);
        let mut batch = DiscoveryBatch {
            sessions: Vec::new(),
            visited: 0,
            sweep_complete: false,
            limited: false,
            diagnostics: Vec::new(),
        };
        while batch.visited < budget {
            if let Some((path, after)) = self.db_pending.pop_front() {
                let db = self.databases.get(&path).ok_or_else(io_error)?;
                let count = (budget - batch.visited).min(512);
                let result = db.lock().map_err(|_| io_error())?.discover(&after, count);
                let sessions = match result {
                    Ok(v) => v,
                    Err(e) => {
                        self.db_pending.push_front((path, after));
                        return Err(e);
                    }
                };
                batch.visited += sessions.len().max(1);
                if sessions.len() == count {
                    self.db_pending
                        .push_back((path, sessions.last().unwrap().native_id.clone()));
                }
                for d in sessions {
                    batch.sessions.push(d);
                }
                continue;
            }
            let Some(candidate) = self.next_path() else {
                batch.sweep_complete = true;
                self.restart();
                break;
            };
            batch.visited += 1;
            let path = match candidate {
                Ok(path) => path,
                Err(path) => {
                    batch.diagnostics.push(self.directory_diagnostic(&path));
                    continue;
                }
            };
            let meta = match fs::symlink_metadata(&path) {
                Ok(m) => m,
                Err(e) => {
                    if !(e.kind() == std::io::ErrorKind::NotFound
                        && self.provider == Provider::Codex
                        && path.parent() == Some(self.root.as_path()))
                    {
                        batch.diagnostics.push(self.directory_diagnostic(&path));
                    }
                    continue;
                }
            };
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                match fs::read_dir(&path) {
                    Ok(dir) => self.directories.push((path, dir)),
                    Err(_) => batch.diagnostics.push(self.directory_diagnostic(&path)),
                }
                continue;
            }
            if !meta.is_file() {
                continue;
            }
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if self.provider == Provider::Opencode {
                if self.root.is_file()
                    || (path.extension().is_some_and(|e| e == "db") && name.starts_with("opencode"))
                {
                    if !self.databases.contains_key(&path) {
                        if self.databases.len() >= self.database_capacity {
                            batch.diagnostics.push(self.discovery_diagnostic(
                                failure(
                                    "database_connection_limit",
                                    "The configured database connection limit is reached.",
                                    None,
                                ),
                                &path,
                            ));
                            continue;
                        }
                        match Database::open(&path, self.id.clone(), self.limits) {
                            Ok(db) => {
                                self.databases
                                    .insert(path.clone(), Arc::new(Mutex::new(db)));
                            }
                            Err(e) => {
                                batch.diagnostics.push(self.discovery_diagnostic(e, &path));
                                continue;
                            }
                        }
                    }
                    self.db_pending.push_back((path, String::new()));
                }
                continue;
            }
            if !name.ends_with(".jsonl") && !name.ends_with(".jsonl.zst") {
                continue;
            }
            if name.ends_with(".zst") && path.with_extension("").exists() {
                continue;
            }
            match self.probe(&path) {
                Ok(d) => batch.sessions.push(d),
                Err(e) => batch.diagnostics.push(self.discovery_diagnostic(e, &path)),
            }
        }
        if !batch.sweep_complete && self.db_pending.is_empty() {
            if let Some(next) = self.next_path() {
                match next {
                    Ok(path) => self.queue.push_front(path),
                    Err(path) => self.pending_directory_errors.push_front(path),
                };
            } else {
                batch.sweep_complete = true;
                self.restart();
            }
        }
        batch.limited = !batch.sweep_complete || !batch.diagnostics.is_empty();
        Ok(batch)
    }
    fn discovery_diagnostic(&self, e: NativeError, path: &Path) -> DiscoveryDiagnostic {
        let relative = self.relative(path);
        DiscoveryDiagnostic {
            provider: self.provider,
            store_id: self.id.clone(),
            id: identity(&[&relative, &e.code]),
            code: e.code,
            message: e.message,
            source: Some(e.source.unwrap_or_else(|| {
                if self.provider == Provider::Opencode {
                    NativeSource::Sqlite {
                        provider: self.provider,
                        store_id: self.id.clone(),
                        database: relative.clone(),
                        schema_profile: "unrecognized".into(),
                        table: "sqlite_schema".into(),
                        primary_key: Vec::new(),
                        field: None,
                        observed_revision: SourceRevision("unavailable".into()),
                    }
                } else {
                    {
                        let mut source = line_source(
                            self.provider,
                            &self.id,
                            &relative,
                            1,
                            0,
                            0,
                            path.extension().is_some_and(|s| s == "zst"),
                        );
                        match &mut source {
                            NativeSource::Jsonl { byte_range, .. } => *byte_range = None,
                            NativeSource::CompressedJsonl {
                                decoded_byte_range, ..
                            } => *decoded_byte_range = None,
                            _ => {}
                        }
                        source
                    }
                }
            })),
        }
    }
    fn relative(&self, path: &Path) -> String {
        if self.root.is_file() {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into()
        } else {
            path.strip_prefix(&self.root)
                .unwrap_or(path)
                .to_string_lossy()
                .into()
        }
    }
    fn probe(&self, path: &Path) -> Result<SessionDescriptor, NativeError> {
        let (v, header_line, header_start, header_end) = header(self.provider, path, self.limits)?;
        let h = if self.provider == Provider::Codex {
            if v["type"] != "session_meta" {
                return Err(failure(
                    "unsupported_header",
                    "The Codex source does not start with session metadata.",
                    None,
                ));
            }
            &v["payload"]
        } else {
            &v
        };
        let native = h
            .get(if self.provider == Provider::Claude {
                "sessionId"
            } else {
                "id"
            })
            .and_then(Value::as_str)
            .ok_or_else(|| {
                failure(
                    "unsupported_header",
                    "The native header has no supported session identity.",
                    None,
                )
            })?;
        let profile = match self.provider {
            Provider::Pi => {
                if h["type"] != "session" {
                    return Err(failure(
                        "unsupported_header",
                        "The Pi source does not start with a session header.",
                        None,
                    ));
                }
                match h.get("version").and_then(Value::as_u64) {
                    None | Some(1) => "pi-session-v1",
                    Some(2) => "pi-session-v2",
                    Some(3) => "pi-session-v3",
                    _ => {
                        return Err(failure(
                            "unsupported_header",
                            "The Pi header version is unsupported.",
                            None,
                        ));
                    }
                }
            }
            Provider::Codex => "codex-0.166-rollout",
            Provider::Claude => "claude-transcript-detected",
            Provider::Opencode => unreachable!(),
        };
        let relative = self.relative(path);
        let recording = if self.provider == Provider::Codex {
            let filename = path.file_name().unwrap_or_default().to_string_lossy();
            let name = filename.strip_suffix(".zst").unwrap_or(&filename);
            let core = name
                .strip_prefix("rollout-")
                .and_then(|x| x.strip_suffix(".jsonl"))
                .and_then(|x| x.get(20..))
                .ok_or_else(|| {
                    failure(
                        "unsupported_header",
                        "Codex recording identity requires a canonical rollout filename.",
                        None,
                    )
                })?;
            let (thread, rollout) = core.split_once('_').unwrap_or((core, core));
            if thread != native {
                return Err(failure(
                    "identity_mismatch",
                    "The Codex filename and metadata thread identities disagree.",
                    None,
                ));
            }
            rollout.to_owned()
        } else {
            native.to_owned()
        };
        let st = stamp(path)?;
        let source = line_source(
            self.provider,
            &self.id,
            &relative,
            header_line,
            header_start,
            header_end,
            path.extension().is_some_and(|x| x == "zst"),
        );
        let mut relationships = Vec::new();
        for (field, kind) in [
            ("parentSession", "fork"),
            ("forked_from_id", "fork"),
            ("parent_thread_id", "parentThread"),
            ("parentSessionId", "parentSession"),
        ] {
            if let Some(target) = h[field].as_str() {
                relationships.push(NativeRelationship {
                    kind: kind.into(),
                    target: target.into(),
                    source: source.clone(),
                    execution: false,
                });
            }
        }
        if self.provider == Provider::Codex {
            relationships.push(NativeRelationship {
                kind: "threadGroup".into(),
                target: native.into(),
                source: source.clone(),
                execution: false,
            });
        }
        let mut capabilities = Capabilities::profile(profile);
        if self.provider == Provider::Claude {
            if let Some(sidecar) = sidecar_path(path)
                && fs::canonicalize(&sidecar)
                    .is_ok_and(|p| p == sidecar && p.starts_with(&self.root))
                && let Ok(file) = safe_file(&sidecar)
            {
                let mut bytes = Vec::new();
                if file.take(65537).read_to_end(&mut bytes).is_ok()
                    && bytes.len() <= 65536
                    && let Ok(metadata) = serde_json::from_slice::<Value>(&bytes)
                {
                    for (field, kind) in [
                        ("toolUseId", "recordedInvokingTool"),
                        ("parentAgentId", "parentAgent"),
                    ] {
                        if let Some(target) = metadata[field].as_str() {
                            relationships.push(NativeRelationship {
                                kind: kind.into(),
                                target: crate::bounded(target, 512).0,
                                source: NativeSource::Json {
                                    provider: self.provider,
                                    store_id: self.id.clone(),
                                    source: self.relative(&sidecar),
                                    pointer: Some(format!("/{field}")),
                                },
                                execution: false,
                            });
                            capabilities.profile = "claude-transcript-sdk-sidecar-detected".into();
                        }
                    }
                }
            }
            let components: Vec<_> = path.components().collect();
            if let Some(i) = components.iter().position(|p| p.as_os_str() == "subagents")
                && i > 0
            {
                relationships.push(NativeRelationship {
                    kind: "parentSession".into(),
                    target: components[i - 1].as_os_str().to_string_lossy().into(),
                    source: source.clone(),
                    execution: false,
                });
            }
        }
        let discriminator = if self.provider == Provider::Codex {
            ""
        } else {
            &relative
        };
        Ok(SessionDescriptor {
            id: CatalogSessionId(identity(&[
                self.provider.as_str(),
                self.id.as_str(),
                &recording,
                discriminator,
            ])),
            provider: self.provider,
            store_id: self.id.clone(),
            native_id: native.into(),
            recording_id: recording,
            project: h["cwd"].as_str().map(|x| crate::bounded(x, 512).0),
            source_revision: file_revision(self.provider, path, &st),
            capabilities,
            source,
            relationships,
            activity_ms: u64::try_from(st.modified / 1_000_000).ok(),
            path: path.into(),
        })
    }
    fn validate_descriptor(&self, d: &SessionDescriptor) -> Result<(), NativeError> {
        let canonical = fs::canonicalize(&d.path).map_err(|_| io_error())?;
        if d.provider != self.provider
            || d.store_id != self.id
            || canonical != d.path
            || if self.root.is_file() {
                canonical != self.root
            } else {
                !canonical.starts_with(&self.root)
            }
        {
            return Err(failure(
                "unknown_session",
                "The session is not in this configured store.",
                None,
            ));
        }
        Ok(())
    }
    pub fn recheck(&mut self, session: &SessionDescriptor) -> Result<SourceRevision, NativeError> {
        self.validate_descriptor(session)?;
        let d = session;
        if d.provider == Provider::Opencode {
            return self
                .databases
                .get(&d.path)
                .ok_or_else(io_error)?
                .lock()
                .map_err(|_| io_error())?
                .revision();
        }
        Ok(file_revision(self.provider, &d.path, &stamp(&d.path)?))
    }
    pub fn reader(&mut self, session: &SessionDescriptor) -> Result<SessionReader, NativeError> {
        self.validate_descriptor(session)?;
        let descriptor = session.clone();
        if descriptor.provider == Provider::Opencode {
            let db = self
                .databases
                .get(&descriptor.path)
                .ok_or_else(io_error)?
                .clone();
            Ok(SessionReader::database(descriptor, self.limits, db))
        } else {
            self.file_reader(descriptor, &mut Vec::new())
        }
    }
    fn file_reader(
        &self,
        descriptor: SessionDescriptor,
        seen: &mut Vec<String>,
    ) -> Result<SessionReader, NativeError> {
        if seen.len() >= self.limits.import.sources || seen.contains(&descriptor.recording_id) {
            return Err(failure(
                "history_cycle",
                "The inherited history chain is cyclic or exceeds its source limit.",
                Some(descriptor.source.clone()),
            ));
        }
        let probed = self.probe(&descriptor.path)?;
        if probed.native_id != descriptor.native_id
            || probed.recording_id != descriptor.recording_id
        {
            return Err(failure(
                "identity_changed",
                "The source now belongs to another logical recording.",
                Some(descriptor.source.clone()),
            ));
        }
        seen.push(descriptor.recording_id.clone());
        let mut reader = SessionReader::file(probed, self.limits)?;
        reader.configured_root = Some(self.root.clone());
        if self.provider == Provider::Codex {
            let (h, _, _, _) = header(self.provider, &descriptor.path, self.limits)?;
            if let Some(base) = h.pointer("/payload/history_base").filter(|v| !v.is_null()) {
                let recording = base["thread_id"].as_str().ok_or_else(|| {
                    failure(
                        "unsupported_history",
                        "History base recording identity is unavailable.",
                        Some(descriptor.source.clone()),
                    )
                })?;
                let end = base["end_byte_offset"].as_u64().ok_or_else(|| {
                    failure(
                        "unsupported_history",
                        "History base byte boundary is unavailable.",
                        Some(descriptor.source.clone()),
                    )
                })?;
                reader.inheritance_boundary = h
                    .pointer("/payload/forked_from_ordinal_exclusive")
                    .and_then(Value::as_u64)
                    .into_iter()
                    .chain(
                        h.pointer("/payload/subagent_history_start_ordinal")
                            .and_then(Value::as_u64),
                    )
                    .max();
                reader.inheritance_origin = h
                    .pointer("/payload/forked_from_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                reader.boundary_unknown =
                    reader.inheritance_origin.is_some() && reader.inheritance_boundary.is_none();
                reader.history_search = Some(Box::new(HistorySearch {
                    traversal: NativeStore::open(self.provider, &self.root, self.limits)?,
                    recording: recording.into(),
                    end,
                    visited: 0,
                    matches: Vec::new(),
                    ancestry: seen.clone(),
                }));
            }
        }
        seen.pop();
        Ok(reader)
    }
}
enum ReaderInput {
    File {
        stream: Box<dyn Read + Send>,
        stamp: Stamp,
        compressed: bool,
        closed: bool,
        offset: u64,
        line: u64,
        pending: Vec<u8>,
        tail: Vec<u8>,
    },
    Database {
        db: SharedDatabase,
        last: Option<SourceRevision>,
        stage: Option<Box<DatabaseStage>>,
        published_bytes: usize,
        identity: Option<String>,
        reset_pending: bool,
    },
}
struct HistorySearch {
    traversal: NativeStore,
    recording: String,
    end: u64,
    visited: usize,
    matches: Vec<SessionDescriptor>,
    ancestry: Vec<String>,
}
pub struct SessionReader {
    descriptor: SessionDescriptor,
    limits: NativeLimits,
    input: ReaderInput,
    state: SessionState,
    snapshot: Option<Arc<SessionSnapshot>>,
    retained: usize,
    read_records: usize,
    read_bytes: usize,
    prefix: Option<Box<SessionReader>>,
    prefix_end: Option<u64>,
    configured_root: Option<PathBuf>,
    history_search: Option<Box<HistorySearch>>,
    inheritance_boundary: Option<u64>,
    inheritance_origin: Option<String>,
    boundary_unknown: bool,
}
impl SessionReader {
    fn file(descriptor: SessionDescriptor, limits: NativeLimits) -> Result<Self, NativeError> {
        let input = ReaderInput::File {
            stream: read_stream(&descriptor.path, limits)?,
            stamp: stamp(&descriptor.path)?,
            compressed: descriptor.path.extension().is_some_and(|s| s == "zst"),
            closed: descriptor.path.extension().is_some_and(|s| s == "zst")
                || descriptor
                    .path
                    .components()
                    .any(|p| p.as_os_str() == "archived_sessions"),
            offset: 0,
            line: 1,
            pending: Vec::new(),
            tail: Vec::new(),
        };
        let state = SessionState::new(descriptor.clone());
        Ok(Self {
            descriptor,
            limits,
            input,
            state,
            snapshot: None,
            retained: 0,
            read_records: 0,
            read_bytes: 0,
            prefix: None,
            prefix_end: None,
            configured_root: None,
            history_search: None,
            inheritance_boundary: None,
            inheritance_origin: None,
            boundary_unknown: false,
        })
    }
    fn database(descriptor: SessionDescriptor, limits: NativeLimits, db: SharedDatabase) -> Self {
        let state = SessionState::new(descriptor.clone());
        Self {
            descriptor,
            limits,
            input: ReaderInput::Database {
                db,
                last: None,
                stage: None,
                published_bytes: 0,
                identity: None,
                reset_pending: false,
            },
            state,
            snapshot: None,
            retained: 0,
            read_records: 0,
            read_bytes: 0,
            prefix: None,
            prefix_end: None,
            configured_root: None,
            history_search: None,
            inheritance_boundary: None,
            inheritance_origin: None,
            boundary_unknown: false,
        }
    }
    pub fn read_records(&self) -> usize {
        self.read_records
    }
    pub fn read_bytes(&self) -> usize {
        self.read_bytes
    }
    pub fn snapshot(&self) -> Option<Arc<SessionSnapshot>> {
        self.snapshot.clone()
    }
    pub fn poll_bounded(&mut self, additional_bytes: usize) -> Result<SessionUpdate, NativeError> {
        let original = self.limits.import.total_bytes;
        self.limits.import.total_bytes =
            original.min(self.retained.saturating_add(additional_bytes));
        let result = self.poll();
        self.limits.import.total_bytes = original;
        result
    }
    pub fn poll(&mut self) -> Result<SessionUpdate, NativeError> {
        if let Some(search) = &mut self.history_search {
            let mut complete = false;
            for _ in 0..self.limits.batch_records.clamp(1, 512) {
                let Some(candidate) = search.traversal.next_path() else {
                    complete = true;
                    break;
                };
                if search.visited >= 100_000 {
                    return Err(failure(
                        "history_discovery_limit",
                        "History base discovery exceeded its traversal limit.",
                        None,
                    ));
                }
                search.visited += 1;
                let path = candidate.map_err(|_| {
                    failure(
                        "history_directory_unavailable",
                        "A history directory cannot be inspected.",
                        None,
                    )
                })?;
                let meta = match fs::symlink_metadata(&path) {
                    Ok(m) => m,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(_) => return Err(io_error()),
                };
                if meta.file_type().is_symlink() {
                    continue;
                }
                if meta.is_dir() {
                    let dir = fs::read_dir(&path).map_err(|_| io_error())?;
                    search.traversal.directories.push((path, dir));
                    continue;
                }
                if !meta.is_file() {
                    continue;
                }
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                let plain = name.strip_suffix(".zst").unwrap_or(&name);
                if plain.ends_with(&format!("-{}.jsonl", search.recording))
                    || plain.ends_with(&format!("_{}.jsonl", search.recording))
                {
                    if path.extension().is_some_and(|e| e == "zst")
                        && path.with_extension("").exists()
                    {
                        continue;
                    }
                    search.matches.push(search.traversal.probe(&path)?);
                    if search.matches.len() > 1 {
                        return Err(failure(
                            "history_unavailable",
                            "The history base cannot be resolved uniquely inside the configured store.",
                            None,
                        ));
                    }
                }
            }
            if complete {
                let mut search = *self.history_search.take().unwrap();
                if search.matches.len() != 1 {
                    return Err(failure(
                        "history_unavailable",
                        "The history base cannot be resolved uniquely inside the configured store.",
                        None,
                    ));
                }
                let descriptor = search.matches.remove(0);
                self.boundary_unknown |= self.inheritance_boundary.is_none()
                    && descriptor.native_id != self.descriptor.native_id;
                let mut prefix = search
                    .traversal
                    .file_reader(descriptor, &mut search.ancestry)?;
                prefix.prefix_end = Some(search.end);
                if let ReaderInput::File { stream, closed, .. } = &mut prefix.input {
                    *closed = false;
                    let old = std::mem::replace(stream, Box::new(std::io::empty()));
                    *stream = Box::new(old.take(search.end));
                }
                self.prefix = Some(Box::new(prefix));
            }
            return Ok(SessionUpdate {
                snapshot: None,
                progress: NativeProgress::CatchingUp,
                source_revision: self.descriptor.source_revision.clone(),
                retained_decoded_bytes: self.retained,
                reset: false,
            });
        }
        if let Some(prefix) = &mut self.prefix {
            let before = (prefix.read_records(), prefix.read_bytes());
            let update = prefix
                .poll_bounded(self.limits.import.total_bytes.saturating_sub(self.retained))?;
            self.read_records = self
                .read_records
                .saturating_add(prefix.read_records() - before.0);
            self.read_bytes = self
                .read_bytes
                .saturating_add(prefix.read_bytes() - before.1);
            self.retained = prefix.retained;
            if update.progress == NativeProgress::CatchingUp {
                return Ok(SessionUpdate {
                    snapshot: None,
                    progress: NativeProgress::CatchingUp,
                    source_revision: self.descriptor.source_revision.clone(),
                    retained_decoded_bytes: self.retained,
                    reset: false,
                });
            }
            if update.progress == NativeProgress::Pending {
                return Err(failure(
                    "history_boundary",
                    "The inherited prefix does not end at a complete record boundary.",
                    Some(prefix.descriptor.source.clone()),
                ));
            }
            let mut state = prefix.state.clone();
            state.descriptor = self.descriptor.clone();
            state.run_id = self.state.run_id.clone();
            for claim in state.ledger.claims.values_mut() {
                claim.view.role = claim.base_role.clone();
                if matches!(
                    claim.base_role,
                    super::usage::UsageRole::Contribution { .. }
                ) {
                    if let Some(boundary) = self.inheritance_boundary {
                        match claim.ordinal {
                            Some(ordinal) if ordinal < boundary => {
                                claim.view.role = super::usage::UsageRole::Inherited {
                                    origin: self
                                        .inheritance_origin
                                        .clone()
                                        .unwrap_or_else(|| "recordedContextPrefix".into()),
                                }
                            }
                            None => claim.partial = true,
                            _ => {}
                        }
                    } else if self.boundary_unknown {
                        claim.partial = true;
                    }
                }
            }
            state.attribution_partial = self.boundary_unknown;
            state.inherited_before = self.inheritance_boundary;
            self.state = state;
            self.prefix = None;
            return Ok(SessionUpdate {
                snapshot: None,
                progress: NativeProgress::CatchingUp,
                source_revision: self.descriptor.source_revision.clone(),
                retained_decoded_bytes: self.retained,
                reset: false,
            });
        }
        if let ReaderInput::Database {
            db,
            last,
            stage,
            published_bytes,
            identity,
            reset_pending,
        } = &mut self.input
        {
            let mut db = db.lock().map_err(|_| io_error())?;
            let revision = if let Some(s) = stage.as_ref() {
                s.revision.clone()
            } else {
                db.revision()?
            };
            let current_identity = db.identity().to_owned();
            if identity
                .as_ref()
                .is_some_and(|old| old != &current_identity)
            {
                *reset_pending = true;
            }
            *identity = Some(current_identity);
            if stage.as_ref().is_some_and(|s| s.revision != revision) {
                *stage = None;
                self.retained = *published_bytes;
            }
            if stage.is_none() && last.as_ref() == Some(&revision) {
                return Ok(SessionUpdate {
                    snapshot: None,
                    progress: NativeProgress::Current,
                    source_revision: revision,
                    retained_decoded_bytes: self.retained,
                    reset: false,
                });
            }
            if stage.is_none() {
                *stage = Some(Box::new(db.begin(
                    &self.descriptor,
                    if *reset_pending {
                        None
                    } else {
                        Some(&self.state.run_id)
                    },
                )?));
            }
            let pending = stage.as_mut().unwrap();
            let before = (pending.read_records, pending.read_bytes);
            let result = db.batch(
                pending,
                self.limits.import.total_bytes.saturating_sub(self.retained),
            );
            self.read_records = self
                .read_records
                .saturating_add(pending.read_records - before.0);
            self.read_bytes = self
                .read_bytes
                .saturating_add(pending.read_bytes - before.1);
            self.retained = published_bytes.saturating_add(pending.bytes);
            let complete = match result {
                Ok(done) => done,
                Err(e) if e.code == "source_changed" => {
                    *stage = None;
                    self.retained = *published_bytes;
                    return Ok(SessionUpdate {
                        snapshot: None,
                        progress: NativeProgress::CatchingUp,
                        source_revision: revision,
                        retained_decoded_bytes: self.retained,
                        reset: false,
                    });
                }
                Err(e) => {
                    db.cancel(&pending.lease_key);
                    *stage = None;
                    self.retained = *published_bytes;
                    return Err(e);
                }
            };
            if !complete {
                return Ok(SessionUpdate {
                    snapshot: None,
                    progress: NativeProgress::CatchingUp,
                    source_revision: revision,
                    retained_decoded_bytes: self.retained,
                    reset: false,
                });
            }
            let finished = *stage.take().unwrap();
            let snapshot = Arc::new(finished.state.project(&revision, self.limits));
            self.retained = finished.bytes;
            *published_bytes = finished.bytes;
            self.state = finished.state;
            self.snapshot = Some(snapshot.clone());
            *last = Some(revision.clone());
            let reset = std::mem::take(reset_pending);
            return Ok(SessionUpdate {
                snapshot: Some(snapshot),
                progress: NativeProgress::Current,
                source_revision: revision,
                retained_decoded_bytes: self.retained,
                reset,
            });
        }
        let mut reset = false;
        let current = stamp(&self.descriptor.path)?;
        if let ReaderInput::File {
            stamp: prior,
            offset,
            tail,
            compressed,
            ..
        } = &self.input
        {
            let mut rewrite = prior.identity != current.identity
                || (!*compressed && current.len < *offset)
                || (*compressed && prior != &current)
                || (!compressed && current.len == prior.len && current.modified != prior.modified);
            if !rewrite && prior != &current && !tail.is_empty() {
                let mut file = File::open(&self.descriptor.path).map_err(|_| io_error())?;
                file.seek(SeekFrom::Start(offset.saturating_sub(tail.len() as u64)))
                    .map_err(|_| io_error())?;
                let mut actual = vec![0; tail.len()];
                if file.read_exact(&mut actual).is_err() || actual != *tail {
                    rewrite = true;
                }
            }
            if rewrite {
                if self.prefix_end.is_some() {
                    return Err(failure(
                        "history_changed",
                        "The history prefix changed during analysis.",
                        Some(self.descriptor.source.clone()),
                    ));
                }
                let old = self.snapshot.clone();
                let counters = (self.read_records, self.read_bytes);
                let fresh = if let Some(root) = &self.configured_root {
                    NativeStore::open(self.descriptor.provider, root, self.limits)?
                        .file_reader(self.descriptor.clone(), &mut Vec::new())?
                } else {
                    Self::file(self.descriptor.clone(), self.limits)?
                };
                *self = fresh;
                self.snapshot = old;
                self.read_records = counters.0;
                self.read_bytes = counters.1;
                reset = true;
            }
        }
        let revision = file_revision(self.descriptor.provider, &self.descriptor.path, &current);
        let descriptor_changed = revision != self.descriptor.source_revision;
        if descriptor_changed
            && self.descriptor.provider == Provider::Claude
            && let Some(root) = &self.configured_root
        {
            let descriptor = NativeStore::open(self.descriptor.provider, root, self.limits)?
                .probe(&self.descriptor.path)?;
            self.descriptor.relationships = descriptor.relationships;
            self.descriptor.capabilities = descriptor.capabilities;
        }
        self.descriptor.source_revision = revision.clone();
        self.state.descriptor = self.descriptor.clone();
        if reset && (self.prefix.is_some() || self.history_search.is_some()) {
            return Ok(SessionUpdate {
                snapshot: None,
                progress: NativeProgress::CatchingUp,
                source_revision: revision,
                retained_decoded_bytes: self.retained,
                reset,
            });
        }
        let ReaderInput::File {
            stream,
            stamp: prior,
            compressed,
            closed,
            offset,
            line,
            pending,
            tail,
        } = &mut self.input
        else {
            unreachable!()
        };
        let remaining = self.limits.import.total_bytes.saturating_sub(self.retained);
        if remaining == 0 && (!*compressed && self.prefix_end.unwrap_or(current.len) > *offset) {
            return Err(failure(
                "decoded_limit",
                "The remaining decoded retention budget is exhausted.",
                Some(self.descriptor.source.clone()),
            ));
        }
        if remaining == 0 && *compressed {
            let mut probe = [0];
            if stream.read(&mut probe).map_err(|_| io_error())? > 0 {
                return Err(failure(
                    "decoded_limit",
                    "Decoded input limit exceeded.",
                    Some(self.descriptor.source.clone()),
                ));
            }
        }
        let mut bytes = vec![0u8; self.limits.batch_bytes.clamp(1, 1024 * 1024).min(remaining)];
        let n = stream.read(&mut bytes).map_err(|_| {
            failure(
                "corrupt_source",
                "The source cannot be decoded; the previous snapshot remains available.",
                Some(self.descriptor.source.clone()),
            )
        })?;
        if n == 0 && self.prefix_end.is_some_and(|end| *offset < end) && remaining > 0 {
            return Err(failure(
                "history_boundary",
                "The history source ends before its recorded byte boundary.",
                Some(self.descriptor.source.clone()),
            ));
        }
        bytes.truncate(n);
        self.read_bytes = self.read_bytes.saturating_add(n);
        self.retained = self
            .retained
            .checked_add(n)
            .ok_or_else(|| failure("decoded_limit", "Decoded input limit exceeded.", None))?;
        if self.retained > self.limits.import.total_bytes {
            return Err(failure(
                "decoded_limit",
                "Decoded input limit exceeded.",
                Some(self.descriptor.source.clone()),
            ));
        }
        *offset += n as u64;
        tail.extend_from_slice(&bytes);
        if tail.len() > 4096 {
            tail.drain(..tail.len() - 4096);
        }
        pending.extend_from_slice(&bytes);
        let relative = match &self.descriptor.source {
            NativeSource::Jsonl { source, .. } | NativeSource::CompressedJsonl { source, .. } => {
                source.clone()
            }
            _ => unreachable!(),
        };
        let before = self.state.records;
        let mut consumed = 0usize;
        let mut records = 0usize;
        while records < self.limits.batch_records {
            let rest = &pending[consumed..];
            let end = rest
                .iter()
                .position(|b| *b == b'\n')
                .map(|p| p + 1)
                .or_else(|| (*closed && n == 0 && !rest.is_empty()).then_some(rest.len()));
            let Some(end) = end else { break };
            if end > self.limits.import.record_bytes {
                return Err(failure(
                    "record_limit",
                    "A native record exceeds its byte limit.",
                    Some(self.descriptor.source.clone()),
                ));
            }
            if self.state.records >= self.limits.import.records {
                return Err(failure(
                    "record_limit",
                    "Native record count limit exceeded.",
                    Some(self.descriptor.source.clone()),
                ));
            }
            let start = offset.saturating_sub(pending.len() as u64) + consumed as u64;
            let source = line_source(
                self.descriptor.provider,
                &self.descriptor.store_id,
                &relative,
                *line,
                start,
                start + end as u64,
                *compressed,
            );
            match serde_json::from_slice::<Value>(&rest[..end]) {
                Ok(v) => self.state.decode(&v, source),
                Err(_) => {
                    self.state.records += 1;
                    self.state.diagnostic("malformed_record", source);
                }
            }
            *line += 1;
            records += 1;
            consumed += end;
        }
        self.read_records = self.read_records.saturating_add(records);
        pending.drain(..consumed);
        if pending.len() > self.limits.import.record_bytes && !pending.contains(&b'\n') {
            return Err(failure(
                "record_limit",
                "An unfinished native record exceeds its byte limit.",
                Some(self.descriptor.source.clone()),
            ));
        }
        *prior = current;
        let progress = if (*closed && n > 0)
            || self.prefix_end.is_some_and(|end| *offset < end)
            || records >= self.limits.batch_records
            || (n > 0 && n == bytes.capacity())
            || pending.contains(&b'\n')
        {
            NativeProgress::CatchingUp
        } else if !pending.is_empty() {
            NativeProgress::Pending
        } else {
            NativeProgress::Current
        };
        let changed =
            before != self.state.records || self.snapshot.is_none() || reset || descriptor_changed;
        let snapshot = changed.then(|| Arc::new(self.state.project(&revision, self.limits)));
        if let Some(s) = &snapshot {
            self.snapshot = Some(s.clone());
        }
        Ok(SessionUpdate {
            snapshot,
            progress,
            source_revision: revision,
            retained_decoded_bytes: self.retained,
            reset,
        })
    }
}

impl Drop for SessionReader {
    fn drop(&mut self) {
        if let ReaderInput::Database {
            db,
            stage: Some(stage),
            ..
        } = &self.input
            && let Ok(mut db) = db.lock()
        {
            db.cancel(&stage.lease_key);
        }
    }
}
