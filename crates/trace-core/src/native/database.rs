use super::{session::SessionState, types::*};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
pub(crate) type SharedDatabase = Arc<Mutex<Database>>;
pub(crate) struct Database {
    connection: Connection,
    path: PathBuf,
    store: StoreId,
    stamp: String,
    version: i64,
    generation: u64,
    limits: NativeLimits,
    lease: Option<(String, Instant)>,
}
fn sql_error(_: rusqlite::Error) -> NativeError {
    failure(
        "database_unavailable",
        "The read-only database query failed or exceeded its execution budget.",
        None,
    )
}
fn uri(path: &Path) -> String {
    let bytes = path.as_os_str().as_encoded_bytes();
    let mut out = "file:".to_string();
    for b in bytes {
        if b.is_ascii_alphanumeric() || b"/-._~".contains(b) {
            out.push(*b as char)
        } else {
            out.push_str(&format!("%{b:02X}"))
        }
    }
    out.push_str("?mode=ro&readonly_shm=1");
    out
}
fn stamp(path: &Path) -> Result<String, NativeError> {
    let m = fs::metadata(path).map_err(|_| {
        failure(
            "source_unavailable",
            "The configured database is unavailable.",
            None,
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(format!("{}:{}", m.dev(), m.ino()))
    }
    #[cfg(not(unix))]
    {
        Ok(format!("{:?}", m.created().ok()))
    }
}
impl Database {
    pub fn open(path: &Path, store: StoreId, limits: NativeLimits) -> Result<Self, NativeError> {
        let wal = PathBuf::from(format!("{}-wal", path.display()));
        let shm = PathBuf::from(format!("{}-shm", path.display()));
        if wal.metadata().is_ok_and(|m| m.len() > 0) && fs::File::open(&shm).is_err() {
            return Err(failure(
                "wal_sidecar_unavailable",
                "A changing WAL database requires a readable existing shared-memory sidecar.",
                None,
            ));
        }
        let connection = Connection::open_with_flags(
            uri(path),
            OpenFlags::SQLITE_OPEN_READ_ONLY
                | OpenFlags::SQLITE_OPEN_URI
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .map_err(sql_error)?;
        connection
            .busy_timeout(Duration::from_millis(25))
            .map_err(sql_error)?;
        let mut db = Self {
            connection,
            path: path.into(),
            store,
            stamp: stamp(path)?,
            version: 0,
            generation: 1,
            limits,
            lease: None,
        };
        db.schema()?;
        db.version = db.data_version()?;
        Ok(db)
    }
    fn deadline(&self) {
        let deadline =
            (Instant::now() + Duration::from_millis(self.limits.statement_millis.max(1))).min(
                self.lease
                    .as_ref()
                    .map_or(Instant::now() + Duration::from_secs(1), |(_, d)| *d),
            );
        self.connection
            .progress_handler(1000, Some(move || Instant::now() >= deadline));
    }
    fn data_version(&self) -> Result<i64, NativeError> {
        self.connection
            .query_row("PRAGMA data_version", [], |r| r.get(0))
            .map_err(sql_error)
    }
    fn schema(&self) -> Result<(), NativeError> {
        self.deadline();
        for (table, required) in [
            (
                "session_v2",
                &[
                    "id",
                    "directory",
                    "time_updated",
                    "fork_session_id",
                    "fork_boundary",
                ][..],
            ),
            (
                "session_message",
                &["id", "session_id", "type", "seq", "data", "time_updated"][..],
            ),
            ("event", &["id", "aggregate_id", "seq", "type", "data"][..]),
        ] {
            let mut st = self
                .connection
                .prepare(&format!("PRAGMA table_info({table})"))
                .map_err(sql_error)?;
            let columns = st
                .query_map([], |r| r.get::<_, String>(1))
                .map_err(sql_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_error)?;
            if !required.iter().all(|c| columns.iter().any(|s| s == c)) {
                return Err(failure(
                    "unsupported_schema",
                    "The database does not have the supported OpenCode version-2 schema signature.",
                    None,
                ));
            }
        }
        Ok(())
    }
    pub fn identity(&self) -> &str {
        &self.stamp
    }
    pub fn revision(&mut self) -> Result<SourceRevision, NativeError> {
        if self.lease.is_some() {
            return Err(failure(
                "database_busy",
                "A bounded selected-session read transaction is active.",
                None,
            ));
        }
        let now = stamp(&self.path)?;
        if now != self.stamp {
            *self = Self::open(&self.path, self.store.clone(), self.limits)?;
            self.generation += 1;
        }
        let version = self.data_version()?;
        if version != self.version {
            self.schema()?;
            self.generation += 1;
            self.version = version;
        }
        Ok(SourceRevision(identity(&[
            &self.stamp,
            &self.generation.to_string(),
            &version.to_string(),
        ])))
    }
    fn source(&self, table: &str, id: &str, revision: &SourceRevision) -> NativeSource {
        NativeSource::Sqlite {
            provider: Provider::Opencode,
            store_id: self.store.clone(),
            database: self
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into(),
            schema_profile: "opencode-v2-2.1.4".into(),
            table: table.into(),
            primary_key: vec![KeyColumn {
                column: "id".into(),
                value: RowKey::Text { value: id.into() },
            }],
            field: None,
            observed_revision: revision.clone(),
        }
    }
    pub fn discover(
        &mut self,
        after: &str,
        limit: usize,
    ) -> Result<Vec<SessionDescriptor>, NativeError> {
        let revision = self.revision()?;
        self.deadline();
        let tx = self.connection.unchecked_transaction().map_err(sql_error)?;
        let mut st=tx.prepare("SELECT id,directory,time_updated,fork_session_id FROM session_v2 WHERE id > ?1 ORDER BY id LIMIT ?2").map_err(sql_error)?;
        let rows = st
            .query_map(rusqlite::params![after, limit as i64], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<i64>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                ))
            })
            .map_err(sql_error)?;
        let mut sessions = Vec::new();
        for row in rows {
            let (id, project, activity, parent) = row.map_err(sql_error)?;
            let source = self.source("session_v2", &id, &revision);
            let relationships = parent
                .map(|target| {
                    vec![NativeRelationship {
                        kind: "fork".into(),
                        target,
                        source: source.clone(),
                        execution: false,
                    }]
                })
                .unwrap_or_default();
            sessions.push(SessionDescriptor {
                id: CatalogSessionId(identity(&[
                    "opencode",
                    self.store.as_str(),
                    &self.path.to_string_lossy(),
                    &id,
                ])),
                provider: Provider::Opencode,
                store_id: self.store.clone(),
                native_id: id.clone(),
                recording_id: id,
                project: Some(crate::bounded(&project, 512).0),
                source_revision: revision.clone(),
                capabilities: Capabilities::profile("opencode-v2-2.1.4"),
                source,
                relationships,
                activity_ms: activity.and_then(|v| u64::try_from(v).ok()),
                path: self.path.clone(),
            });
        }
        drop(st);
        tx.commit().map_err(sql_error)?;
        Ok(sessions)
    }
    pub fn begin(
        &mut self,
        descriptor: &SessionDescriptor,
        run: Option<&str>,
    ) -> Result<DatabaseStage, NativeError> {
        let revision = self.revision()?;
        self.deadline();
        self.connection
            .execute_batch("BEGIN DEFERRED")
            .map_err(sql_error)?;
        let lease_key = crate::RunId::new().as_str().to_owned();
        self.lease = Some((lease_key.clone(), Instant::now() + Duration::from_secs(1)));
        let exists_result = self
            .connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM session_v2 WHERE id=?1)",
                [&descriptor.native_id],
                |r| r.get(0),
            )
            .map_err(sql_error);
        let exists: bool = match exists_result {
            Ok(v) => v,
            Err(e) => {
                self.cancel(&lease_key);
                return Err(e);
            }
        };
        if !exists {
            self.cancel(&lease_key);
            return Err(failure(
                "source_unavailable",
                "The selected session has been removed.",
                Some(descriptor.source.clone()),
            ));
        }
        let mut current_descriptor = descriptor.clone();
        current_descriptor.source_revision = revision.clone();
        if let NativeSource::Sqlite {
            observed_revision, ..
        } = &mut current_descriptor.source
        {
            *observed_revision = revision.clone();
        }
        let mut state = SessionState::new(current_descriptor);
        if let Some(run) = run {
            state.run_id = run.into();
        }
        let segments = match self.segments(&descriptor.native_id) {
            Ok(v) => v,
            Err(e) if e.code == "fork_boundary_unavailable" => {
                state.attribution_partial = true;
                state.diagnostic("fork_boundary_unavailable", state.descriptor.source.clone());
                vec![DatabaseSegment {
                    session: descriptor.native_id.clone(),
                    max_seq: i64::MAX,
                    inherited: false,
                }]
            }
            Err(e) => {
                self.cancel(&lease_key);
                return Err(e);
            }
        };
        if let Err(e) = self.session_reconciliation(&mut state, &revision) {
            self.cancel(&lease_key);
            return Err(e);
        }
        Ok(DatabaseStage {
            state,
            revision,
            bytes: 0,
            table: 0,
            seq: i64::MIN,
            id: String::new(),
            read_records: 0,
            read_bytes: 0,
            lease_key,
            segments,
        })
    }
    pub fn batch(
        &mut self,
        stage: &mut DatabaseStage,
        additional: usize,
    ) -> Result<bool, NativeError> {
        if !self
            .lease
            .as_ref()
            .is_some_and(|(key, deadline)| key == &stage.lease_key && Instant::now() < *deadline)
        {
            self.cancel(&stage.lease_key);
            return Err(failure(
                "database_budget",
                "The bounded read transaction expired.",
                None,
            ));
        }
        self.deadline();
        let durable = stage.table == stage.segments.len();
        let (table, session_column, session, max_seq, inherited) = if durable {
            (
                "event",
                "aggregate_id",
                stage.state.descriptor.native_id.clone(),
                i64::MAX,
                false,
            )
        } else {
            let segment = &stage.segments[stage.table];
            (
                "session_message",
                "session_id",
                segment.session.clone(),
                segment.max_seq,
                segment.inherited,
            )
        };
        let sql = format!(
            "SELECT id,type,seq,length(CAST(data AS BLOB)),data FROM {table} WHERE {session_column}=?1 AND (seq>?2 OR (seq=?2 AND id>?3)) AND seq<=?5 {} ORDER BY seq,id LIMIT ?4",
            if durable {
                "AND type='session.usage.recorded'"
            } else {
                ""
            }
        );
        let mut st = self.connection.prepare(&sql).map_err(sql_error)?;
        let mut rows = st
            .query(rusqlite::params![
                session,
                stage.seq,
                stage.id,
                self.limits.batch_records.max(1) as i64 + 1,
                max_seq
            ])
            .map_err(sql_error)?;
        let mut count = 0;
        let mut bytes = 0;
        let mut ended = true;
        while let Some(row) = rows.next().map_err(sql_error)? {
            let length: i64 = row.get(3).map_err(sql_error)?;
            let length = usize::try_from(length)
                .map_err(|_| failure("record_limit", "Invalid database row length.", None))?;
            if length > self.limits.import.record_bytes {
                return Err(failure(
                    "record_limit",
                    "A database row exceeds its decoded byte limit.",
                    None,
                ));
            }
            if count >= self.limits.batch_records.max(1)
                || bytes + length > self.limits.batch_bytes.max(self.limits.import.record_bytes)
            {
                ended = false;
                break;
            }
            if bytes + length > additional {
                return Err(failure(
                    "decoded_limit",
                    "The remaining decoded retention budget is exhausted.",
                    None,
                ));
            }
            if stage.bytes + length > self.limits.import.total_bytes
                || stage.state.records >= self.limits.import.records
            {
                return Err(failure(
                    "decoded_limit",
                    "The database session exceeds its decoded input limits.",
                    None,
                ));
            }
            let id: String = row.get(0).map_err(sql_error)?;
            let kind: String = row.get(1).map_err(sql_error)?;
            let seq: i64 = row.get(2).map_err(sql_error)?;
            let data: String = row.get(4).map_err(sql_error)?;
            let source = self.source(table, &id, &stage.revision);
            match serde_json::from_str::<Value>(&data) {
                Ok(mut v) if v.is_object() => {
                    v["id"] = Value::String(id.clone());
                    v["type"] = Value::String(kind);
                    stage.state.decode(&v, source);
                    if inherited {
                        let key = format!("message:{id}");
                        for claim in stage
                            .state
                            .ledger
                            .claims
                            .values_mut()
                            .filter(|c| c.owner == key)
                        {
                            claim.view.role = super::usage::UsageRole::Inherited {
                                origin: session.clone(),
                            };
                        }
                    }
                }
                _ => {
                    stage.state.records += 1;
                    stage.state.diagnostic("malformed_record", source)
                }
            }
            stage.seq = seq;
            stage.id = id;
            stage.bytes += length;
            stage.read_bytes += length;
            stage.read_records += 1;
            bytes += length;
            count += 1;
        }
        drop(rows);
        drop(st);
        if ended {
            stage.table += 1;
            stage.seq = i64::MIN;
            stage.id.clear();
        }
        if stage.table == stage.segments.len() + 1 {
            self.connection.execute_batch("COMMIT").map_err(sql_error)?;
            self.lease = None;
        }
        Ok(stage.table == stage.segments.len() + 1)
    }
    fn session_reconciliation(
        &self,
        state: &mut SessionState,
        revision: &SourceRevision,
    ) -> Result<(), NativeError> {
        let fields = [
            "tokens_input",
            "tokens_output",
            "tokens_reasoning",
            "tokens_cache_read",
            "tokens_cache_write",
        ];
        let mut info = self
            .connection
            .prepare("PRAGMA table_info(session_v2)")
            .map_err(sql_error)?;
        let columns = info
            .query_map([], |r| r.get::<_, String>(1))
            .map_err(sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_error)?;
        if !fields
            .iter()
            .all(|field| columns.iter().any(|c| c == field))
        {
            return Ok(());
        }
        let values=self.connection.query_row("SELECT tokens_input,tokens_output,tokens_reasoning,tokens_cache_read,tokens_cache_write FROM session_v2 WHERE id=?1",[&state.descriptor.native_id],|row|{let mut out=Vec::new();for i in 0..5{let value=match row.get_ref(i)?{rusqlite::types::ValueRef::Integer(n)=>Value::from(n),rusqlite::types::ValueRef::Null=>Value::Null,_=>Value::String("invalid counter type".into())};out.push(value);}Ok(out)}).map_err(sql_error)?;
        let value = serde_json::json!({"input":values[0],"output":values[1],"reasoning":values[2],"cache":{"read":values[3],"write":values[4]}});
        let source = self.source("session_v2", &state.descriptor.native_id, revision);
        let mut claim = super::usage::UsageClaim::parse(
            Provider::Opencode,
            "session",
            "session-aggregate",
            &value,
            source.clone(),
            super::usage::UsageRole::Reconciliation {
                scope: "sessionAggregate".into(),
            },
        );
        for counter in &mut claim.view.native_counters {
            let column = match counter.name.as_str() {
                "input" => Some("tokens_input"),
                "output" => Some("tokens_output"),
                "reasoning" => Some("tokens_reasoning"),
                "cacheRead" => Some("tokens_cache_read"),
                "cacheWrite" => Some("tokens_cache_write"),
                _ => None,
            };
            counter.source = source.clone();
            if let (Some(column), NativeSource::Sqlite { field, .. }) =
                (column, &mut counter.source)
            {
                *field = Some(FieldPath::Column {
                    column: column.into(),
                });
            }
        }
        for normalization in &mut claim.view.normalization {
            for operand in &mut normalization.operands {
                if let Some(counter) = claim
                    .view
                    .native_counters
                    .iter()
                    .find(|c| c.name == operand.name)
                {
                    operand.source = counter.source.clone();
                }
            }
        }
        state.ledger.put("session-aggregate".into(), claim);
        Ok(())
    }
    fn segments(&self, selected: &str) -> Result<Vec<DatabaseSegment>, NativeError> {
        let mut chain = Vec::new();
        let mut next = Some(selected.to_owned());
        let mut seen = std::collections::HashSet::new();
        while let Some(id) = next {
            if chain.len() >= self.limits.import.sources || !seen.insert(id.clone()) {
                return Err(failure(
                    "fork_cycle",
                    "The database fork ancestry is cyclic or exceeds its source limit.",
                    None,
                ));
            }
            let (parent, boundary): (Option<String>, Option<String>) = self
                .connection
                .query_row(
                    "SELECT fork_session_id,fork_boundary FROM session_v2 WHERE id=?1",
                    [&id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(sql_error)?
                .ok_or_else(|| {
                    failure(
                        "fork_boundary_unavailable",
                        "The persisted fork ancestry is unavailable.",
                        None,
                    )
                })?;
            next = parent.clone();
            chain.push((id, parent, boundary));
        }
        let mut segments: Vec<DatabaseSegment> = Vec::new();
        for (id, parent, boundary) in chain.into_iter().rev() {
            if parent.is_some() {
                let boundary = boundary
                    .and_then(|s| serde_json::from_str::<Value>(&s).ok())
                    .ok_or_else(|| {
                        failure(
                            "fork_boundary_unavailable",
                            "The persisted fork has no supported inherited-history boundary.",
                            None,
                        )
                    })?;
                let message = boundary["messageID"].as_str().ok_or_else(|| {
                    failure(
                        "fork_boundary_unavailable",
                        "The persisted fork boundary has no message identity.",
                        None,
                    )
                })?;
                let (owner, seq): (String, i64) = self
                    .connection
                    .query_row(
                        "SELECT session_id,seq FROM session_message WHERE id=?1",
                        [message],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()
                    .map_err(sql_error)?
                    .ok_or_else(|| {
                        failure(
                            "fork_boundary_unavailable",
                            "The persisted fork boundary message is unavailable.",
                            None,
                        )
                    })?;
                let i = segments
                    .iter()
                    .position(|s| s.session == owner)
                    .ok_or_else(|| {
                        failure(
                            "fork_boundary_unavailable",
                            "The persisted fork boundary is outside recorded ancestry.",
                            None,
                        )
                    })?;
                let end = match boundary["type"].as_str() {
                    Some("through") => seq,
                    Some("before") => seq.checked_sub(1).ok_or_else(|| {
                        failure(
                            "fork_boundary_unavailable",
                            "The fork boundary is outside supported sequence bounds.",
                            None,
                        )
                    })?,
                    _ => {
                        return Err(failure(
                            "fork_boundary_unavailable",
                            "The fork boundary variant is unsupported.",
                            None,
                        ));
                    }
                };
                segments.truncate(i + 1);
                segments[i].max_seq = segments[i].max_seq.min(end);
            }
            segments.push(DatabaseSegment {
                inherited: id != selected,
                session: id,
                max_seq: i64::MAX,
            });
        }
        Ok(segments)
    }
    pub fn cancel(&mut self, key: &str) {
        if self.lease.as_ref().is_some_and(|(owner, _)| owner == key) {
            let _ = self.connection.execute_batch("ROLLBACK");
            self.lease = None;
        }
    }
}
pub(crate) struct DatabaseStage {
    pub state: SessionState,
    pub revision: SourceRevision,
    pub bytes: usize,
    table: usize,
    seq: i64,
    id: String,
    pub read_records: usize,
    pub read_bytes: usize,
    pub lease_key: String,
    segments: Vec<DatabaseSegment>,
}
struct DatabaseSegment {
    session: String,
    max_seq: i64,
    inherited: bool,
}
