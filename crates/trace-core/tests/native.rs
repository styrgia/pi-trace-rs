use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    sync::Arc,
};
use trace_core::{DiagnosticQuery, ObservationQuery, native::*, views::*};
fn write(path: &Path, rows: &[Value]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let text = rows.iter().map(|v| format!("{v}\n")).collect::<String>();
    fs::write(path, text).unwrap();
}
fn append(path: &Path, value: &Value) {
    writeln!(
        OpenOptions::new().append(true).open(path).unwrap(),
        "{value}"
    )
    .unwrap();
}
fn discover(store: &mut NativeStore) -> Vec<SessionDescriptor> {
    let mut entries = Vec::new();
    for _ in 0..1000 {
        let b = store.discover(3).unwrap();
        assert!(b.visited <= 3);
        entries.extend(b.sessions);
        if b.sweep_complete {
            return entries;
        }
    }
    panic!("discovery failed to finish")
}
fn settled(reader: &mut SessionReader) -> Arc<SessionSnapshot> {
    for _ in 0..1000 {
        let u = reader.poll().unwrap();
        if u.progress != NativeProgress::CatchingUp {
            return reader.snapshot().unwrap();
        }
    }
    panic!("reader failed to settle")
}
fn total(s: &SessionSnapshot) -> Option<String> {
    s.overview().unwrap().token_accounting.total_tokens.value
}
fn pi_header() -> Value {
    json!({"type":"session","version":3,"id":"synthetic","cwd":"/fabricated/project"})
}
fn pi_message(id: &str, input: u64) -> Value {
    json!({"type":"message","id":id,"parentId":null,"timestamp":"2026-01-01T00:00:00Z","message":{"role":"assistant","model":"fixture-model","timestamp":1000,"durationMs":12,"content":[{"type":"text","text":"fabricated"}],"usage":{"input":input,"output":20,"cacheRead":0,"cacheWrite":0},"stopReason":"stop"}})
}
#[test]
fn pi_incremental_multiple_contributions_and_rewrite_run() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("project/session.jsonl");
    write(&path, &[pi_header(), pi_message("a", 100)]);
    let before = fs::read(&path).unwrap();
    let mut store = NativeStore::open(Provider::Pi, temp.path(), NativeLimits::default()).unwrap();
    let d = discover(&mut store).remove(0);
    assert_eq!(d.project.as_deref(), Some("/fabricated/project"));
    let mut reader = store.reader(&d).unwrap();
    let first = settled(&mut reader);
    assert_eq!(total(&first).as_deref(), Some("120"));
    assert_eq!(before, fs::read(&path).unwrap());
    let run = first.run_id().to_owned();
    assert!(reader.poll().unwrap().snapshot.is_none());
    append(
        &path,
        &json!({"type":"usage","id":"aux","usage":{"input":3,"output":7,"cacheRead":0,"cacheWrite":0}}),
    );
    let next = settled(&mut reader);
    assert_eq!(total(&next).as_deref(), Some("130"));
    assert_eq!(run, next.run_id());
    let reopened = settled(&mut store.reader(&d).unwrap());
    assert_eq!(total(&next), total(&reopened));
    write(&path, &[pi_header(), pi_message("b", 1)]);
    let reset = reader.poll().unwrap();
    assert!(reset.reset);
    let third = settled(&mut reader);
    assert_ne!(run, third.run_id());
    assert_eq!(total(&third).as_deref(), Some("21"));
}
#[test]
fn pending_utf8_and_malformed_middle_keep_original_lines() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("s.jsonl");
    write(&path, &[pi_header()]);
    let mut store = NativeStore::open(Provider::Pi, temp.path(), NativeLimits::default()).unwrap();
    let d = discover(&mut store).remove(0);
    let mut r = store.reader(&d).unwrap();
    settled(&mut r);
    let row = json!({"type":"message","id":"unicode","message":{"role":"user","content":"é"}})
        .to_string();
    let bytes = row.as_bytes();
    let split = bytes.iter().position(|b| *b == 0xc3).unwrap() + 1;
    OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(&bytes[..split])
        .unwrap();
    assert_eq!(r.poll().unwrap().progress, NativeProgress::Pending);
    assert_eq!(
        r.snapshot()
            .unwrap()
            .diagnostics(&DiagnosticQuery::default())
            .unwrap()
            .total,
        0
    );
    let mut f = OpenOptions::new().append(true).open(&path).unwrap();
    f.write_all(&bytes[split..]).unwrap();
    f.write_all(b"\n{broken}\n").unwrap();
    append(&path, &pi_message("good", 1));
    let s = settled(&mut r);
    assert_eq!(total(&s).as_deref(), Some("21"));
    let diag = s.diagnostics(&DiagnosticQuery::default()).unwrap();
    assert_eq!(diag.total, 1);
    let v = serde_json::to_value(&diag.items[0]).unwrap();
    assert_eq!(v["source"]["line"], "3");
    assert!(!v.to_string().contains("broken"));
}
#[test]
fn claude_revisions_parallel_tools_and_unknown_duration() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("lossy-project/a.jsonl");
    let msg = |output| json!({"type":"assistant","sessionId":"s","uuid":format!("revision-{output}"),"cwd":"/actual/project","message":{"id":"api-id","role":"assistant","content":[{"type":"tool_use","id":"t1","name":"tool-a","input":{}},{"type":"tool_use","id":"t2","name":"tool-b","input":{}}],"usage":{"input_tokens":100,"output_tokens":output,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}});
    write(
        &path,
        &[
            msg(10),
            msg(20),
            json!({"type":"user","sessionId":"s","uuid":"result","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"fabricated","is_error":false}]}}),
        ],
    );
    let mut store =
        NativeStore::open(Provider::Claude, temp.path(), NativeLimits::default()).unwrap();
    let d = discover(&mut store).remove(0);
    let s = settled(&mut store.reader(&d).unwrap());
    assert_eq!(total(&s).as_deref(), Some("120"));
    let rows = s.observations(&ObservationQuery::default()).unwrap();
    assert_eq!(
        rows.items
            .iter()
            .filter(|r| r.kind == NodeKind::Tool)
            .count(),
        2
    );
    assert!(
        rows.items
            .iter()
            .all(|r| r.duration.state == MeasurementState::Unknown)
    );
    let generation = rows
        .items
        .iter()
        .find(|r| r.kind == NodeKind::Generation)
        .unwrap();
    assert_eq!(
        s.span(&generation.span_id)
            .unwrap()
            .detail
            .span_usage
            .summary
            .own
            .total_tokens
            .value
            .as_deref(),
        Some("120")
    );
}
fn codex_header() -> Value {
    json!({"type":"session_meta","payload":{"id":"00000000-0000-0000-0000-000000000001","cwd":"/fabricated/codex"}})
}
const ROLLOUT: &str = "rollout-2026-01-01T00-00-00-00000000-0000-0000-0000-000000000001.jsonl";
#[test]
fn codex_archives_zstd_identity_exact_counters_and_bounds() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("sessions/2026/01/01").join(ROLLOUT);
    let usage = json!({"type":"event_msg","payload":{"type":"token_usage_record","response_id":"r","session_id":"ctx","usage":{"input_tokens":9007199254740993u64,"cached_input_tokens":3,"cache_write_input_tokens":0,"output_tokens":7,"reasoning_output_tokens":4},"thread_token_usage":{"total_tokens":99}}});
    write(&path, &[codex_header(), usage.clone(), usage]);
    let mut store =
        NativeStore::open(Provider::Codex, temp.path(), NativeLimits::default()).unwrap();
    let first = discover(&mut store).remove(0);
    let plain = settled(&mut store.reader(&first).unwrap());
    assert_eq!(total(&plain).as_deref(), Some("9007199254741000"));
    let archive = temp
        .path()
        .join("archived_sessions")
        .join(format!("{ROLLOUT}.zst"));
    fs::create_dir_all(archive.parent().unwrap()).unwrap();
    let data = fs::read(&path).unwrap();
    fs::write(
        &archive,
        zstd::stream::encode_all(data.as_slice(), 0).unwrap(),
    )
    .unwrap();
    fs::remove_file(&path).unwrap();
    let archived = discover(&mut store).remove(0);
    assert_eq!(first.id, archived.id);
    let compressed = settled(&mut store.reader(&archived).unwrap());
    assert_eq!(total(&plain), total(&compressed));
    let row = compressed
        .observations(&ObservationQuery::default())
        .unwrap()
        .items
        .remove(1);
    assert!(matches!(row.source, NativeSource::CompressedJsonl { .. }));
    let mut limits = NativeLimits::default();
    limits.import.total_bytes = 50;
    let mut bounded = NativeStore::open(Provider::Codex, temp.path(), limits).unwrap();
    let d = discover(&mut bounded).remove(0);
    let mut r = bounded.reader(&d).unwrap();
    let mut error = None;
    for _ in 0..10 {
        if let Err(e) = r.poll() {
            error = Some(e);
            break;
        }
    }
    assert_eq!(error.unwrap().code, "decoded_limit");
}
#[test]
fn older_pi_headers_and_provider_store_identity_are_read_only() {
    for version in [None, Some(1), Some(2), Some(3)] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("s.jsonl");
        let mut h = pi_header();
        if let Some(v) = version {
            h["version"] = json!(v)
        } else {
            h.as_object_mut().unwrap().remove("version");
        }
        write(&path, &[h, pi_message("a", 1)]);
        let bytes = fs::read(&path).unwrap();
        let mut store =
            NativeStore::open(Provider::Pi, temp.path(), NativeLimits::default()).unwrap();
        let d = discover(&mut store).remove(0);
        assert_eq!(
            total(&settled(&mut store.reader(&d).unwrap())).as_deref(),
            Some("21")
        );
        assert_eq!(bytes, fs::read(&path).unwrap());
    }
    let temp = tempfile::tempdir().unwrap();
    write(&temp.path().join("a.jsonl"), &[pi_header()]);
    write(&temp.path().join("b.jsonl"), &[pi_header()]);
    let mut s = NativeStore::open(Provider::Pi, temp.path(), NativeLimits::default()).unwrap();
    let d = discover(&mut s);
    assert_ne!(d[0].id, d[1].id);
    assert_ne!(
        StoreId::for_path(Provider::Pi, temp.path()),
        StoreId::for_path(Provider::Claude, temp.path())
    );
}
struct DatabaseWriter {
    child: std::process::Child,
    input: std::process::ChildStdin,
    output: std::io::BufReader<std::process::ChildStdout>,
}
impl DatabaseWriter {
    fn start(path: &Path) -> Self {
        use std::io::BufRead;
        use std::process::{Command, Stdio};
        let script = r#"
import sqlite3,sys,json
c=sqlite3.connect(sys.argv[1]);c.execute('PRAGMA journal_mode=WAL')
c.executescript('CREATE TABLE session_v2(id TEXT PRIMARY KEY,directory TEXT,time_updated INTEGER,fork_session_id TEXT,fork_boundary TEXT);CREATE TABLE session_message(id TEXT PRIMARY KEY,session_id TEXT,type TEXT,seq INTEGER,data TEXT,time_updated INTEGER);CREATE TABLE event(id TEXT PRIMARY KEY,aggregate_id TEXT,seq INTEGER,type TEXT,data TEXT);')
c.execute('INSERT INTO session_v2 VALUES(?,?,?,?,?)',('s','/fabricated/opencode',1,None,None))
def data(n): return json.dumps({'model':{'id':'fabricated'},'tokens':{'input':n,'output':15,'reasoning':5,'cache':{'read':0,'write':0}},'content':[{'type':'tool','id':'tool','name':'fabricated','state':{'status':'completed','input':{},'content':[{'type':'text','text':'fabricated'}]},'time':{'ran':100,'completed':110}}]})
c.execute('INSERT INTO session_message VALUES(?,?,?,?,?,?)',('m','s','assistant',1,data(100),1));c.commit();print('READY',flush=True)
for line in sys.stdin:
 command=line.strip()
 if command=='update':c.execute('UPDATE session_message SET data=? WHERE id=?',(data(110),'m'));c.commit()
 if command=='delete':c.execute('DELETE FROM session_message');c.commit()
 if command=='fork':
  c.execute('INSERT INTO session_v2 VALUES(?,?,?,?,?)',('child','/fabricated/child',1,'s',json.dumps({'type':'through','messageID':'m'})))
  c.execute('INSERT INTO session_message VALUES(?,?,?,?,?,?)',('child-m','child','assistant',1,json.dumps({'tokens':{'input':8,'output':1,'reasoning':1,'cache':{'read':0,'write':0}},'content':[]}),1))
  c.execute('INSERT INTO session_message VALUES(?,?,?,?,?,?)',('compact','child','compaction',2,json.dumps({'tokens':{'input':3,'output':1,'reasoning':1,'cache':{'read':0,'write':0}},'status':'completed'}),1))
  c.execute('INSERT INTO event VALUES(?,?,?,?,?)',('usage','child',1,'session.usage.recorded',json.dumps({'source':'compaction','tokens':{'input':3,'output':1,'reasoning':1,'cache':{'read':0,'write':0}}})));c.commit()

 if command=='aggregate':
  for name,value in [('input',100),('output',15),('reasoning',5),('cache_read',0),('cache_write',0)]:c.execute('ALTER TABLE session_v2 ADD COLUMN tokens_'+name+' INTEGER DEFAULT '+str(value))
  c.commit()
 if command=='missing-fork-boundary':c.execute('UPDATE session_v2 SET fork_boundary=NULL WHERE id=?',('child',));c.commit()
 if command=='schema':c.execute('ALTER TABLE event RENAME TO event_old');c.commit()
 if command=='busy':c.execute('BEGIN EXCLUSIVE')
 if command=='close':break
 print('READY',flush=True)
"#;
        let mut child = Command::new("python3")
            .args(["-u", "-c", script])
            .arg(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let mut output = std::io::BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        assert_eq!(line.trim(), "READY");
        Self {
            child,
            input,
            output,
        }
    }
    fn command(&mut self, s: &str) {
        use std::io::BufRead;
        writeln!(self.input, "{s}").unwrap();
        self.input.flush().unwrap();
        let mut line = String::new();
        self.output.read_line(&mut line).unwrap();
        assert_eq!(line.trim(), "READY");
    }
}
impl Drop for DatabaseWriter {
    fn drop(&mut self) {
        let _ = writeln!(self.input, "close");
        let _ = self.input.flush();
        let _ = self.child.wait();
    }
}
fn db_bytes(path: &Path) -> Vec<Vec<u8>> {
    ["", "-wal", "-shm"]
        .iter()
        .map(|suffix| fs::read(format!("{}{suffix}", path.display())).unwrap())
        .collect()
}
#[test]
fn opencode_readonly_wal_transactional_replacement_delete_and_schema() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("opencode.db");
    let mut writer = DatabaseWriter::start(&path);
    let initial = db_bytes(&path);
    let mut store =
        NativeStore::open(Provider::Opencode, temp.path(), NativeLimits::default()).unwrap();
    let descriptors = discover(&mut store);
    assert_eq!(descriptors.len(), 1);
    let d = &descriptors[0];
    let mut r = store.reader(d).unwrap();
    let first = settled(&mut r);
    assert_eq!(total(&first).as_deref(), Some("120"));
    assert_eq!(initial, db_bytes(&path));
    assert!(r.poll().unwrap().snapshot.is_none());
    let row = first
        .observations(&ObservationQuery::default())
        .unwrap()
        .items
        .into_iter()
        .find(|r| r.kind == NodeKind::Generation)
        .unwrap();
    let detail = first.span(&row.span_id).unwrap();
    let source = serde_json::to_value(detail.native_usage[0].source.clone()).unwrap();
    assert_eq!(source["kind"], "sqlite");
    assert_eq!(source["table"], "session_message");
    assert!(source.get("line").is_none());
    writer.command("update");
    let after_write = db_bytes(&path);
    let next = settled(&mut r);
    assert_eq!(total(&next).as_deref(), Some("130"));
    assert_eq!(first.run_id(), next.run_id());
    assert_eq!(after_write, db_bytes(&path));
    assert_eq!(total(&next), total(&settled(&mut store.reader(d).unwrap())));
    writer.command("delete");
    assert_eq!(total(&settled(&mut r)), None);
    writer.command("schema");
    assert_eq!(r.poll().err().unwrap().code, "unsupported_schema");
    assert!(r.snapshot().is_some());
}
#[test]
fn opencode_unsupported_schema_and_memory_store_are_structural() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("opencode.db");
    let c = rusqlite::Connection::open(&path).unwrap();
    c.execute_batch("CREATE TABLE session(id TEXT PRIMARY KEY)")
        .unwrap();
    drop(c);
    let before = fs::read(&path).unwrap();
    let mut s =
        NativeStore::open(Provider::Opencode, temp.path(), NativeLimits::default()).unwrap();
    let mut codes = Vec::new();
    for _ in 0..10 {
        let b = s.discover(10).unwrap();
        codes.extend(b.diagnostics.into_iter().map(|d| d.code));
        if b.sweep_complete {
            break;
        }
    }
    assert_eq!(codes, vec!["unsupported_schema"]);
    assert_eq!(before, fs::read(path).unwrap());
    assert_eq!(
        NativeStore::open(
            Provider::Opencode,
            Path::new(":memory:"),
            NativeLimits::default()
        )
        .err()
        .unwrap()
        .code,
        "unsupported_store"
    );
}

