use crate::{
    ImportError, ImportLimits, Trace, error,
    import::{Source, discover_with_entries, open_regular, snapshot, unchanged},
    records::{RecordBudget, RecordDecoder},
    reducer::{Builder, Occurrence},
    views::SourceRef,
};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, Metadata},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

const BATCH_BYTES: usize = 1024 * 1024;
const BATCH_RECORDS: usize = 2000;
const BOUNDARY_BYTES: usize = 64;

#[derive(Clone)]
struct SourceCursor {
    path: PathBuf,
    metadata: Metadata,
    complete_offset: u64,
    high_water: u64,
    decoder: RecordDecoder,
    occurrence: Occurrence,
    boundary: Vec<u8>,
}

#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FollowProgress {
    pub source_count: usize,
    pub record_count: usize,
    pub read_bytes: usize,
    pub pending_bytes: usize,
}

pub struct FollowReader {
    configured_root: PathBuf,
    root: PathBuf,
    root_identity: Metadata,
    limits: ImportLimits,
    cursors: HashMap<String, SourceCursor>,
    entries: HashSet<PathBuf>,
    evidence: Builder,
    budget: RecordBudget,
    published: bool,
    stopped: Option<ImportError>,
}
pub struct FollowUpdate {
    pub snapshot: Option<Trace>,
    pub catching_up: bool,
    pub progress: FollowProgress,
}
fn location(file: &str, line: usize) -> SourceRef {
    SourceRef {
        file: file.to_owned(),
        line,
    }
}
fn unreadable(file: &str, line: usize) -> ImportError {
    error(
        "source_unreadable",
        "A recording source could not be read.",
        Some(location(file, line)),
    )
}
fn changed(file: &str, line: usize) -> ImportError {
    error(
        "source_changed",
        "An append-only source changed discontinuously.",
        Some(location(file, line)),
    )
}
fn same_identity(a: &Metadata, b: &Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        a.dev() == b.dev() && a.ino() == b.ino()
    }
    #[cfg(not(unix))]
    {
        let _ = (a, b);
        false
    }
}
fn continuous(before: &Metadata, after: &Metadata) -> bool {
    same_identity(before, after)
        && after.len() >= before.len()
        && (after.len() > before.len() || unchanged(before, after))
}
fn sample(
    file: &mut File,
    high_water: u64,
    relative: &str,
    line: usize,
) -> Result<Vec<u8>, ImportError> {
    let length = high_water.min(BOUNDARY_BYTES as u64) as usize;
    let mut boundary = vec![0; length];
    file.seek(SeekFrom::Start(high_water - length as u64))
        .map_err(|_| unreadable(relative, line))?;
    file.read_exact(&mut boundary)
        .map_err(|_| changed(relative, line))?;
    Ok(boundary)
}
impl FollowReader {
    pub fn open(root: &Path, limits: ImportLimits) -> Result<Self, ImportError> {
        Self::open_in_run(root, limits, crate::RunId::new())
    }
    pub fn open_in_run(
        root: &Path,
        limits: ImportLimits,
        run_id: crate::RunId,
    ) -> Result<Self, ImportError> {
        if !cfg!(unix) {
            return Err(error(
                "source_identity_unsupported",
                "This platform has no supported append-only source identity check.",
                None,
            ));
        }
        let configured_root = std::env::current_dir()
            .map_err(|_| unreadable("events.jsonl", 1))?
            .join(root);
        let root = fs::canonicalize(&configured_root).map_err(|_| unreadable("events.jsonl", 1))?;
        let root_identity = fs::metadata(&root).map_err(|_| unreadable("events.jsonl", 1))?;
        if !root_identity.is_dir() {
            return Err(unreadable("events.jsonl", 1));
        }
        Ok(Self {
            configured_root,
            root,
            root_identity,
            limits,
            cursors: HashMap::new(),
            entries: HashSet::new(),
            evidence: Builder::with_run(run_id),
            budget: RecordBudget {
                bytes: 0,
                records: 0,
                limits,
            },
            published: false,
            stopped: None,
        })
    }
    pub fn run_id(&self) -> &str {
        self.evidence.run_id()
    }
    pub fn progress(&self) -> FollowProgress {
        FollowProgress {
            source_count: self.cursors.len(),
            record_count: self.budget.records,
            read_bytes: self.budget.bytes,
            pending_bytes: self
                .cursors
                .values()
                .map(|cursor| cursor.decoder.pending.len())
                .sum(),
        }
    }
    fn validate(&self, source: &Source) -> Result<(), ImportError> {
        if let Some(cursor) = self.cursors.get(&source.relative) {
            let line = cursor.decoder.line + 1;
            if cursor.path != source.path || !continuous(&cursor.metadata, &source.before) {
                return Err(changed(&source.relative, line));
            }
        }
        Ok(())
    }
    pub fn update(&mut self) -> Result<FollowUpdate, ImportError> {
        self.update_with_hook(|| {})
    }
    fn update_with_hook(
        &mut self,
        before_commit: impl FnOnce(),
    ) -> Result<FollowUpdate, ImportError> {
        if let Some(error) = &self.stopped {
            return Err(error.clone());
        }
        let result = self.attempt_update(before_commit);
        if let Err(error) = &result {
            self.stopped = Some(error.clone());
        }
        result
    }
    fn attempt_update(
        &mut self,
        before_commit: impl FnOnce(),
    ) -> Result<FollowUpdate, ImportError> {
        let resolved =
            fs::canonicalize(&self.configured_root).map_err(|_| changed("events.jsonl", 1))?;
        let root_now = fs::metadata(&resolved).map_err(|_| changed("events.jsonl", 1))?;
        if resolved != self.root || !same_identity(&root_now, &self.root_identity) {
            return Err(changed("events.jsonl", 1));
        }
        let mut entries = self.entries.clone();
        let sources = discover_with_entries(&self.root, self.limits.sources, &mut entries)?;
        for source in &sources {
            self.validate(source)?;
        }
        for (relative, cursor) in &self.cursors {
            if !sources.iter().any(|source| source.relative == *relative) {
                return Err(changed(relative, cursor.decoder.line + 1));
            }
        }
        let has_updates = sources.iter().any(|source| {
            self.cursors
                .get(&source.relative)
                .is_none_or(|cursor| source.before.len() > cursor.high_water)
        });
        if !has_updates && self.published {
            self.entries = entries;
            return Ok(FollowUpdate {
                snapshot: None,
                catching_up: false,
                progress: self.progress(),
            });
        }
        let mut cursors = self.cursors.clone();
        let mut evidence = self.evidence.clone();
        let mut budget = self.budget.clone();
        let old_records = budget.records;
        let old_sources = cursors.len();
        let mut batch_bytes = 0;
        let mut catching_up = false;
        for source in &sources {
            let cursor = cursors
                .entry(source.relative.clone())
                .or_insert_with(|| SourceCursor {
                    path: source.path.clone(),
                    metadata: source.before.clone(),
                    complete_offset: 0,
                    high_water: 0,
                    decoder: RecordDecoder {
                        file: source.relative.clone(),
                        line: 0,
                        pending: Vec::new(),
                    },
                    occurrence: Occurrence::default(),
                    boundary: Vec::new(),
                });
            if unchanged(&cursor.metadata, &source.before)
                && cursor.high_water == source.before.len()
            {
                continue;
            }
            let line = cursor.decoder.line + 1;
            let mut file = open_regular(&source.path, &source.relative, line)?;
            let opened = file
                .metadata()
                .map_err(|_| unreadable(&source.relative, line))?;
            if !continuous(&source.before, &opened) {
                return Err(changed(&source.relative, line));
            }
            #[cfg(target_os = "linux")]
            {
                use std::os::fd::AsRawFd;
                let actual = fs::canonicalize(format!("/proc/self/fd/{}", file.as_raw_fd()))
                    .map_err(|_| changed(&source.relative, line))?;
                if !actual.starts_with(&self.root) {
                    return Err(error(
                        "source_escape",
                        "An opened recording source escapes its selected root.",
                        Some(location(&source.relative, line)),
                    ));
                }
            }
            if sample(&mut file, cursor.high_water, &source.relative, line)? != cursor.boundary {
                return Err(changed(&source.relative, line));
            }
            while cursor.high_water < source.before.len()
                && batch_bytes < BATCH_BYTES
                && budget.records - old_records < BATCH_RECORDS
            {
                let length = (source.before.len() - cursor.high_water)
                    .min(8192)
                    .min((BATCH_BYTES - batch_bytes) as u64) as usize;
                let mut bytes = vec![0; length];
                file.seek(SeekFrom::Start(cursor.high_water))
                    .map_err(|_| unreadable(&source.relative, cursor.decoder.line + 1))?;
                file.read_exact(&mut bytes)
                    .map_err(|_| changed(&source.relative, cursor.decoder.line + 1))?;
                let remaining_records = BATCH_RECORDS - (budget.records - old_records);
                if let Some((index, _)) = bytes
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| **b == b'\n')
                    .nth(remaining_records - 1)
                {
                    bytes.truncate(index + 1);
                }
                let records = cursor.decoder.push(&bytes, &mut budget)?;
                cursor.high_water += bytes.len() as u64;
                cursor.complete_offset = cursor.high_water - cursor.decoder.pending.len() as u64;
                batch_bytes += bytes.len();
                for record in records {
                    evidence.record(
                        &mut cursor.occurrence,
                        &record.bytes,
                        true,
                        record.source,
                        source.child_id.as_deref(),
                    );
                }
            }
            cursor.boundary = sample(
                &mut file,
                cursor.high_water,
                &source.relative,
                cursor.decoder.line + 1,
            )?;
            let after = file
                .metadata()
                .map_err(|_| unreadable(&source.relative, cursor.decoder.line + 1))?;
            if !continuous(&opened, &after) {
                return Err(changed(&source.relative, cursor.decoder.line + 1));
            }
            cursor.metadata = after;
            catching_up |= cursor.high_water < cursor.metadata.len();
        }
        before_commit();
        let final_root =
            fs::canonicalize(&self.configured_root).map_err(|_| changed("events.jsonl", 1))?;
        let final_root_metadata =
            fs::metadata(&final_root).map_err(|_| changed("events.jsonl", 1))?;
        if final_root != self.root || !same_identity(&final_root_metadata, &self.root_identity) {
            return Err(changed("events.jsonl", 1));
        }
        let final_sources = discover_with_entries(&self.root, self.limits.sources, &mut entries)?;
        for (relative, cursor) in &mut cursors {
            let source = final_sources
                .iter()
                .find(|source| source.relative == *relative)
                .ok_or_else(|| changed(relative, cursor.decoder.line + 1))?;
            if source.path != cursor.path || !continuous(&cursor.metadata, &source.before) {
                return Err(changed(relative, cursor.decoder.line + 1));
            }
            if !unchanged(&source.before, &cursor.metadata) {
                let mut file = open_regular(&source.path, relative, cursor.decoder.line + 1)?;
                let metadata = file
                    .metadata()
                    .map_err(|_| unreadable(relative, cursor.decoder.line + 1))?;
                if !continuous(&source.before, &metadata)
                    || sample(
                        &mut file,
                        cursor.high_water,
                        relative,
                        cursor.decoder.line + 1,
                    )? != cursor.boundary
                {
                    return Err(changed(relative, cursor.decoder.line + 1));
                }
                let after = file
                    .metadata()
                    .map_err(|_| unreadable(relative, cursor.decoder.line + 1))?;
                if !continuous(&metadata, &after) {
                    return Err(changed(relative, cursor.decoder.line + 1));
                }
                cursor.metadata = after;
            }
            catching_up |= cursor.high_water < cursor.metadata.len();
        }
        catching_up |= final_sources.len() > cursors.len();
        let publish =
            !self.published || old_sources != cursors.len() || old_records != budget.records;
        let snapshot =
            publish.then(|| snapshot(&evidence, false, cursors.len(), budget.records, self.limits));
        self.cursors = cursors;
        self.evidence = evidence;
        self.budget = budget;
        self.entries = entries;
        self.published |= publish;
        Ok(FollowUpdate {
            snapshot,
            catching_up,
            progress: self.progress(),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs::{self, OpenOptions},
        io::Write,
    };
    fn append(path: &Path, bytes: &[u8]) {
        OpenOptions::new()
            .append(true)
            .open(path)
            .unwrap()
            .write_all(bytes)
            .unwrap();
    }
    #[test]
    fn replacement_during_a_staged_batch_preserves_previous_publication_and_offsets() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("events.jsonl");
        fs::write(&path, b"").unwrap();
        let mut reader = FollowReader::open(directory.path(), Default::default()).unwrap();
        let original = reader.update().unwrap().snapshot.unwrap();
        append(&path, b"{\"type\":\"session_start\"}\n");
        let result = reader.update_with_hook(|| {
            let replacement = directory.path().join("replacement");
            fs::write(&replacement, b"{\"type\":\"session_start\"}\n").unwrap();
            fs::rename(replacement, &path).unwrap();
        });
        assert_eq!(result.err().unwrap().code, "source_changed");
        assert_eq!(reader.progress().read_bytes, 0);
        assert_eq!(reader.progress().record_count, 0);
        assert_eq!(reader.cursors["events.jsonl"].complete_offset, 0);
        assert_eq!(original.overview().unwrap().record_count, 0);
    }
    #[test]
    fn inspection_retention_budget_remains_cumulative_across_live_batches() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("events.jsonl");
        let tools: Vec<_> = (0..16).map(|i| serde_json::json!({"name":format!("{i:02}{}", "x".repeat(510)), "parameters":{}})).collect();
        let request = serde_json::json!({"type":"llm_request","input":{"input":[],"tools":tools}})
            .to_string();
        fs::write(
            &path,
            format!("{{\"type\":\"step_start\"}}\n{request}\n").repeat(2000),
        )
        .unwrap();
        let mut reader = FollowReader::open(directory.path(), Default::default()).unwrap();
        loop {
            let update = reader.update().unwrap();
            assert!(reader.evidence.inspection_bytes <= crate::inspection::TOTAL_BYTES);
            let actual: usize = reader
                .evidence
                .nodes
                .iter()
                .map(|node| node.context.retained_bytes() + node.tool_inspection.retained_bytes())
                .sum();
            assert_eq!(actual, reader.evidence.inspection_bytes);
            if !update.catching_up {
                break;
            }
        }
        assert_eq!(reader.progress().record_count, 4000);
        assert!(
            reader
                .evidence
                .nodes
                .iter()
                .any(|node| matches!(node.context, crate::model::ContextState::Limited { .. }))
        );
        let accepted_bytes = reader.evidence.inspection_bytes;
        for _ in 0..10 {
            assert!(reader.update().unwrap().snapshot.is_none());
            assert_eq!(reader.evidence.inspection_bytes, accepted_bytes);
        }
    }
    #[test]
    fn continuity_uses_latest_observed_length_even_above_the_captured_prefix() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("events.jsonl");
        fs::write(&path, vec![b'x'; 100]).unwrap();
        let captured = fs::metadata(&path).unwrap();
        append(&path, &[b'x'; 100]);
        let opened = fs::metadata(&path).unwrap();
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(150)
            .unwrap();
        let after = fs::metadata(&path).unwrap();
        assert!(continuous(&captured, &after));
        assert!(!continuous(&opened, &after));
    }
    #[test]
    fn empty_prefix_and_split_records_publish_once_without_redecoding() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("events.jsonl");
        fs::write(&path, b"").unwrap();
        let mut reader = FollowReader::open(directory.path(), Default::default()).unwrap();
        let empty = reader.update().unwrap().snapshot.unwrap();
        assert_eq!(empty.overview().unwrap().record_count, 0);
        assert!(reader.update().unwrap().snapshot.is_none());
        append(&path, b"{\"type\":\"step_start\",\"text\":\"\xd0");
        assert!(reader.update().unwrap().snapshot.is_none());
        append(&path, b"\xb0\"}");
        assert!(reader.update().unwrap().snapshot.is_none());
        append(&path, b"\n");
        let started = reader.update().unwrap().snapshot.unwrap();
        assert_eq!(started.overview().unwrap().record_count, 1);
        assert_eq!(started.node("n4").unwrap().observation.source.line, 1);
        assert!(
            !started
                .diagnostics(&Default::default())
                .unwrap()
                .items
                .iter()
                .any(|d| d.code == "missing_end" || d.code == "malformed_record")
        );
        let span = started.node("n4").unwrap().observation.span_id;
        assert!(reader.update().unwrap().snapshot.is_none());
        append(
            &path,
            b"{\"type\":\"step_end\",\"usage\":{\"totalTokens\":150}}\n",
        );
        let complete = reader.update().unwrap().snapshot.unwrap();
        assert_eq!(complete.node("n4").unwrap().observation.span_id, span);
        assert_eq!(
            complete
                .overview()
                .unwrap()
                .token_accounting
                .total_tokens
                .value
                .as_deref(),
            Some("150")
        );
    }
}
