use crate::{
    ImportError, ImportLimits, Trace, error,
    records::{RecordBudget, RecordDecoder},
    reducer::{Builder, Occurrence},
    views::*,
};
use std::{
    fs::{self, File, Metadata},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static SNAPSHOT_SEQUENCE: AtomicU64 = AtomicU64::new(1);
pub(crate) struct Source {
    pub(crate) path: PathBuf,
    pub(crate) relative: String,
    pub(crate) child_id: Option<String>,
    pub(crate) before: Metadata,
}
fn source_ref(file: &str, line: usize) -> SourceRef {
    SourceRef {
        file: file.into(),
        line,
    }
}
fn io_error(source: SourceRef) -> ImportError {
    error(
        "source_unreadable",
        "The recording source could not be read.",
        Some(source),
    )
}
fn checked_path(root: &Path, path: &Path, relative: &str) -> Result<PathBuf, ImportError> {
    let resolved = fs::canonicalize(path).map_err(|_| io_error(source_ref(relative, 1)))?;
    if !resolved.starts_with(root) {
        return Err(error(
            "source_escape",
            "A recording path escapes the selected directory.",
            Some(source_ref(relative, 1)),
        ));
    }
    Ok(resolved)
}
fn discover(root: &Path, limit: usize) -> Result<Vec<Source>, ImportError> {
    discover_with_entries(root, limit, &mut std::collections::HashSet::new())
}
pub(crate) fn discover_with_entries(
    root: &Path,
    limit: usize,
    entries: &mut std::collections::HashSet<PathBuf>,
) -> Result<Vec<Source>, ImportError> {
    let mut sources = Vec::new();
    let mut add =
        |path: PathBuf, relative: String, child_id: Option<String>| -> Result<(), ImportError> {
            if sources.len() >= limit {
                return Err(error(
                    "source_limit",
                    "The recording source count exceeds the import limit.",
                    Some(source_ref(&relative, 1)),
                ));
            }
            let path = checked_path(root, &path, &relative)?;
            let before = fs::metadata(&path).map_err(|_| io_error(source_ref(&relative, 1)))?;
            if !before.is_file() {
                return Err(io_error(source_ref(&relative, 1)));
            }
            sources.push(Source {
                path,
                relative,
                child_id,
                before,
            });
            Ok(())
        };
    add(root.join("events.jsonl"), "events.jsonl".into(), None)?;
    let mut pending = vec![(root.to_path_buf(), String::new())];
    let mut seen = std::collections::HashSet::new();
    seen.insert(root.to_path_buf());
    while let Some((parent, prefix)) = pending.pop() {
        let directory = parent.join("subagents");
        let directory_relative = format!("{prefix}subagents");
        match fs::symlink_metadata(&directory) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err(io_error(source_ref(&directory_relative, 1))),
            Ok(_) => {}
        }
        let directory = checked_path(root, &directory, &directory_relative)?;
        for entry in
            fs::read_dir(directory).map_err(|_| io_error(source_ref(&directory_relative, 1)))?
        {
            let entry = entry.map_err(|_| io_error(source_ref(&directory_relative, 1)))?;
            entries.insert(entry.path());
            if entries.len() >= limit {
                return Err(error(
                    "source_entry_limit",
                    "The source-directory entry traversal exceeds the import limit.",
                    Some(source_ref(&directory_relative, 1)),
                ));
            }
            let name = entry.file_name().into_string().map_err(|_| {
                error(
                    "source_name",
                    "A source path is not valid UTF-8.",
                    Some(source_ref(&directory_relative, 1)),
                )
            })?;
            let child_prefix = format!("{prefix}subagents/{name}/");
            let relative = format!("{child_prefix}events.jsonl");
            let child = checked_path(root, &entry.path(), &child_prefix)?;
            if !child.is_dir() {
                continue;
            }
            let path = child.join("events.jsonl");
            match fs::symlink_metadata(&path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(_) => return Err(io_error(source_ref(&relative, 1))),
                Ok(_) => {
                    if !seen.insert(child.clone()) {
                        return Err(error(
                            "source_cycle",
                            "A recording directory is referenced more than once.",
                            Some(source_ref(&relative, 1)),
                        ));
                    }
                    add(path, relative.clone(), Some(relative))?;
                    pending.push((child, child_prefix));
                }
            }
        }
    }
    sources[1..].sort_by(|a, b| a.relative.cmp(&b.relative));
    Ok(sources)
}
pub(crate) fn unchanged(a: &Metadata, b: &Metadata) -> bool {
    if a.len() != b.len() || a.modified().ok() != b.modified().ok() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if a.dev() != b.dev()
            || a.ino() != b.ino()
            || a.ctime() != b.ctime()
            || a.ctime_nsec() != b.ctime_nsec()
        {
            return false;
        }
    }
    true
}
fn changed(source: &Source) -> ImportError {
    error(
        "source_changed",
        "A recording source changed during import.",
        Some(source_ref(&source.relative, 1)),
    )
}

pub(crate) fn open_regular(path: &Path, relative: &str, line: usize) -> Result<File, ImportError> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
    }
    let file = options
        .open(path)
        .map_err(|_| io_error(source_ref(relative, line)))?;
    if !file
        .metadata()
        .map_err(|_| io_error(source_ref(relative, line)))?
        .is_file()
    {
        return Err(io_error(source_ref(relative, line)));
    }
    Ok(file)
}