#[test]
fn codex_paginated_compressed_history_is_inherited_once() {
    let temp = tempfile::tempdir().unwrap();
    let parent = temp
        .path()
        .join("archived_sessions")
        .join(format!("{ROLLOUT}.zst"));
    fs::create_dir_all(parent.parent().unwrap()).unwrap();
    let response = |id: &str, ordinal, input| json!({"type":"token_usage_record","ordinal":ordinal,"payload":{"response_id":id,"session_id":"context","usage":{"input_tokens":input,"cached_input_tokens":0,"cache_write_input_tokens":0,"output_tokens":2}}});
    let bytes = format!(
        "{}\n{}\n",
        codex_header(),
        response("parent-response", 2, 100)
    )
    .into_bytes();
    fs::write(
        &parent,
        zstd::stream::encode_all(bytes.as_slice(), 0).unwrap(),
    )
    .unwrap();
    let child=temp.path().join("sessions/2026/01/01/rollout-2026-01-01T00-00-01-00000000-0000-0000-0000-000000000002.jsonl");
    write(
        &child,
        &[
            json!({"type":"session_meta","payload":{"id":"00000000-0000-0000-0000-000000000002","cwd":"/fixture","history_mode":"paginated","forked_from_id":"00000000-0000-0000-0000-000000000001","forked_from_ordinal_exclusive":3,"history_base":{"thread_id":"00000000-0000-0000-0000-000000000001","end_ordinal_exclusive":3,"end_byte_offset":bytes.len()}}}),
            json!({"type":"event_msg","payload":{"type":"item_completed","started_at_ms":100,"completed_at_ms":125,"item":{"type":"AgentMessage","id":"visible","content":[{"type":"text","text":"fabricated"}]}}}),
            response("child-response", 4, 8),
        ],
    );
    let mut store =
        NativeStore::open(Provider::Codex, temp.path(), NativeLimits::default()).unwrap();
    let d = discover(&mut store)
        .into_iter()
        .find(|d| d.native_id.ends_with('2'))
        .unwrap();
    let s = settled(&mut store.reader(&d).unwrap());
    assert_eq!(total(&s).as_deref(), Some("10"));
    let rows = s.observations(&ObservationQuery::default()).unwrap();
    let root = s.node(&rows.items[0].id).unwrap();
    assert!(
        root.native_usage
            .iter()
            .any(|e| matches!(e.role, UsageRole::Inherited { .. }))
    );
    assert!(rows.items.iter().any(|r| r.duration.value_ms == Some(25.0)));
    assert_eq!(root.detail.span_usage.contributor_count, 1);
}
#[test]
fn codex_repeated_cumulative_reports_keep_delta_and_diagnose_reset() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("sessions").join(ROLLOUT);
    let report = |n| json!({"type":"event_msg","payload":{"type":"token_count","context_id":"c","info":{"total_token_usage":{"input_tokens":n,"output_tokens":0,"cached_input_tokens":0,"cache_write_input_tokens":0,"total_tokens":n,"reasoning_output_tokens":0}}}});
    write(
        &path,
        &[
            codex_header(),
            report(100),
            report(120),
            report(120),
            report(90),
        ],
    );
    let mut store =
        NativeStore::open(Provider::Codex, temp.path(), NativeLimits::default()).unwrap();
    let d = discover(&mut store).remove(0);
    let s = settled(&mut store.reader(&d).unwrap());
    assert_eq!(total(&s).as_deref(), Some("20"));
    assert!(
        s.diagnostics(&DiagnosticQuery::default())
            .unwrap()
            .items
            .iter()
            .any(|d| d.code == "usage_discontinuity")
    );
}
#[test]
fn native_inclusive_evidence_count_and_timing_contract() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("s.jsonl");
    let mut assistant = pi_message("a", 100);
    assistant["message"]["content"] =
        json!([{"type":"toolCall","id":"t","name":"fabricated","arguments":{}}]);
    write(
        &path,
        &[
            pi_header(),
            assistant,
            json!({"type":"message","id":"result","timestamp":"2026-01-01T00:00:01Z","message":{"role":"toolResult","toolCallId":"t","timestamp":1010,"durationMs":5,"usage":{"input":3,"output":7,"cacheRead":0,"cacheWrite":0},"nestedCalls":{"complete":true,"calls":[{"id":"nested","name":"fabricated","status":"ok","durationMs":2}]},"content":"fabricated"}}),
        ],
    );
    let mut store = NativeStore::open(Provider::Pi, temp.path(), NativeLimits::default()).unwrap();
    let d = discover(&mut store).remove(0);
    let s = settled(&mut store.reader(&d).unwrap());
    for row in s.observations(&ObservationQuery::default()).unwrap().items {
        let d = s.node(&row.id).unwrap();
        let usage = d.detail.span_usage;
        assert_eq!(
            usage.evidence.len() + usage.omitted_count,
            usage.summary.inclusive.contributor_count
        );
        assert_eq!(
            usage.contributor_count,
            usage.summary.inclusive.contributor_count
        );
        if row.kind == NodeKind::Generation {
            assert_eq!(usage.summary.own.total_tokens.value.as_deref(), Some("120"));
            assert_eq!(
                usage.summary.inclusive.total_tokens.value.as_deref(),
                Some("130")
            );
            assert!(
                d.timing_evidence
                    .iter()
                    .any(|t| t.meaning == TimestampMeaning::RequestStart)
            );
        }
    }
}
#[test]
fn reader_metrics_are_actual_work_and_global_budget_is_pre_read() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("s.jsonl");
    write(&path, &[pi_header(), pi_message("a", 1)]);
    let mut store = NativeStore::open(Provider::Pi, temp.path(), NativeLimits::default()).unwrap();
    let d = discover(&mut store).remove(0);
    let mut reader = store.reader(&d).unwrap();
    let first = reader.poll_bounded(10).unwrap();
    assert_eq!(reader.read_bytes(), 10);
    assert_eq!(first.retained_decoded_bytes, 10);
    assert_eq!(reader.read_records(), 0);
    settled(&mut reader);
    let counters = (reader.read_bytes(), reader.read_records());
    reader.poll().unwrap();
    assert_eq!(counters, (reader.read_bytes(), reader.read_records()));
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    write(&path, &[pi_header(), pi_message("a", 2)]);
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(modified + std::time::Duration::from_secs(1))
        .unwrap();
    settled(&mut reader);
    assert!(reader.read_bytes() > counters.0);
    assert_eq!(reader.read_records(), counters.1 * 2);
}

