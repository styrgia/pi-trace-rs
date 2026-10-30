use crate::system_config::{StoreConfig, SystemConfig};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use trace_core::native::{
    NativeError, NativeLimits, NativeProgress, NativeStore, SessionDescriptor, SessionReader,
    SessionSnapshot, StoreId,
};

const CATALOG_LIMIT: usize = 10_000;
const GLOBAL_BYTES: usize = 256 * 1024 * 1024;
const BATCH: usize = 512;
const PASS: usize = 100_000;
fn error(code: &str, message: &str) -> NativeError {
    Box::new(trace_core::views::StructuralError {
        code: code.into(),
        message: message.into(),
        source: None,
    })
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogItem {
    #[serde(flatten)]
    pub descriptor: SessionDescriptor,
    pub ingestion_state: String,
    pub last_observed_ms: u64,
    pub process_state: &'static str,
    #[serde(skip)]
    seen_generation: u64,
}
#[derive(Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Coverage {
    pub state: String,
    pub limits: Vec<String>,
    pub retained_entries: usize,
    pub omitted_count: Option<usize>,
}
#[derive(Clone)]
struct CatalogRevision {
    id: u64,
    items: Vec<CatalogItem>,
    coverage: Coverage,
}
struct Store {
    config: StoreConfig,
    id: StoreId,
    native: Option<NativeStore>,
    state: &'static str,
    error: Option<NativeError>,
    next: Instant,
    visited: usize,
    pass_visited: usize,
    sweep_complete: bool,
    sweep_generation: u64,
    sweep_ids: BTreeSet<String>,
    session_count: usize,
    limited: bool,
    failures: u32,
    scan_error: Option<NativeError>,
}
struct Analysis {
    reader: Option<SessionReader>,
    current: Option<Arc<SessionSnapshot>>,
    previous: Option<Arc<SessionSnapshot>>,
    requested: Instant,
    next: Instant,
    bytes: usize,
    reader_bytes: usize,
    current_bytes: usize,
    previous_bytes: usize,
    recovering: bool,
    reserved: usize,
    in_flight: bool,
    generation: u64,
    revision: u64,
    progress: NativeProgress,
    error: Option<NativeError>,
    failures: u32,
    source_revision: String,
    binding: String,
}
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Metrics {
    parse_jobs: u64,
    read_records: u64,
    read_bytes: u64,
    database_connections: usize,
    discovery_batches: u64,
    loaded_entries: usize,
    retained_bytes: usize,
    active_parse_jobs: usize,
    max_active_parse_jobs: usize,
}
struct State {
    stores: Vec<Store>,
    items: BTreeMap<String, CatalogItem>,
    revisions: VecDeque<CatalogRevision>,
    analyses: BTreeMap<String, Analysis>,
    metrics: Metrics,
    next_store: usize,
    generation: u64,
    dirty: bool,
    catalog_limited: bool,
    global_bytes: usize,
}
pub struct Catalog {
    state: Mutex<State>,
    stop: AtomicBool,
    worker: Mutex<Option<tokio::task::JoinHandle<()>>>,
    disabled: Vec<trace_core::native::Provider>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionWorkspace {
    pub state: String,
    pub session_id: String,
    pub run_id: Option<String>,
    pub snapshot_id: Option<String>,
    pub revision: u64,
    pub progress: NativeProgress,
    pub error: Option<NativeError>,
    pub source_revision: String,
    pub capabilities: trace_core::native::Capabilities,
    pub process_state: &'static str,
}
impl Analysis {
    fn active(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.requested) < Duration::from_secs(2)
    }
    fn workspace(&self, item: &CatalogItem) -> SessionWorkspace {
        SessionWorkspace {
            state: if item.ingestion_state == "unavailable" {
                if self.current.is_some() {
                    "stale"
                } else {
                    "failed"
                }
            } else if self.recovering {
                "stale"
            } else if self.error.is_some() {
                if self.current.is_some() {
                    "stale"
                } else {
                    "failed"
                }
            } else if self.current.is_some() {
                "ready"
            } else {
                "loading"
            }
            .into(),
            session_id: item.descriptor.id.to_string(),
            run_id: self.current.as_ref().map(|s| s.run_id().into()),
            snapshot_id: self.current.as_ref().map(|s| s.snapshot_id().into()),
            revision: self.revision,
            progress: self.progress,
            error: self.error.clone().or_else(|| {
                (item.ingestion_state == "unavailable").then(|| {
                    error(
                        "source_unavailable",
                        "The source could not be verified by the latest discovery sweep.",
                    )
                })
            }),
            source_revision: self.source_revision.clone(),
            capabilities: item.descriptor.capabilities.clone(),
            process_state: "unknown",
        }
    }
}
impl State {
    fn coverage(&self) -> Coverage {
        let mut limits = Vec::new();
        if self.catalog_limited {
            limits.push("catalog_entries".into());
        }
        if self.stores.iter().any(|s| {
            s.error
                .as_ref()
                .is_some_and(|e| e.code == "database_connection_limit")
        }) {
            limits.push("database_connections".into());
        }
        if self.stores.iter().any(|s| s.limited) {
            limits.push("discovery_entries".into());
        }
        Coverage {
            state: if !limits.is_empty() {
                "limited"
            } else if self
                .stores
                .iter()
                .any(|s| s.state != "current" || !s.sweep_complete)
            {
                "partial"
            } else {
                "complete"
            }
            .into(),
            omitted_count: if limits.is_empty()
                && self
                    .stores
                    .iter()
                    .all(|s| s.sweep_complete && s.state == "current")
            {
                Some(0)
            } else {
                None
            },
            limits,
            retained_entries: self.items.len(),
        }
    }
    fn publish(&mut self) {
        let coverage = self.coverage();
        if !self.dirty
            && self
                .revisions
                .back()
                .is_some_and(|r| r.coverage == coverage)
        {
            return;
        }
        let id = self.revisions.back().map_or(1, |r| r.id + 1);
        let mut items: Vec<_> = self.items.values().cloned().collect();
        items.sort_by(|a, b| {
            b.descriptor
                .activity_ms
                .cmp(&a.descriptor.activity_ms)
                .then_with(|| a.descriptor.id.cmp(&b.descriptor.id))
        });
        self.revisions.push_back(CatalogRevision {
            id,
            items,
            coverage,
        });
        while self.revisions.len() > 2 {
            self.revisions.pop_front();
        }
        self.dirty = false;
    }
    fn admit(&mut self, descriptor: SessionDescriptor, now: Instant) {
        let generation = self
            .stores
            .iter()
            .find(|s| s.id == descriptor.store_id)
            .map_or(0, |s| s.sweep_generation);
        let id = descriptor.id.to_string();
        if let Some(old) = self.items.get_mut(&id) {
            old.seen_generation = generation;
            if serde_json::to_value(&old.descriptor).ok() != serde_json::to_value(&descriptor).ok()
                || old.ingestion_state == "unavailable"
            {
                old.descriptor = descriptor;
                old.last_observed_ms = wall_time();
                old.ingestion_state = "discovered".into();
                self.dirty = true;
            }
            return;
        }
        if self.items.len() >= CATALOG_LIMIT {
            self.catalog_limited = true;
            let candidate = self
                .items
                .iter()
                .filter(|(id, _)| !self.analyses.get(*id).is_some_and(|a| a.active(now)))
                .min_by(|(aid, a), (bid, b)| {
                    a.descriptor
                        .activity_ms
                        .cmp(&b.descriptor.activity_ms)
                        .then_with(|| bid.cmp(aid))
                })
                .map(|(id, item)| (id.clone(), item.descriptor.activity_ms));
            let Some((cold, activity)) = candidate else {
                return;
            };
            if (descriptor.activity_ms, std::cmp::Reverse(&id))
                <= (activity, std::cmp::Reverse(&cold))
            {
                return;
            }
            self.items.remove(&cold);
        }
        self.items.insert(
            id,
            CatalogItem {
                descriptor,
                ingestion_state: "discovered".into(),
                last_observed_ms: wall_time(),
                process_state: "unknown",
                seen_generation: generation,
            },
        );
        self.dirty = true;
    }
    fn evict(&mut self, now: Instant) {
        self.analyses.retain(|_, a| {
            a.in_flight || now.saturating_duration_since(a.requested) < Duration::from_secs(60)
        });
    }
    fn ensure(&mut self, id: &str, now: Instant) -> Result<(), NativeError> {
        let item = self.items.get(id).ok_or_else(|| {
            error(
                "unknown_session",
                "The session is not in the configured catalog.",
            )
        })?;
        if let Some(a) = self.analyses.get_mut(id) {
            a.requested = now;
            return Ok(());
        }
        if item.ingestion_state == "unavailable" {
            return Err(error(
                "source_unavailable",
                "The session source is no longer available.",
            ));
        }
        self.evict(now);
        if self.analyses.len() >= 4 {
            let candidate = self
                .analyses
                .iter()
                .filter(|(_, a)| !a.in_flight && !a.active(now))
                .min_by_key(|(_, a)| a.requested)
                .map(|(id, _)| id.clone());
            match candidate {
                Some(id) => {
                    self.analyses.remove(&id);
                }
                None => {
                    return Err(error(
                        "analysis_capacity",
                        "Four sessions have active demand. Retry after 2000 ms.",
                    ));
                }
            }
        }
        self.generation += 1;
        let item = &self.items[id];
        self.analyses.insert(
            id.into(),
            Analysis {
                reader: None,
                current: None,
                previous: None,
                requested: now,
                next: now,
                bytes: 0,
                reader_bytes: 0,
                current_bytes: 0,
                previous_bytes: 0,
                recovering: false,
                reserved: 0,
                in_flight: false,
                generation: self.generation,
                revision: 0,
                progress: NativeProgress::CatchingUp,
                error: None,
                failures: 0,
                source_revision: item.descriptor.source_revision.to_string(),
                binding: source_binding(&item.descriptor),
            },
        );
        Ok(())
    }
}
fn source_binding(descriptor: &SessionDescriptor) -> String {
    use trace_core::native::NativeSource;
    match &descriptor.source {
        NativeSource::Json { source, .. } => format!("json:{source}"),
        NativeSource::Jsonl { source, .. } => format!("jsonl:{source}"),
        NativeSource::CompressedJsonl { source, .. } => format!("compressed:{source}"),
        NativeSource::Sqlite { database, .. } => format!("sqlite:{database}"),
    }
}
fn wall_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}
impl Catalog {
    pub fn new(config: SystemConfig) -> Arc<Self> {
        let now = Instant::now();
        Arc::new(Self {
            state: Mutex::new(State {
                stores: config
                    .stores
                    .into_iter()
                    .map(|config| Store {
                        id: StoreId::for_path(config.provider, &config.path),
                        config,
                        native: None,
                        state: "pending",
                        error: None,
                        next: now,
                        visited: 0,
                        pass_visited: 0,
                        sweep_complete: false,
                        sweep_generation: 0,
                        sweep_ids: BTreeSet::new(),
                        session_count: 0,
                        limited: false,
                        failures: 0,
                        scan_error: None,
                    })
                    .collect(),
                items: BTreeMap::new(),
                revisions: VecDeque::new(),
                analyses: BTreeMap::new(),
                metrics: Metrics::default(),
                next_store: 0,
                generation: 0,
                dirty: true,
                catalog_limited: false,
                global_bytes: GLOBAL_BYTES,
            }),
            stop: AtomicBool::new(false),
            worker: Mutex::new(None),
            disabled: config.disabled,
        })
    }
    #[cfg(test)]
    pub(crate) fn test_discover(&self) {
        while self.discovery() {}
    }
    #[cfg(test)]
    pub(crate) fn test_due(&self) {
        for a in self.state.lock().unwrap().analyses.values_mut() {
            a.next = Instant::now();
        }
    }
    #[cfg(test)]
    pub(crate) fn test_parse(&self) {
        while let Some(job) = self.job() {
            self.parse(job);
        }
    }
    pub fn begin(self: &Arc<Self>) {
        let mut worker = self.worker.lock().unwrap();
        if worker.is_some() {
            return;
        }
        let this = self.clone();
        *worker = Some(tokio::spawn(async move {
            let mut jobs = tokio::task::JoinSet::new();
            loop {
                if this.stop.load(Ordering::Acquire) {
                    break;
                }
                while jobs.try_join_next().is_some() {}
                let this2 = this.clone();
                let backlog = tokio::task::spawn_blocking(move || this2.discovery())
                    .await
                    .unwrap_or(false);
                for _ in 0..2 {
                    if jobs.len() >= 2 {
                        break;
                    }
                    if let Some(job) = this.job() {
                        let this = this.clone();
                        jobs.spawn_blocking(move || this.parse(job));
                    } else {
                        break;
                    }
                }
                if backlog {
                    tokio::task::yield_now().await;
                } else if !jobs.is_empty() {
                    jobs.join_next().await;
                } else {
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            }
            while jobs.join_next().await.is_some() {}
        }));
    }
    pub async fn shutdown(&self) {
        self.stop.store(true, Ordering::Release);
        let worker = self.worker.lock().unwrap().take();
        if let Some(worker) = worker {
            let _ = worker.await;
        }
    }
    fn discovery(&self) -> bool {
        let now = Instant::now();
        let mut state = self.state.lock().unwrap();
        state.evict(now);
        let count = state.stores.len();
        let index = (0..count)
            .map(|i| (state.next_store + i) % count)
            .find(|i| state.stores[*i].next <= now);
        if let Some(index) = index {
            state.next_store = (index + 1) % count;
            state.metrics.discovery_batches += 1;
            let connections: usize = state
                .stores
                .iter()
                .filter_map(|s| s.native.as_ref())
                .map(NativeStore::database_connections)
                .sum();
            let store = &mut state.stores[index];
            if store.native.is_none() {
                match NativeStore::open(
                    store.config.provider,
                    &store.config.path,
                    NativeLimits::default(),
                ) {
                    Ok(native) if native.root() != store.config.path => {
                        store.state = "unavailable";
                        store.error = Some(error(
                            "source_replaced",
                            "The configured root binding changed.",
                        ));
                        store.next = now + Duration::from_secs(5);
                    }
                    Ok(native) => {
                        store.native = Some(native);
                        store.state = "indexing";
                        store.error = None;
                    }
                    Err(e) => {
                        store.state = if store.config.path.exists() {
                            "unavailable"
                        } else {
                            "absent"
                        };
                        store.error = Some(e);
                        store.next = now + Duration::from_secs(5);
                    }
                }
            }
            if let Some(native) = &mut store.native {
                native.set_database_capacity(
                    native.database_connections() + 16usize.saturating_sub(connections),
                );
            }
            let batch = store
                .native
                .as_mut()
                .map(|s| s.discover(BATCH.min(PASS - store.pass_visited)));
            if let Some(batch) = batch {
                match batch {
                    Ok(batch) => {
                        store.failures = 0;
                        store.visited += batch.visited;
                        store.pass_visited += batch.visited;
                        for d in &batch.sessions {
                            if store.sweep_ids.len() <= CATALOG_LIMIT {
                                store.sweep_ids.insert(d.id.to_string());
                            }
                        }
                        if let Some(d) = batch.diagnostics.first() {
                            store.scan_error = Some(Box::new(trace_core::views::StructuralError {
                                code: d.code.clone(),
                                message: d.message.clone(),
                                source: d.source.clone(),
                            }));
                        }
                        if batch.sweep_complete {
                            store.error = store.scan_error.take();
                        } else if store.scan_error.is_some() {
                            store.error = store.scan_error.clone();
                        }
                        store.state = if store.error.is_some() {
                            "partial"
                        } else if batch.sweep_complete || store.sweep_complete {
                            "current"
                        } else {
                            "indexing"
                        };
                        let removed = if batch.sweep_complete {
                            let store_id = store.id.clone();
                            let generation = store.sweep_generation;
                            store.session_count = store.sweep_ids.len();
                            store.sweep_ids.clear();
                            store.sweep_complete = true;
                            store.pass_visited = 0;
                            store.limited = false;
                            store.next = now + Duration::from_secs(1);
                            Some((
                                store_id,
                                generation,
                                store.error.is_none() && !batch.limited,
                            ))
                        } else {
                            if store.pass_visited >= PASS {
                                store.pass_visited = 0;
                                store.limited = true;
                                store.next = now + Duration::from_secs(1);
                            } else {
                                store.next = now;
                            }
                            None
                        };
                        for descriptor in batch.sessions {
                            state.admit(descriptor, now);
                        }
                        if let Some((store_id, generation, trustworthy)) = removed {
                            state.stores[index].sweep_generation += 1;
                            let ids: Vec<_> = state
                                .items
                                .iter()
                                .filter(|(_, i)| {
                                    i.descriptor.store_id == store_id
                                        && i.seen_generation != generation
                                })
                                .map(|(id, _)| id.clone())
                                .collect();
                            for id in ids {
                                if !trustworthy || state.analyses.contains_key(&id) {
                                    let item = state.items.get_mut(&id).unwrap();
                                    if item.ingestion_state != "unavailable" {
                                        item.ingestion_state = "unavailable".into();
                                        state.dirty = true;
                                    }
                                } else {
                                    state.items.remove(&id);
                                    state.dirty = true;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        store.failures += 1;
                        store.scan_error = Some(e.clone());
                        store.error = Some(e);
                        store.state = "unavailable";
                        store.next = now
                            + Duration::from_secs(
                                (1u64 << store.failures.saturating_sub(1).min(3)).min(5),
                            );
                    }
                }
            }
        }
        if state.stores.iter().all(|s| s.sweep_complete) {
            let known: usize = state.stores.iter().map(|s| s.session_count).sum();
            let present = state
                .items
                .values()
                .filter(|i| i.ingestion_state != "unavailable")
                .count();
            state.catalog_limited = known > CATALOG_LIMIT || known > present;
        }
        state.publish();
        state.stores.iter().any(|s| s.next <= now)
    }
    pub fn status(&self) -> Value {
        let mut s = self.state.lock().unwrap();
        s.publish();
        s.metrics.database_connections = s
            .stores
            .iter()
            .filter_map(|s| s.native.as_ref())
            .map(NativeStore::database_connections)
            .sum();
        s.metrics.loaded_entries = s.analyses.len();
        s.metrics.retained_bytes = s.analyses.values().map(|a| a.bytes).sum();
        json!({"mode":"system","catalogRevision":s.revisions.back().unwrap().id.to_string(),"stores":s.stores.iter().map(|v|json!({"id":v.id,"provider":v.config.provider,"root":v.config.path,"state":v.state,"error":v.error,"visited":v.visited,"sweepComplete":v.sweep_complete})).collect::<Vec<_>>(),"disabledProviders":self.disabled,"coverage":s.coverage(),"limits":{"stores":16,"catalogEntries":CATALOG_LIMIT,"discoveryEntriesPerPass":PASS,"discoveryBatch":BATCH,"catalogPage":100,"catalogRevisions":2,"analyses":4,"activeDemandMs":2000,"idleEvictionMs":60000,"parseJobs":2,"databaseConnections":16,"batchBytes":1048576,"batchRecords":2000,"retainedDecodedBytes":s.global_bytes,"sessionDecodedBytes":134217728,"sessionRecords":100000,"recordBytes":1048576,"relatedSources":1024,"compressedBytes":268435456},"metrics":s.metrics})
    }
    pub fn page(
        &self,
        provider: Option<&str>,
        project: Option<&str>,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<Value, NativeError> {
        if !(1..=100).contains(&limit) {
            return Err(error(
                "invalid_query",
                "Catalog page limit must be between 1 and 100.",
            ));
        }
        let mut s = self.state.lock().unwrap();
        s.publish();
        let filter = serde_json::to_string(&(provider, project)).unwrap();
        let fingerprint = filter.bytes().fold(0xcbf29ce484222325u64, |h, b| {
            (h ^ b as u64).wrapping_mul(0x100000001b3)
        });
        let (revision, offset) = match cursor {
            None => (s.revisions.back().unwrap().id, 0),
            Some(c) => {
                let (prefix, f) = c
                    .rsplit_once(':')
                    .ok_or_else(|| error("invalid_cursor", "Invalid catalog cursor."))?;
                if f != format!("{fingerprint:016x}") {
                    return Err(error(
                        "invalid_cursor",
                        "The cursor belongs to different filters.",
                    ));
                }
                let (r, n) = prefix
                    .split_once(':')
                    .ok_or_else(|| error("invalid_cursor", "Invalid catalog cursor."))?;
                (
                    r.parse::<u64>()
                        .map_err(|_| error("invalid_cursor", "Invalid catalog cursor."))?,
                    n.parse::<usize>()
                        .map_err(|_| error("invalid_cursor", "Invalid catalog cursor."))?,
                )
            }
        };
        let revision = s
            .revisions
            .iter()
            .find(|r| r.id == revision)
            .ok_or_else(|| {
                error(
                    "expired_catalog_revision",
                    "The catalog revision is no longer retained.",
                )
            })?;
        let items: Vec<_> = revision
            .items
            .iter()
            .filter(|i| {
                provider.is_none_or(|p| i.descriptor.provider.as_str() == p)
                    && project.is_none_or(|p| i.descriptor.project.as_deref() == Some(p))
            })
            .collect();
        if offset > items.len() {
            return Err(error(
                "invalid_cursor",
                "The catalog cursor is outside this result.",
            ));
        }
        let end = offset.saturating_add(limit).min(items.len());
        Ok(
            json!({"revision":revision.id.to_string(),"items":&items[offset..end],"nextCursor":(end<items.len()).then(||format!("{}:{end}:{fingerprint:016x}",revision.id)),"coverage":revision.coverage}),
        )
    }
    pub fn workspace(&self, id: &str) -> Result<SessionWorkspace, NativeError> {
        if self.stop.load(Ordering::Acquire) {
            return Err(error(
                "workspace_closing",
                "The system workspace is shutting down.",
            ));
        }
        let mut s = self.state.lock().unwrap();
        s.ensure(id, Instant::now())?;
        Ok(s.analyses[id].workspace(&s.items[id]))
    }
    pub fn snapshot(&self, id: &str, snapshot: &str) -> Result<Arc<SessionSnapshot>, NativeError> {
        let mut s = self.state.lock().unwrap();
        if !s.items.contains_key(id) {
            return Err(error(
                "unknown_session",
                "The session is not in the configured catalog.",
            ));
        }
        let a = s
            .analyses
            .get_mut(id)
            .ok_or_else(|| error("unknown_snapshot", "The analysis was evicted."))?;
        a.requested = Instant::now();
        a.current
            .iter()
            .chain(a.previous.iter())
            .find(|v| v.snapshot_id() == snapshot)
            .cloned()
            .ok_or_else(|| {
                error(
                    "unknown_snapshot",
                    "The immutable session snapshot is not retained.",
                )
            })
    }
    pub fn retry(&self, id: &str) -> Result<SessionWorkspace, NativeError> {
        if self.stop.load(Ordering::Acquire) {
            return Err(error(
                "workspace_closing",
                "The system workspace is shutting down.",
            ));
        }
        let mut s = self.state.lock().unwrap();
        let now = Instant::now();
        s.ensure(id, now)?;
        let a = s.analyses.get_mut(id).unwrap();
        if a.in_flight || a.error.is_none() {
            return Err(error(
                "analysis_busy",
                "The session is already following or loading.",
            ));
        }
        a.reader = None;
        a.reader_bytes = 0;
        a.previous_bytes = 0;
        a.recovering = a.current.is_some();
        a.bytes = a.current_bytes;
        a.error = None;
        a.previous = None;
        a.progress = NativeProgress::CatchingUp;
        a.next = now;
        a.failures = 0;
        Ok(s.analyses[id].workspace(&s.items[id]))
    }
    fn job(&self) -> Option<Job> {
        let now = Instant::now();
        let mut s = self.state.lock().unwrap();
        if s.metrics.active_parse_jobs >= 2 {
            return None;
        }
        let id = s
            .analyses
            .iter()
            .filter(|(id, a)| {
                s.items
                    .get(*id)
                    .is_some_and(|i| i.ingestion_state != "unavailable")
                    && !a.in_flight
                    && a.active(now)
                    && a.next <= now
                    && !a.error.as_ref().is_some_and(|e| {
                        matches!(
                            e.code.as_str(),
                            "decoded_limit" | "record_limit" | "compressed_limit" | "source_limit"
                        )
                    })
            })
            .min_by_key(|(_, a)| a.next)
            .map(|(id, _)| id.clone())?;

        let descriptor = s.items.get(&id)?.descriptor.clone();
        let binding = source_binding(&descriptor);
        let analysis = s.analyses.get_mut(&id).unwrap();
        if analysis.binding != binding {
            analysis.reader = None;
            analysis.reader_bytes = 0;
            analysis.previous_bytes = 0;
            analysis.recovering = analysis.current.is_some();
            analysis.bytes = analysis.current_bytes;
            analysis.previous = None;
            analysis.binding = binding;
        }
        let needs_reader = analysis.reader.is_none();
        if needs_reader && analysis.current.is_some() {
            analysis.recovering = true;
        }
        if needs_reader {
            let result = s
                .stores
                .iter_mut()
                .find(|v| v.id == descriptor.store_id)
                .and_then(|v| v.native.as_mut())
                .ok_or_else(|| error("source_unavailable", "The configured store is unavailable."))
                .and_then(|v| v.reader(&descriptor));
            match result {
                Ok(reader) => s.analyses.get_mut(&id).unwrap().reader = Some(reader),
                Err(e) => {
                    let a = s.analyses.get_mut(&id).unwrap();
                    a.error = Some(e);
                    a.next = now + Duration::from_secs(5);
                    return None;
                }
            }
        }
        if !needs_reader && s.analyses[&id].progress != NativeProgress::CatchingUp {
            let unchanged = s
                .stores
                .iter_mut()
                .find(|store| store.id == descriptor.store_id)
                .and_then(|store| store.native.as_mut())
                .and_then(|store| store.recheck(&descriptor).ok())
                .is_some_and(|revision| revision.as_str() == s.analyses[&id].source_revision);
            if unchanged && s.analyses[&id].error.is_none() {
                s.analyses.get_mut(&id).unwrap().next = now + Duration::from_millis(250);
                return None;
            }
        }
        let used: usize = s.analyses.values().map(|a| a.bytes + a.reserved).sum();
        let available = s.global_bytes.saturating_sub(used);
        // Publication retains the old current snapshot and retires the old previous snapshot.
        let projection_headroom = s.analyses[&id]
            .reader_bytes
            .saturating_sub(s.analyses[&id].previous_bytes);
        if available < projection_headroom {
            let a = s.analyses.get_mut(&id).unwrap();
            a.error = Some(error(
                "global_decoded_limit",
                "Retained snapshots and reader data exhaust the decoded budget.",
            ));
            a.next = now + Duration::from_secs(1);
            return None;
        }
        let reservation = ((available - projection_headroom) / 2).min(1024 * 1024);
        let reserved = projection_headroom + 2 * reservation;
        let a = s.analyses.get_mut(&id).unwrap();
        a.in_flight = true;
        a.reserved = reserved;
        let job = Job {
            id,
            reader: a.reader.take().unwrap(),
            generation: a.generation,
            reservation,
        };
        s.metrics.parse_jobs += 1;
        s.metrics.active_parse_jobs += 1;
        s.metrics.max_active_parse_jobs = s
            .metrics
            .max_active_parse_jobs
            .max(s.metrics.active_parse_jobs);
        Some(job)
    }
    fn parse(&self, mut job: Job) {
        let before = (job.reader.read_records(), job.reader.read_bytes());
        let result = job.reader.poll_bounded(job.reservation);
        let read_records = job.reader.read_records().saturating_sub(before.0) as u64;
        let read_bytes = job.reader.read_bytes().saturating_sub(before.1) as u64;
        let now = Instant::now();
        let mut s = self.state.lock().unwrap();
        s.metrics.active_parse_jobs = s.metrics.active_parse_jobs.saturating_sub(1);
        s.metrics.read_records += read_records;
        s.metrics.read_bytes += read_bytes;
        let global_bytes = s.global_bytes;
        let other: usize = s
            .analyses
            .iter()
            .filter(|(id, _)| **id != job.id)
            .map(|(_, a)| a.bytes + a.reserved)
            .sum();
        let Some(a) = s
            .analyses
            .get_mut(&job.id)
            .filter(|a| a.generation == job.generation)
        else {
            return;
        };
        a.in_flight = false;
        a.reserved = 0;
        a.reader = Some(job.reader);
        match result {
            Ok(update) => {
                a.reader_bytes = update.retained_decoded_bytes;
                a.source_revision = update.source_revision.to_string();
                a.progress = update.progress;
                a.error = None;
                a.failures = 0;
                if let Some(snapshot) = update.snapshot {
                    let reset = update.reset
                        || a.current
                            .as_ref()
                            .is_some_and(|old| old.run_id() != snapshot.run_id());
                    a.previous_bytes = if reset { 0 } else { a.current_bytes };
                    a.current_bytes = update.retained_decoded_bytes;
                    a.recovering = false;
                    a.previous = if update.reset
                        || a.current
                            .as_ref()
                            .is_some_and(|old| old.run_id() != snapshot.run_id())
                    {
                        None
                    } else {
                        a.current.take()
                    };
                    a.current = Some(snapshot);
                    a.revision += 1;
                }
                a.bytes = a.reader_bytes + a.current_bytes + a.previous_bytes;
                assert!(
                    a.bytes + other <= global_bytes,
                    "retained decoded reservation must cover publication"
                );
                a.next = now
                    + if a.progress == NativeProgress::CatchingUp {
                        Duration::ZERO
                    } else {
                        Duration::from_millis(250)
                    };
            }
            Err(mut e) => {
                if e.code == "decoded_limit" && job.reservation < 1024 * 1024 {
                    e = error(
                        "global_decoded_limit",
                        "Global decoded retention capacity is exhausted.",
                    );
                }
                if matches!(
                    e.code.as_str(),
                    "decoded_limit"
                        | "record_limit"
                        | "compressed_limit"
                        | "source_limit"
                        | "global_decoded_limit"
                ) {
                    a.reader = None;
                    a.reader_bytes = 0;
                } else {
                    a.reader_bytes = a
                        .reader_bytes
                        .saturating_add((read_bytes as usize).min(job.reservation));
                }
                a.bytes = a.reader_bytes + a.current_bytes + a.previous_bytes;
                a.failures += 1;
                a.error = Some(e);
                a.next =
                    now + Duration::from_secs((1u64 << a.failures.saturating_sub(1).min(3)).min(5));
            }
        }
    }
}
struct Job {
    id: String,
    reader: SessionReader,
    generation: u64,
    reservation: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use trace_core::native::Provider;
    fn fixture(count: usize) -> (tempfile::TempDir, Arc<Catalog>) {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..count {
            std::fs::write(dir.path().join(format!("{i}.jsonl")),format!("{{\"type\":\"session\",\"version\":3,\"id\":\"fixture-{i}\",\"cwd\":\"/project\"}}\n")).unwrap();
        }
        let c = Catalog::new(SystemConfig {
            stores: vec![StoreConfig {
                provider: Provider::Pi,
                path: dir.path().into(),
            }],
            disabled: vec![],
        });
        while c.discovery() {}
        (dir, c)
    }
    fn descriptor(template: &SessionDescriptor, i: usize) -> SessionDescriptor {
        let mut d = template.clone();
        d.id = serde_json::from_value(json!(format!("id-{i:05}"))).unwrap();
        d.native_id = format!("native-{i}");
        d.recording_id = d.native_id.clone();
        d.activity_ms = Some(i as u64);
        d
    }
    #[test]
    fn retention_active_capacity_inactive_lru_and_idle_clock() {
        let (_dir, c) = fixture(5);
        let ids: Vec<_> = c.state.lock().unwrap().items.keys().cloned().collect();
        let now = Instant::now();
        let mut s = c.state.lock().unwrap();
        for id in &ids[..4] {
            s.ensure(id, now).unwrap();
        }
        assert_eq!(
            s.ensure(&ids[4], now).unwrap_err().code,
            "analysis_capacity"
        );
        assert_eq!(s.analyses.len(), 4);
        s.analyses.get_mut(&ids[1]).unwrap().requested = now - Duration::from_secs(3);
        s.ensure(&ids[4], now).unwrap();
        assert!(!s.analyses.contains_key(&ids[1]));
        assert_eq!(s.items.len(), 5);
        s.evict(now + Duration::from_secs(60));
        assert!(s.analyses.is_empty());
        assert_eq!(s.items.len(), 5);
    }
    #[test]
    fn exact_catalog_limit_recent_admission_keeps_investigated_and_identity() {
        let (_dir, c) = fixture(1);
        let now = Instant::now();
        let mut s = c.state.lock().unwrap();
        let template = s.items.values().next().unwrap().descriptor.clone();
        s.items.clear();
        for i in 0..CATALOG_LIMIT {
            s.admit(descriptor(&template, i), now);
        }
        assert_eq!(s.items.len(), CATALOG_LIMIT);
        assert!(!s.catalog_limited);
        s.ensure("id-00000", now).unwrap();
        s.admit(descriptor(&template, CATALOG_LIMIT), now);
        assert_eq!(s.items.len(), CATALOG_LIMIT);
        assert!(s.items.contains_key("id-00000"));
        assert!(!s.items.contains_key("id-00001"));
        assert!(s.catalog_limited);
        s.admit(descriptor(&template, 1), now);
        assert!(!s.items.contains_key("id-00001"));
        s.items.remove("id-00002");
        s.admit(descriptor(&template, 1), now);
        assert!(s.items.contains_key("id-00001"));
        assert_eq!(s.coverage().omitted_count, None);
    }
    #[test]
    fn stable_filtered_pages_two_revisions_and_expired_cursor() {
        let (_dir, c) = fixture(3);
        let page = c.page(Some("pi"), Some("/project"), None, 1).unwrap();
        let cursor = page["nextCursor"].as_str().unwrap();
        assert!(c.page(None, None, Some(cursor), 1).is_err());
        let next = c
            .page(Some("pi"), Some("/project"), Some(cursor), 1)
            .unwrap();
        assert_ne!(page["items"][0]["id"], next["items"][0]["id"]);
        {
            let mut s = c.state.lock().unwrap();
            s.dirty = true;
            s.publish();
        }
        assert!(
            c.page(Some("pi"), Some("/project"), Some(cursor), 1)
                .is_ok()
        );
        {
            let mut s = c.state.lock().unwrap();
            s.dirty = true;
            s.publish();
        }
        assert_eq!(
            c.page(Some("pi"), Some("/project"), Some(cursor), 1)
                .unwrap_err()
                .code,
            "expired_catalog_revision"
        );
    }
    #[test]
    fn idle_scans_do_not_revise_catalog_and_removal_is_deterministic() {
        let (dir, c) = fixture(3);
        let revision = c.status()["catalogRevision"].clone();
        for _ in 0..3 {
            c.state.lock().unwrap().stores[0].next = Instant::now();
            while c.discovery() {}
        }
        assert_eq!(c.status()["catalogRevision"], revision);
        std::fs::remove_file(dir.path().join("0.jsonl")).unwrap();
        c.state.lock().unwrap().stores[0].next = Instant::now();
        while c.discovery() {}
        assert_eq!(c.state.lock().unwrap().items.len(), 2);
    }
    #[test]
    fn discovery_is_lazy_bounded_and_absent_store_recovers() {
        let (dir, c) = fixture(600);
        let status = c.status();
        assert_eq!(status["metrics"]["parseJobs"], 0);
        assert!(status["metrics"]["discoveryBatches"].as_u64().unwrap() >= 2);
        assert_eq!(status["coverage"]["retainedEntries"], 600);
        let absent = dir.path().join("later");
        let other = Catalog::new(SystemConfig {
            stores: vec![StoreConfig {
                provider: Provider::Pi,
                path: absent.clone(),
            }],
            disabled: vec![],
        });
        other.discovery();
        assert_eq!(other.status()["stores"][0]["state"], "absent");
        std::fs::create_dir(&absent).unwrap();
        std::fs::write(
            absent.join("new.jsonl"),
            "{\"type\":\"session\",\"version\":3,\"id\":\"new\"}\n",
        )
        .unwrap();
        other.state.lock().unwrap().stores[0].next = Instant::now();
        while other.discovery() {}
        assert_eq!(other.status()["coverage"]["retainedEntries"], 1);
    }
    #[test]
    fn pass_limit_rotates_instead_of_restarting() {
        let (_dir, c) = fixture(600);
        {
            let mut s = c.state.lock().unwrap();
            s.stores[0].next = Instant::now();
            s.stores[0].pass_visited = PASS - 1;
        }
        c.discovery();
        assert!(c.state.lock().unwrap().stores[0].limited);
        assert_eq!(
            c.status()["coverage"]["limits"],
            json!(["discovery_entries"])
        );
        {
            let mut s = c.state.lock().unwrap();
            s.stores[0].next = Instant::now();
        }
        while c.discovery() {}
        assert_eq!(c.state.lock().unwrap().items.len(), 600);
        assert!(c.state.lock().unwrap().stores[0].sweep_complete);
    }
    fn force_next(c: &Catalog, id: &str) {
        c.state.lock().unwrap().analyses.get_mut(id).unwrap().next = Instant::now();
    }
    #[test]
    fn actual_read_counters_idle_polls_snapshot_retirement_and_retry() {
        use std::io::Write;
        let (dir, c) = fixture(1);
        let id = c.state.lock().unwrap().items.keys().next().unwrap().clone();
        c.workspace(&id).unwrap();
        c.test_parse();
        let first = c.workspace(&id).unwrap();
        let first_id = first.snapshot_id.unwrap();
        let records = c.status()["metrics"]["readRecords"].clone();
        let bytes = c.status()["metrics"]["readBytes"].clone();
        force_next(&c, &id);
        c.test_parse();
        assert_eq!(c.status()["metrics"]["readRecords"], records);
        assert_eq!(c.status()["metrics"]["readBytes"], bytes);
        assert_eq!(
            c.workspace(&id).unwrap().snapshot_id.as_deref(),
            Some(first_id.as_str())
        );
        for i in 0..2 {
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(dir.path().join("0.jsonl"))
                .unwrap();
            writeln!(file,"{{\"type\":\"message\",\"id\":\"m{i}\",\"message\":{{\"role\":\"user\",\"content\":\"fabricated body\"}}}}").unwrap();
            force_next(&c, &id);
            c.test_parse();
        }
        assert_eq!(c.status()["metrics"]["readRecords"], 3);
        assert!(matches!(c.snapshot(&id,&first_id),Err(e) if e.code=="unknown_snapshot"));
        let current = c.workspace(&id).unwrap();
        let retained = current.snapshot_id.unwrap();
        std::fs::remove_file(dir.path().join("0.jsonl")).unwrap();
        force_next(&c, &id);
        c.test_parse();
        assert_eq!(c.workspace(&id).unwrap().state, "stale");
        assert!(c.snapshot(&id, &retained).is_ok());
        std::fs::write(
            dir.path().join("0.jsonl"),
            "{\"type\":\"session\",\"version\":3,\"id\":\"fixture-0\",\"cwd\":\"/project\"}\n",
        )
        .unwrap();
        c.retry(&id).unwrap();
        c.test_parse();
        let fresh = c.workspace(&id).unwrap();
        assert_ne!(current.run_id, fresh.run_id);
        assert!(matches!(c.snapshot(&id,&retained),Err(e) if e.code=="unknown_snapshot"));
    }
    #[test]
    fn parse_concurrency_and_global_reservations_precede_io() {
        let (_dir, c) = fixture(3);
        let ids: Vec<_> = c.state.lock().unwrap().items.keys().cloned().collect();
        for id in &ids {
            c.workspace(id).unwrap();
        }
        {
            let mut s = c.state.lock().unwrap();
            s.global_bytes = 50;
        }
        let first = c.job().unwrap();
        assert_eq!(first.reservation, 25);
        let second = c.job().unwrap();
        assert_eq!(second.reservation, 0);
        assert!(c.job().is_none());
        {
            let s = c.state.lock().unwrap();
            assert_eq!(
                s.analyses
                    .values()
                    .map(|a| a.bytes + a.reserved)
                    .sum::<usize>(),
                50
            );
            assert_eq!(s.metrics.active_parse_jobs, 2);
        }
        c.parse(first);
        c.parse(second);
        assert_eq!(c.status()["metrics"]["maxActiveParseJobs"], 2);
        assert!(c.status()["metrics"]["retainedBytes"].as_u64().unwrap() <= 50);
    }
    #[test]
    fn database_connection_capacity_is_global_across_stores() {
        let dir = tempfile::tempdir().unwrap();
        let mut stores = Vec::new();
        for root in ["a", "b"] {
            let path = dir.path().join(root);
            std::fs::create_dir(&path).unwrap();
            for n in 0..10 {
                let conn =
                    rusqlite::Connection::open(path.join(format!("opencode-{n}.db"))).unwrap();
                conn.execute_batch("CREATE TABLE session_v2(id TEXT PRIMARY KEY,directory TEXT,time_updated INTEGER,fork_session_id TEXT,fork_boundary TEXT);CREATE TABLE session_message(id TEXT PRIMARY KEY,session_id TEXT,type TEXT,seq INTEGER,data TEXT,time_updated INTEGER);CREATE TABLE event(id TEXT PRIMARY KEY,aggregate_id TEXT,seq INTEGER,type TEXT,data TEXT);").unwrap();
            }
            stores.push(StoreConfig {
                provider: Provider::Opencode,
                path,
            });
        }
        let c = Catalog::new(SystemConfig {
            stores,
            disabled: vec![],
        });
        c.test_discover();
        let status = c.status();
        assert_eq!(status["metrics"]["databaseConnections"], 16);
        assert!(
            status["stores"]
                .as_array()
                .unwrap()
                .iter()
                .any(|s| s["error"]["code"] == "database_connection_limit")
        );
        assert_ne!(status["coverage"]["state"], "complete");
    }
    #[test]
    fn failed_sweep_retains_unseen_descriptors_until_clean_absence() {
        let (dir, c) = fixture(600);
        let initial: Vec<_> = c
            .state
            .lock()
            .unwrap()
            .items
            .values()
            .map(|i| (i.descriptor.native_id.clone(), i.descriptor.id.to_string()))
            .collect();
        let selected = initial
            .iter()
            .find(|(native, _)| native == "fixture-1")
            .unwrap()
            .1
            .clone();
        let removed = initial
            .iter()
            .find(|(native, _)| native == "fixture-0")
            .unwrap()
            .1
            .clone();
        c.workspace(&selected).unwrap();
        c.test_parse();
        let snapshot = c.workspace(&selected).unwrap().snapshot_id.unwrap();
        for i in 0..599 {
            std::fs::write(
                dir.path().join(format!("{i}.jsonl")),
                "temporarily incomplete header\n",
            )
            .unwrap();
        }
        c.state.lock().unwrap().stores[0].next = Instant::now();
        assert!(c.discovery());
        assert!(c.state.lock().unwrap().stores[0].scan_error.is_some());
        while c.discovery() {}
        assert_eq!(c.state.lock().unwrap().items.len(), 600);
        assert_eq!(c.status()["coverage"]["state"], "partial");
        assert_eq!(
            c.state.lock().unwrap().items[&removed].ingestion_state,
            "unavailable"
        );
        assert_eq!(c.workspace(&selected).unwrap().state, "stale");
        assert!(c.snapshot(&selected, &snapshot).is_ok());
        for i in 0..599 {
            std::fs::write(dir.path().join(format!("{i}.jsonl")),format!("{{\"type\":\"session\",\"version\":3,\"id\":\"fixture-{i}\",\"cwd\":\"/project\"}}\n")).unwrap();
        }
        c.state.lock().unwrap().stores[0].next = Instant::now();
        while c.discovery() {}
        assert_eq!(
            c.state.lock().unwrap().items[&removed].ingestion_state,
            "discovered"
        );
        assert_eq!(c.status()["coverage"]["state"], "complete");
        std::fs::remove_file(dir.path().join("0.jsonl")).unwrap();
        c.state.lock().unwrap().stores[0].next = Instant::now();
        while c.discovery() {}
        assert!(!c.state.lock().unwrap().items.contains_key(&removed));
        assert_eq!(c.state.lock().unwrap().items.len(), 599);
    }
    #[test]
    fn restored_unchanged_source_clears_transient_failure_without_new_snapshot() {
        let (dir, c) = fixture(1);
        let id = c.state.lock().unwrap().items.keys().next().unwrap().clone();
        c.workspace(&id).unwrap();
        c.test_parse();
        let before = c.workspace(&id).unwrap();
        let path = dir.path().join("0.jsonl");
        let removed = dir.path().join("0.removed");
        std::fs::rename(&path, &removed).unwrap();
        force_next(&c, &id);
        c.test_parse();
        let stale = c.workspace(&id).unwrap();
        assert_eq!(stale.state, "stale");
        assert!(stale.error.is_some());
        std::fs::rename(&removed, &path).unwrap();
        force_next(&c, &id);
        c.test_parse();
        let recovered = c.workspace(&id).unwrap();
        assert_eq!(recovered.state, "ready");
        assert!(recovered.error.is_none());
        assert_eq!(recovered.run_id, before.run_id);
        assert_eq!(recovered.snapshot_id, before.snapshot_id);
        assert_eq!(recovered.revision, before.revision);
    }
    #[test]
    fn database_previous_snapshot_remains_charged_after_large_to_small_update() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("opencode.db");
        let writer = rusqlite::Connection::open(&path).unwrap();
        writer.execute_batch("CREATE TABLE session_v2(id TEXT PRIMARY KEY,directory TEXT,time_updated INTEGER,fork_session_id TEXT,fork_boundary TEXT);CREATE TABLE session_message(id TEXT PRIMARY KEY,session_id TEXT,type TEXT,seq INTEGER,data TEXT,time_updated INTEGER);CREATE TABLE event(id TEXT PRIMARY KEY,aggregate_id TEXT,seq INTEGER,type TEXT,data TEXT);").unwrap();
        for id in ["main", "other-a", "other-b"] {
            writer
                .execute(
                    "INSERT INTO session_v2 VALUES(?1,'/synthetic',1,NULL,NULL)",
                    [id],
                )
                .unwrap();
            let size = if id == "main" { 400 * 1024 } else { 4096 };
            let data = json!({"content":[{"type":"text","text":"x".repeat(size)}]}).to_string();
            writer
                .execute(
                    "INSERT INTO session_message VALUES(?1,?1,'user',1,?2,1)",
                    rusqlite::params![id, data],
                )
                .unwrap();
        }
        let c = Catalog::new(SystemConfig {
            stores: vec![StoreConfig {
                provider: Provider::Opencode,
                path: path.clone(),
            }],
            disabled: vec![],
        });
        c.state.lock().unwrap().global_bytes = 6 * 1024 * 1024;
        c.test_discover();
        let ids: BTreeMap<_, _> = c
            .state
            .lock()
            .unwrap()
            .items
            .values()
            .map(|i| (i.descriptor.native_id.clone(), i.descriptor.id.to_string()))
            .collect();
        let id = &ids["main"];
        c.workspace(id).unwrap();
        c.test_parse();
        let large = c.workspace(id).unwrap();
        let large_id = large.snapshot_id.unwrap();
        let old_charge = c.state.lock().unwrap().analyses[id].current_bytes;
        assert!(old_charge >= 400 * 1024);
        writer
            .execute(
                "UPDATE session_message SET data=?1 WHERE id='main'",
                [json!({"content":[{"type":"text","text":"small"}]}).to_string()],
            )
            .unwrap();
        force_next(&c, id);
        c.test_parse();
        let small = c.workspace(id).unwrap();
        assert_eq!(large.run_id, small.run_id);
        assert_ne!(small.snapshot_id.as_deref(), Some(large_id.as_str()));
        assert!(c.snapshot(id, &large_id).is_ok());
        {
            let s = c.state.lock().unwrap();
            let a = &s.analyses[id];
            assert_eq!(a.previous_bytes, old_charge);
            assert!(a.current_bytes < 1024);
            assert_eq!(a.bytes, a.reader_bytes + a.current_bytes + old_charge);
        }
        for other in [&ids["other-a"], &ids["other-b"]] {
            c.workspace(other).unwrap();
        }
        let first = c.job().unwrap();
        let second = c.job().unwrap();
        assert_eq!(first.reservation, 1024 * 1024);
        assert_eq!(second.reservation, 1024 * 1024);
        {
            let s = c.state.lock().unwrap();
            assert!(
                s.analyses
                    .values()
                    .map(|a| a.bytes + a.reserved)
                    .sum::<usize>()
                    <= s.global_bytes
            );
            assert_eq!(s.analyses[id].previous_bytes, old_charge);
        }
        c.parse(first);
        c.parse(second);
        c.test_parse();
        writer
            .execute(
                "UPDATE session_message SET data=?1 WHERE id='main'",
                [json!({"content":[{"type":"text","text":"next"}]}).to_string()],
            )
            .unwrap();
        force_next(&c, id);
        c.test_parse();
        assert!(matches!(c.snapshot(id,&large_id),Err(e) if e.code=="unknown_snapshot"));
        let s = c.state.lock().unwrap();
        assert!(s.analyses[id].bytes < 4096);
        assert!(
            s.analyses
                .values()
                .map(|a| a.bytes + a.reserved)
                .sum::<usize>()
                <= s.global_bytes
        );
    }
}