pub(crate) fn open(source: &Path, limits: ImportLimits) -> Result<Trace, ImportError> {
    open_with_hook(source, limits, || {})
}
fn open_with_hook(
    source: &Path,
    limits: ImportLimits,
    before_final_check: impl FnOnce(),
) -> Result<Trace, ImportError> {
    let root = fs::canonicalize(source).map_err(|_| io_error(source_ref("events.jsonl", 1)))?;
    let sources = discover(&root, limits.sources)?;
    let mut builder = Builder::new();
    let mut budget = RecordBudget {
        bytes: 0,
        records: 0,
        limits,
    };
    for source in &sources {
        let file = open_regular(&source.path, &source.relative, 1)?;
        if !unchanged(
            &source.before,
            &file.metadata().map_err(|_| changed(source))?,
        ) {
            return Err(changed(source));
        }
        #[cfg(target_os = "linux")]
        {
            use std::os::fd::AsRawFd;
            let actual = fs::canonicalize(format!("/proc/self/fd/{}", file.as_raw_fd()))
                .map_err(|_| changed(source))?;
            if !actual.starts_with(&root) {
                return Err(error(
                    "source_escape",
                    "An opened source escapes the selected directory.",
                    Some(source_ref(&source.relative, 1)),
                ));
            }
        }
        let mut reader = BufReader::with_capacity(8192, file);
        let mut occurrence = Occurrence::default();
        let mut decoder = RecordDecoder {
            file: source.relative.clone(),
            line: 0,
            pending: Vec::new(),
        };
        loop {
            let available = reader
                .fill_buf()
                .map_err(|_| io_error(source_ref(&source.relative, decoder.line + 1)))?;
            if available.is_empty() {
                break;
            }
            let count = available.len();
            let records = decoder.push(available, &mut budget)?;
            reader.consume(count);
            for record in records {
                builder.record(
                    &mut occurrence,
                    &record.bytes,
                    record.terminated,
                    record.source,
                    source.child_id.as_deref(),
                );
            }
        }
        if let Some(record) = decoder.finish_offline(&mut budget)? {
            builder.record(
                &mut occurrence,
                &record.bytes,
                record.terminated,
                record.source,
                source.child_id.as_deref(),
            );
        }
        if !unchanged(
            &source.before,
            &reader.get_ref().metadata().map_err(|_| changed(source))?,
        ) {
            return Err(changed(source));
        }
    }
    before_final_check();
    let final_sources = discover(&root, limits.sources).map_err(|_| {
        error(
            "source_changed",
            "The recording source set changed during import.",
            Some(source_ref("events.jsonl", 1)),
        )
    })?;
    if sources.len() != final_sources.len() {
        return Err(error(
            "source_changed",
            "The recording source set changed during import.",
            Some(source_ref("events.jsonl", 1)),
        ));
    }
    for (before, after) in sources.iter().zip(&final_sources) {
        if before.relative != after.relative
            || before.path != after.path
            || !unchanged(&before.before, &after.before)
        {
            return Err(changed(before));
        }
    }
    Ok(snapshot(
        &builder,
        true,
        sources.len(),
        budget.records,
        limits,
    ))
}

pub(crate) fn snapshot(
    evidence: &Builder,
    closed: bool,
    source_count: usize,
    record_count: usize,
    limits: ImportLimits,
) -> Trace {
    let mut builder = evidence.project(closed);
    let mut order = Vec::with_capacity(builder.nodes.len());
    let mut stack: Vec<_> = builder
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(i, n)| n.parent.is_none().then_some((i, 0)))
        .rev()
        .collect();
    while let Some((index, depth)) = stack.pop() {
        builder.nodes[index].depth = depth;
        order.push(index);
        for child in builder.nodes[index].children.iter().rev() {
            stack.push((*child, depth + 1));
        }
    }
    let mut start: Option<f64> = None;
    let mut end: Option<f64> = None;
    for node in &builder.nodes {
        if let (Some(s), Some(d)) = (node.start, node.duration.value_ms) {
            if node.end.is_some_and(|e| e < s) {
                continue;
            }
            if (s + d).is_finite() {
                start = Some(start.map_or(s, |n| n.min(s)));
                end = Some(end.map_or(s + d, |n| n.max(s + d)));
            }
        }
    }
    let timeline = start
        .zip(end)
        .filter(|(s, e)| (e - s).is_finite())
        .map(|(start_ms, end_ms)| Timeline {
            start_ms,
            end_ms,
            duration_ms: end_ms - start_ms,
        });
    let sequence = SNAPSHOT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    Trace {
        span_index: std::sync::OnceLock::new(),
        run_id: builder.run_id().to_owned(),
        snapshot_id: format!("s{stamp:x}-{:x}-{sequence:x}", std::process::id()),
        nodes: builder.nodes,
        order,
        diagnostics: builder.diagnostics,
        source_count,
        record_count,
        timeline,
        token_accounting: builder.token_accounting,
        limits,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn checked_source_open_refuses_non_regular_files_and_retargeted_symlinks() {
        use std::{
            ffi::CString,
            os::unix::{ffi::OsStrExt, fs::symlink},
        };
        let directory = tempfile::tempdir().unwrap();
        let fifo = directory.path().join("events.jsonl");
        let name = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        assert_eq!(
            open_regular(&fifo, "events.jsonl", 1).err().unwrap().code,
            "source_unreadable"
        );
        fs::remove_file(&fifo).unwrap();
        let target = directory.path().join("other");
        fs::write(&target, b"{}").unwrap();
        symlink(&target, &fifo).unwrap();
        assert_eq!(
            open_regular(&fifo, "events.jsonl", 1).err().unwrap().code,
            "source_unreadable"
        );
    }
    #[test]
    fn observable_source_change_fails_before_publication() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("events.jsonl");
        fs::write(&path, "").unwrap();
        let result = open_with_hook(directory.path(), ImportLimits::default(), || {
            fs::write(&path, "{}\n").unwrap()
        });
        assert_eq!(result.err().unwrap().code, "source_changed");
    }
}