#[test]
fn claude_pretty_sidecar_has_json_pointer_and_refreshes_without_transcript_read() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp
        .path()
        .join("project/parent/subagents/workflow/agent-child.jsonl");
    write(
        &path,
        &[
            json!({"type":"file-history-snapshot","snapshot":{}}),
            json!({"type":"assistant","sessionId":"parent","uuid":"u","cwd":"/fabricated","message":{"id":"m","usage":{"input_tokens":1,"output_tokens":2,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}),
        ],
    );
    let side = path.with_extension("meta.json");
    fs::write(
        &side,
        "{\n  \"toolUseId\": \"tool-a\",\n  \"parentAgentId\": \"parent-agent\"\n}\n",
    )
    .unwrap();
    let mut store =
        NativeStore::open(Provider::Claude, temp.path(), NativeLimits::default()).unwrap();
    let d = discover(&mut store).remove(0);
    let source = serde_json::to_value(&d.source).unwrap();
    assert_eq!(source["line"], "2");
    assert!(
        source["byteRange"]["start"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap()
            > 0
    );
    let relationship = d
        .relationships
        .iter()
        .find(|r| r.kind == "recordedInvokingTool")
        .unwrap();
    let locator = serde_json::to_value(&relationship.source).unwrap();
    assert_eq!(locator["kind"], "json");
    assert_eq!(locator["pointer"], "/toolUseId");
    assert!(locator.get("line").is_none());
    assert!(!relationship.execution);
    let mut reader = store.reader(&d).unwrap();
    let first = settled(&mut reader);
    let counters = (reader.read_bytes(), reader.read_records());
    fs::write(&side, "{\n  \"toolUseId\": \"different-tool-b\"\n}\n").unwrap();
    assert_ne!(d.source_revision, store.recheck(&d).unwrap());
    let second = settled(&mut reader);
    assert_eq!(first.run_id(), second.run_id());
    assert_ne!(first.snapshot_id(), second.snapshot_id());
    assert_eq!(counters, (reader.read_bytes(), reader.read_records()));
    let root = second
        .observations(&ObservationQuery::default())
        .unwrap()
        .items
        .remove(0);
    assert!(
        second
            .node(&root.id)
            .unwrap()
            .relationships
            .iter()
            .any(|r| r.target == "different-tool-b")
    );
}
#[test]
fn sqlite_batched_snapshot_is_one_transaction_and_drop_releases_lease() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("opencode.db");
    let mut writer = DatabaseWriter::start(&path);
    let limits = NativeLimits {
        batch_records: 1,
        ..Default::default()
    };
    let mut store = NativeStore::open(Provider::Opencode, temp.path(), limits).unwrap();
    let d = discover(&mut store).remove(0);
    let mut reader = store.reader(&d).unwrap();
    assert_eq!(reader.poll().unwrap().progress, NativeProgress::CatchingUp);
    assert_eq!(store.recheck(&d).err().unwrap().code, "database_busy");
    writer.command("update");
    let before = settled(&mut reader);
    assert_eq!(total(&before).as_deref(), Some("120"));
    let after = settled(&mut reader);
    assert_eq!(total(&after).as_deref(), Some("130"));
    let mut abandoned = store.reader(&d).unwrap();
    assert_eq!(
        abandoned.poll().unwrap().progress,
        NativeProgress::CatchingUp
    );
    drop(abandoned);
    assert!(store.recheck(&d).is_ok());
}

#[test]
fn archived_plain_and_compressed_final_records_and_corruption_are_bounded() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("archived_sessions").join(ROLLOUT);
    let usage = json!({"type":"token_usage_record","payload":{"response_id":"r","session_id":"ctx","usage":{"input_tokens":1,"cached_input_tokens":0,"cache_write_input_tokens":0,"output_tokens":2}}});
    write(&path, &[codex_header()]);
    OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(usage.to_string().as_bytes())
        .unwrap();
    let mut s = NativeStore::open(Provider::Codex, temp.path(), NativeLimits::default()).unwrap();
    let d = discover(&mut s).remove(0);
    assert_eq!(
        total(&settled(&mut s.reader(&d).unwrap())).as_deref(),
        Some("3")
    );
    let compressed = path.with_extension("jsonl.zst");
    fs::write(
        &compressed,
        zstd::stream::encode_all(fs::read(&path).unwrap().as_slice(), 0).unwrap(),
    )
    .unwrap();
    fs::remove_file(&path).unwrap();
    let d = discover(&mut s).remove(0);
    assert_eq!(
        total(&settled(&mut s.reader(&d).unwrap())).as_deref(),
        Some("3")
    );
    let mut bytes = fs::read(&compressed).unwrap();
    bytes.truncate(bytes.len() - 3);
    fs::write(&compressed, bytes).unwrap();
    let batch = s.discover(100).unwrap();
    assert!(
        batch
            .diagnostics
            .iter()
            .any(|d| d.code == "corrupt_compression")
            || !batch.sessions.is_empty()
    );
    if let Some(d) = batch.sessions.first() {
        let mut r = s.reader(d).unwrap();
        let mut failed = false;
        for _ in 0..10 {
            if r.poll().is_err() {
                failed = true;
                break;
            }
        }
        assert!(failed);
    }
}
#[test]
fn bounded_rotating_discovery_exact_boundary_and_source_escape() {
    let temp = tempfile::tempdir().unwrap();
    for i in 0..10 {
        write(
            &temp.path().join(format!("s{i}.jsonl")),
            &[json!({"type":"session","version":3,"id":format!("s{i}"),"cwd":"/synthetic"})],
        );
    }
    let mut store = NativeStore::open(Provider::Pi, temp.path(), NativeLimits::default()).unwrap();
    let first = store.discover(5).unwrap();
    assert!(!first.sweep_complete);
    let second = store.discover(6).unwrap();
    assert!(second.sweep_complete);
    assert_eq!(first.sessions.len() + second.sessions.len(), 10);
    #[cfg(unix)]
    {
        let outside = tempfile::tempdir().unwrap();
        write(&outside.path().join("escape.jsonl"), &[pi_header()]);
        std::os::unix::fs::symlink(outside.path(), temp.path().join("escape")).unwrap();
        let entries = discover(&mut store);
        assert_eq!(entries.len(), 10);
    }
}
fn assert_native_contract(name: &str, snapshot: &SessionSnapshot, value: Value) {
    let rows = snapshot
        .observations(&ObservationQuery {
            limit: 200,
            ..Default::default()
        })
        .unwrap();
    let mut replacements = std::collections::HashMap::new();
    for (i, r) in rows.items.iter().enumerate() {
        replacements.insert(r.id.clone(), format!("synthetic-node-{}", i + 1));
        replacements.insert(r.span_id.clone(), format!("synthetic-span-{}", i + 1));
    }
    fn normalize(v: &mut Value, replacements: &std::collections::HashMap<String, String>) {
        match v {
            Value::Object(fields) => {
                for (key, value) in fields {
                    if key == "storeId" {
                        *value = json!("synthetic-store");
                    } else if key == "snapshotId" {
                        *value = json!("synthetic-snapshot");
                    } else if key == "observedRevision" || key == "sourceRevision" {
                        *value = json!("synthetic-source-revision");
                    } else {
                        normalize(value, replacements);
                    }
                }
            }
            Value::Array(items) => {
                for item in items {
                    normalize(item, replacements);
                }
            }
            Value::String(s) => {
                if let Some(replacement) = replacements.get(s) {
                    *s = replacement.clone()
                }
            }
            _ => {}
        }
    }
    let mut value = value;
    normalize(&mut value, &replacements);
    let text = format!("{}\n", serde_json::to_string_pretty(&value).unwrap());
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/contracts")
        .join(format!("{name}.json"));
    if std::env::var_os("UPDATE_NATIVE_CONTRACTS").is_some() {
        fs::write(&path, &text).unwrap();
    }
    assert_eq!(fs::read_to_string(path).unwrap(), text);
}
#[test]
fn native_serialized_contracts_from_fabricated_stores() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("s.jsonl");
    write(&path, &[pi_header(), pi_message("a", 100)]);
    let mut store = NativeStore::open(Provider::Pi, temp.path(), NativeLimits::default()).unwrap();
    let d = discover(&mut store).remove(0);
    let snapshot = settled(&mut store.reader(&d).unwrap());
    assert_native_contract(
        "native-pi-overview",
        &snapshot,
        serde_json::to_value(snapshot.overview().unwrap()).unwrap(),
    );
    let row = snapshot
        .observations(&ObservationQuery::default())
        .unwrap()
        .items
        .remove(1);
    assert_native_contract(
        "native-pi-detail",
        &snapshot,
        serde_json::to_value(snapshot.node(&row.id).unwrap()).unwrap(),
    );
    let dbroot = tempfile::tempdir().unwrap();
    let path = dbroot.path().join("opencode.db");
    let _writer = DatabaseWriter::start(&path);
    let mut store =
        NativeStore::open(Provider::Opencode, dbroot.path(), NativeLimits::default()).unwrap();
    let d = discover(&mut store).remove(0);
    let snapshot = settled(&mut store.reader(&d).unwrap());
    let row = snapshot
        .observations(&ObservationQuery::default())
        .unwrap()
        .items
        .into_iter()
        .find(|r| r.kind == NodeKind::Generation)
        .unwrap();
    assert_native_contract(
        "native-sqlite-detail",
        &snapshot,
        serde_json::to_value(snapshot.node(&row.id).unwrap()).unwrap(),
    );

    let tool = snapshot
        .observations(&ObservationQuery::default())
        .unwrap()
        .items
        .into_iter()
        .find(|r| r.kind == NodeKind::Tool)
        .unwrap();
    assert_native_contract(
        "native-sqlite-tool-detail",
        &snapshot,
        serde_json::to_value(snapshot.node(&tool.id).unwrap()).unwrap(),
    );
    let store_id = StoreId::for_path(Provider::Pi, Path::new("/fabricated/store"));
    let revision: SourceRevision =
        serde_json::from_value(json!("synthetic-source-revision")).unwrap();
    let sources = vec![
        NativeSource::Jsonl {
            provider: Provider::Pi,
            store_id: store_id.clone(),
            source: "project/session.jsonl".into(),
            line: "2".into(),
            byte_range: Some(ByteRange {
                start: "88".into(),
                end: "310".into(),
            }),
        },
        NativeSource::Jsonl {
            provider: Provider::Claude,
            store_id: store_id.clone(),
            source: "project/agent-child.jsonl".into(),
            line: "1".into(),
            byte_range: None,
        },
        NativeSource::CompressedJsonl {
            provider: Provider::Codex,
            store_id: store_id.clone(),
            source: "archived_sessions/rollout.jsonl.zst".into(),
            encoding: "zstd".into(),
            decoded_line: "3".into(),
            decoded_byte_range: Some(ByteRange {
                start: "9007199254740993".into(),
                end: "9007199254740999".into(),
            }),
        },
        NativeSource::CompressedJsonl {
            provider: Provider::Codex,
            store_id: store_id.clone(),
            source: "archived_sessions/rollout.jsonl.zst".into(),
            encoding: "zstd".into(),
            decoded_line: "1".into(),
            decoded_byte_range: None,
        },
        NativeSource::Json {
            provider: Provider::Claude,
            store_id: store_id.clone(),
            source: "project/subagents/agent-child.meta.json".into(),
            pointer: Some("/toolUseId".into()),
        },
        NativeSource::Json {
            provider: Provider::Claude,
            store_id: store_id.clone(),
            source: "project/subagents/agent-child.meta.json".into(),
            pointer: None,
        },
        NativeSource::Sqlite {
            provider: Provider::Opencode,
            store_id: store_id.clone(),
            database: "opencode.db".into(),
            schema_profile: "opencode-v2-2.1.4".into(),
            table: "session_message".into(),
            primary_key: vec![KeyColumn {
                column: "id".into(),
                value: RowKey::Text { value: "m".into() },
            }],
            field: Some(FieldPath::Json {
                column: "data".into(),
                pointer: "/tokens/cache/read".into(),
            }),
            observed_revision: revision.clone(),
        },
        NativeSource::Sqlite {
            provider: Provider::Opencode,
            store_id: store_id.clone(),
            database: "opencode.db".into(),
            schema_profile: "typed-key-contract".into(),
            table: "fixture".into(),
            primary_key: vec![
                KeyColumn {
                    column: "ordinal".into(),
                    value: RowKey::Integer {
                        value: "9223372036854775807".into(),
                    },
                },
                KeyColumn {
                    column: "digest".into(),
                    value: RowKey::Blob { hex: "00ff".into() },
                },
            ],
            field: Some(FieldPath::Column {
                column: "count".into(),
            }),
            observed_revision: revision.clone(),
        },
        NativeSource::Sqlite {
            provider: Provider::Opencode,
            store_id,
            database: "opencode.db".into(),
            schema_profile: "opencode-v2-2.1.4".into(),
            table: "sqlite_schema".into(),
            primary_key: Vec::new(),
            field: None,
            observed_revision: revision,
        },
    ];
    assert_native_contract(
        "native-provenance-variants",
        &snapshot,
        serde_json::to_value(&sources).unwrap(),
    );
    let detail = snapshot.node(&row.id).unwrap();
    let template = detail.native_usage[0].clone();
    let mut usage = Vec::new();
    for (i, role) in [
        UsageRole::Contribution {
            category: "modelResponse".into(),
        },
        UsageRole::Contribution {
            category: "toolResult".into(),
        },
        UsageRole::Contribution {
            category: "compaction".into(),
        },
        UsageRole::Contribution {
            category: "branch_summary".into(),
        },
        UsageRole::Contribution {
            category: "cumulativeDelta".into(),
        },
        UsageRole::Reconciliation {
            scope: "thread_token_usage".into(),
        },
        UsageRole::Reconciliation {
            scope: "resultSummary".into(),
        },
        UsageRole::Inherited {
            origin: "parent-recording".into(),
        },
        UsageRole::Breakdown {
            included_in: "tool-result".into(),
        },
    ]
    .into_iter()
    .enumerate()
    {
        let mut entry = template.clone();
        entry.id = format!("synthetic-contribution-{i}");
        entry.role = role;
        entry.accepted = matches!(entry.role, UsageRole::Contribution { .. });
        usage.push(entry);
    }
    for (i, (state, value)) in [
        (TokenState::Exact, Some("0")),
        (TokenState::Exact, Some("9007199254740993")),
        (TokenState::Partial, Some("0")),
        (TokenState::Unknown, None),
        (TokenState::Invalid, None),
        (TokenState::NotApplicable, None),
    ]
    .into_iter()
    .enumerate()
    {
        let mut entry = template.clone();
        entry.id = format!("counter-state-{i}");
        let counter = TokenCounter {
            state,
            value: value.map(str::to_owned),
        };
        entry.accounting.input_tokens = counter.clone();
        entry.accounting.total_tokens = counter.clone();
        entry.native_counters = vec![NativeCounter {
            name: "nativeInput".into(),
            value: counter,
            source: sources[6].clone(),
        }];
        usage.push(entry);
    }
    for rule in [
        "subtractCachedInputSubset",
        "addDisjointReasoning",
        "differenceCumulativeSameContext",
        "reasoningIsSubsetOfOutput",
        "cacheSubdivisionIncludedInAggregate",
    ] {
        let mut entry = template.clone();
        entry.id = format!("normalization-{rule}");
        entry.normalization = vec![Normalization {
            component: "inputTokens".into(),
            rule: rule.into(),
            operands: template.native_counters.iter().take(2).cloned().collect(),
        }];
        usage.push(entry);
    }
    assert_native_contract(
        "native-usage-variants",
        &snapshot,
        serde_json::to_value(usage).unwrap(),
    );
}

#[test]
#[cfg(unix)]
fn unreadable_directory_is_explicit_partial_coverage_with_no_fabricated_locator() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let blocked = temp.path().join("blocked");
    write(&blocked.join("session.jsonl"), &[pi_header()]);
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o0)).unwrap();
    let mut store = NativeStore::open(Provider::Pi, temp.path(), NativeLimits::default()).unwrap();
    let batch = store.discover(100).unwrap();
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(batch.sweep_complete);
    assert!(batch.limited);
    assert!(batch.sessions.is_empty());
    assert_eq!(batch.diagnostics.len(), 1);
    let diagnostic = &batch.diagnostics[0];
    assert_eq!(diagnostic.code, "directory_unavailable");
    assert_eq!(diagnostic.provider, Provider::Pi);
    assert!(diagnostic.source.is_none());
    assert_eq!(diagnostic.store_id, store.store_id().clone());
    assert_eq!(discover(&mut store).len(), 1);
}

#[test]
fn opencode_fork_prefix_and_compaction_event_do_not_double_count() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("opencode.db");
    let mut writer = DatabaseWriter::start(&path);
    writer.command("fork");
    let mut store =
        NativeStore::open(Provider::Opencode, temp.path(), NativeLimits::default()).unwrap();
    let d = discover(&mut store)
        .into_iter()
        .find(|d| d.native_id == "child")
        .unwrap();
    let s = settled(&mut store.reader(&d).unwrap());
    assert_eq!(total(&s).as_deref(), Some("15"));
    let root = s
        .observations(&ObservationQuery::default())
        .unwrap()
        .items
        .remove(0);
    let detail = s.node(&root.id).unwrap();
    assert_eq!(detail.detail.span_usage.contributor_count, 2);
    assert!(
        detail
            .native_usage
            .iter()
            .any(|e| matches!(e.role, UsageRole::Inherited { .. }))
    );
    assert!(
        detail
            .native_usage
            .iter()
            .any(|e| matches!(e.role, UsageRole::Reconciliation { .. }))
    );
    assert!(
        detail
            .relationships
            .iter()
            .any(|r| r.kind == "fork" && !r.execution)
    );
}
#[test]
fn pi_auxiliary_kinds_and_cache_subsets_are_distinct_historical_work() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("s.jsonl");
    let usage = json!({"input":1,"output":2,"reasoning":1,"cacheRead":3,"cacheWrite":4,"cacheWrite1h":2,"totalTokens":10});
    write(
        &path,
        &[
            pi_header(),
            json!({"type":"usage","id":"u","parentId":null,"usage":usage}),
            json!({"type":"compaction","id":"c","parentId":"u","usage":usage}),
            json!({"type":"branch_summary","id":"b","parentId":"u","fromId":"c","usage":usage}),
        ],
    );
    let mut store = NativeStore::open(Provider::Pi, temp.path(), NativeLimits::default()).unwrap();
    let d = discover(&mut store).remove(0);
    let s = settled(&mut store.reader(&d).unwrap());
    assert_eq!(total(&s).as_deref(), Some("30"));
    let root = s
        .observations(&ObservationQuery::default())
        .unwrap()
        .items
        .remove(0);
    let detail = s.node(&root.id).unwrap();
    assert_eq!(detail.selected_context.as_deref(), Some("b"));
    assert_eq!(detail.native_usage.len(), 3);
    let accounting = &s.overview().unwrap().token_accounting;
    assert_eq!(accounting.contributor_count, 3);
    assert_eq!(accounting.observed_generations, 0);
    assert_eq!(accounting.total_generations, 0);
    assert_native_contract(
        "native-auxiliary-overview",
        &s,
        serde_json::to_value(s.overview().unwrap()).unwrap(),
    );
    assert_native_contract(
        "native-auxiliary-detail",
        &s,
        serde_json::to_value(&detail).unwrap(),
    );
    assert!(detail.native_usage.iter().all(|e| {
        e.normalization
            .iter()
            .any(|n| n.rule == "cacheSubdivisionIncludedInAggregate")
    }));
}
#[test]
fn claude_revised_call_keeps_late_result_and_ambiguous_owners_do_not_link() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("s.jsonl");
    let call = |id: &str| json!({"type":"assistant","sessionId":"s","uuid":id,"message":{"id":id,"content":[{"type":"tool_use","id":"t","name":"fixture","input":{}}]}});
    write(
        &path,
        &[
            call("a"),
            json!({"type":"user","sessionId":"s","uuid":"result","message":{"content":[{"type":"tool_result","tool_use_id":"t","content":"fabricated-result"}]}}),
            call("a"),
            call("other-owner"),
        ],
    );
    let mut store =
        NativeStore::open(Provider::Claude, temp.path(), NativeLimits::default()).unwrap();
    let d = discover(&mut store).remove(0);
    let s = settled(&mut store.reader(&d).unwrap());
    let tool = s
        .observations(&ObservationQuery::default())
        .unwrap()
        .items
        .into_iter()
        .find(|r| r.kind == NodeKind::Tool)
        .unwrap();
    assert_eq!(
        s.node(&tool.id).unwrap().detail.relationship.state,
        RelationshipState::Ambiguous
    );
    assert_eq!(
        s.content(&tool.id).unwrap().output.text.as_deref(),
        Some("fabricated-result")
    );
    let detail = s.node(&tool.id).unwrap();
    let source = detail
        .detail
        .tool_inspection
        .unwrap()
        .result
        .preview
        .source
        .unwrap();
    assert!(matches!(&source,NativeSource::Jsonl{line,..} if line=="2"));
    assert!(detail.detail.sources.contains(&source));
}

#[test]
fn codex_history_physical_prefix_and_logical_inheritance_are_independent() {
    for (same_thread, boundary, expected, inherited) in [
        (true, None, "112", 0),
        (false, Some(2), "12", 1),
        (false, None, "112", 0),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let parent = temp.path().join("archived_sessions").join(ROLLOUT);
        let response = |id: &str, ordinal, input| json!({"type":"token_usage_record","ordinal":ordinal,"payload":{"response_id":id,"usage":{"input_tokens":input,"cached_input_tokens":0,"cache_write_input_tokens":0,"output_tokens":0}}});
        write(
            &parent,
            &[
                codex_header(),
                response("inherited", 1, 100),
                response("same-thread", 3, 10),
            ],
        );
        let len = fs::metadata(&parent).unwrap().len();
        let thread = if same_thread {
            "00000000-0000-0000-0000-000000000001"
        } else {
            "00000000-0000-0000-0000-000000000002"
        };
        let child = temp.path().join("sessions").join(format!(
            "rollout-2026-01-01T00-00-01-{thread}_00000000-0000-0000-0000-000000000003.jsonl"
        ));
        let mut header = json!({"type":"session_meta","ordinal":0,"payload":{"id":thread,"history_mode":"paginated","history_base":{"thread_id":"00000000-0000-0000-0000-000000000001","end_ordinal_exclusive":4,"end_byte_offset":len}}});
        if !same_thread {
            header["payload"]["forked_from_id"] = json!("00000000-0000-0000-0000-000000000001");
        }
        if let Some(boundary) = boundary {
            header["payload"]["forked_from_ordinal_exclusive"] = json!(boundary);
        }
        write(&child, &[header, response("new", 4, 2)]);
        let mut store =
            NativeStore::open(Provider::Codex, temp.path(), NativeLimits::default()).unwrap();
        let d = discover(&mut store)
            .into_iter()
            .find(|d| d.recording_id.ends_with('3'))
            .unwrap();
        let snapshot = settled(&mut store.reader(&d).unwrap());
        assert_eq!(total(&snapshot).as_deref(), Some(expected));
        let row = snapshot
            .observations(&ObservationQuery::default())
            .unwrap()
            .items
            .remove(0);
        let detail = snapshot.node(&row.id).unwrap();
        assert_eq!(
            detail
                .native_usage
                .iter()
                .filter(|e| matches!(e.role, UsageRole::Inherited { .. }))
                .count(),
            inherited
        );
        if !same_thread && boundary.is_none() {
            assert_eq!(
                snapshot
                    .overview()
                    .unwrap()
                    .token_accounting
                    .total_tokens
                    .state,
                TokenState::Partial
            );
        }
    }
}
#[test]
fn codex_invalid_execution_boundaries_do_not_overflow_or_invent_duration() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("sessions").join(ROLLOUT);
    let item = |id: &str, start: i64, end: i64| json!({"type":"event_msg","payload":{"type":"item_completed","started_at_ms":start,"completed_at_ms":end,"item":{"type":"AgentMessage","id":id,"content":[]}}});
    write(
        &path,
        &[
            codex_header(),
            item("overflow", i64::MIN, i64::MAX),
            item("backward", 200, 100),
        ],
    );
    let mut store =
        NativeStore::open(Provider::Codex, temp.path(), NativeLimits::default()).unwrap();
    let d = discover(&mut store).remove(0);
    let s = settled(&mut store.reader(&d).unwrap());
    let rows = s.observations(&ObservationQuery::default()).unwrap();
    for row in rows.items.iter().filter(|r| r.kind != NodeKind::Session) {
        assert_eq!(row.duration.state, MeasurementState::Invalid);
        assert_eq!(row.duration.value_ms, None);
    }
    assert_eq!(
        s.diagnostics(&DiagnosticQuery::default())
            .unwrap()
            .items
            .iter()
            .filter(|d| d.code == "invalid_timing")
            .count(),
        2
    );
}
#[test]
fn codex_history_search_yields_and_limits_one_large_directory() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("sessions").join(ROLLOUT);
    write(
        &path,
        &[
            json!({"type":"session_meta","payload":{"id":"00000000-0000-0000-0000-000000000001","history_base":{"thread_id":"missing","end_byte_offset":1}}}),
        ],
    );
    let mut store =
        NativeStore::open(Provider::Codex, temp.path(), NativeLimits::default()).unwrap();
    let d = discover(&mut store).remove(0);
    let entries = temp.path().join("sessions/many");
    fs::create_dir(&entries).unwrap();
    for i in 0..100_001 {
        fs::File::create(entries.join(i.to_string())).unwrap();
    }
    let mut reader = store.reader(&d).unwrap();
    assert_eq!(reader.poll().unwrap().progress, NativeProgress::CatchingUp);
    assert_eq!(reader.read_records(), 0);
    let mut polls = 1;
    loop {
        polls += 1;
        match reader.poll() {
            Ok(u) => assert_eq!(u.progress, NativeProgress::CatchingUp),
            Err(e) => {
                assert_eq!(e.code, "history_discovery_limit");
                break;
            }
        }
        assert!(polls < 205);
    }
    assert!(polls > 190);
    assert_eq!(reader.read_records(), 0);
}

#[test]
fn opencode_session_aggregate_is_reconciliation_with_column_provenance() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("custom.sqlite");
    let mut writer = DatabaseWriter::start(&path);
    writer.command("aggregate");
    let initial = db_bytes(&path);
    let mut store = NativeStore::open(Provider::Opencode, &path, NativeLimits::default()).unwrap();
    let d = discover(&mut store).remove(0);
    let s = settled(&mut store.reader(&d).unwrap());
    assert_eq!(total(&s).as_deref(), Some("120"));
    let row = s
        .observations(&ObservationQuery::default())
        .unwrap()
        .items
        .remove(0);
    let detail = s.node(&row.id).unwrap();
    let evidence = detail
        .native_usage
        .iter()
        .find(|e| matches!(&e.role,UsageRole::Reconciliation{scope} if scope=="sessionAggregate"))
        .unwrap();
    assert!(!evidence.accepted);
    assert!(evidence.native_counters.iter().any(|c|matches!(&c.source,NativeSource::Sqlite{table,field:Some(FieldPath::Column{column}),..} if table=="session_v2"&&column=="tokens_input")));
    assert_eq!(initial, db_bytes(&path));
}
#[test]
fn opencode_missing_fork_boundary_is_partial_without_invented_prefix() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("opencode.db");
    let mut writer = DatabaseWriter::start(&path);
    writer.command("fork");
    writer.command("missing-fork-boundary");
    let mut store = NativeStore::open(Provider::Opencode, &path, NativeLimits::default()).unwrap();
    let d = discover(&mut store)
        .into_iter()
        .find(|d| d.native_id == "child")
        .unwrap();
    let s = settled(&mut store.reader(&d).unwrap());
    assert_eq!(total(&s).as_deref(), Some("15"));
    assert_eq!(
        s.overview().unwrap().token_accounting.total_tokens.state,
        TokenState::Partial
    );
    assert!(
        s.diagnostics(&DiagnosticQuery::default())
            .unwrap()
            .items
            .iter()
            .any(|d| d.code == "fork_boundary_unavailable")
    );
}
#[test]
fn opencode_missing_wal_shm_is_refused_without_creating_sidecars() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("opencode.db");
    let _writer = DatabaseWriter::start(&path);
    let shm = temp.path().join("opencode.db-shm");
    fs::rename(&shm, temp.path().join("preserved-shm")).unwrap();
    let before = fs::read(&path).unwrap();
    let wal = fs::read(temp.path().join("opencode.db-wal")).unwrap();
    let mut store = NativeStore::open(Provider::Opencode, &path, NativeLimits::default()).unwrap();
    let batch = store.discover(10).unwrap();
    assert!(batch.sessions.is_empty());
    assert!(
        batch
            .diagnostics
            .iter()
            .any(|d| d.code == "wal_sidecar_unavailable")
    );
    assert!(!shm.exists());
    assert_eq!(before, fs::read(&path).unwrap());
    assert_eq!(wal, fs::read(temp.path().join("opencode.db-wal")).unwrap());
}

#[test]
fn codex_history_prefix_rejects_short_source_and_split_record_boundary() {
    for extra in [-2i64, 2] {
        let temp = tempfile::tempdir().unwrap();
        let parent = temp.path().join("archived_sessions").join(ROLLOUT);
        write(
            &parent,
            &[
                codex_header(),
                json!({"type":"token_usage_record","ordinal":1,"payload":{"response_id":"a","usage":{"input_tokens":1,"output_tokens":0,"cached_input_tokens":0,"cache_write_input_tokens":0}}}),
            ],
        );
        let end = (fs::metadata(&parent).unwrap().len() as i64 + extra) as u64;
        let child=temp.path().join("sessions/rollout-2026-01-01T00-00-01-00000000-0000-0000-0000-000000000001_00000000-0000-0000-0000-000000000002.jsonl");
        write(
            &child,
            &[
                json!({"type":"session_meta","payload":{"id":"00000000-0000-0000-0000-000000000001","history_base":{"thread_id":"00000000-0000-0000-0000-000000000001","end_byte_offset":end}}}),
            ],
        );
        let mut store =
            NativeStore::open(Provider::Codex, temp.path(), NativeLimits::default()).unwrap();
        let d = discover(&mut store)
            .into_iter()
            .find(|d| d.recording_id.ends_with('2'))
            .unwrap();
        let mut reader = store.reader(&d).unwrap();
        let mut error = None;
        for _ in 0..20 {
            match reader.poll() {
                Ok(u) => assert_eq!(u.progress, NativeProgress::CatchingUp),
                Err(e) => {
                    error = Some(e);
                    break;
                }
            }
        }
        assert_eq!(error.unwrap().code, "history_boundary");
        assert!(reader.snapshot().is_none());
    }
}
