import { appendFile, mkdir, readdir, readFile, stat, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { resolve, dirname, relative } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { zstdCompressSync } from 'node:zlib';

export const providers = ['codex', 'pi', 'claude', 'opencode'];
export const fabricatedBody = 'FABRICATED-ONLY-secret <img src="https://example.invalid/trace-probe">';
const timestamp = '2026-10-30T09:00:00.000Z';
const epoch = Date.parse(timestamp);
const line = (value) => `${JSON.stringify(value)}\n`;
const uuid = (key) => {
  const h = createHash('sha256').update(key).digest('hex');
  return `${h.slice(0,8)}-${h.slice(8,12)}-4${h.slice(13,16)}-a${h.slice(17,20)}-${h.slice(20,32)}`;
};
const piUsage = (amount) => ({ input: amount - 50, output: 30, cacheRead: 20, cacheWrite: 0, totalTokens: amount });
const codexUsage = (amount, count = 1) => ({ input_tokens: amount - 30 * count, cached_input_tokens: 20 * count, cache_write_input_tokens: 0, output_tokens: 30 * count, reasoning_output_tokens: 5 * count, total_tokens: amount });

export async function createStores(directory) {
  const roots = Object.fromEntries(providers.map((provider) => [provider, resolve(directory, `${provider} store`)]));
  for (const root of Object.values(roots)) await mkdir(root, { recursive: true });
  const database = resolve(roots.opencode, 'opencode.db');
  const db = new DatabaseSync(database);
  db.exec(`PRAGMA journal_mode=WAL;
CREATE TABLE session_v2 (
 id TEXT PRIMARY KEY, project_id TEXT NOT NULL, workspace_id TEXT, parent_id TEXT,
 fork_session_id TEXT, fork_boundary TEXT, slug TEXT NOT NULL, directory TEXT NOT NULL,
 path TEXT, title TEXT, version TEXT NOT NULL, cost REAL NOT NULL DEFAULT 0,
 tokens_input INTEGER NOT NULL DEFAULT 0, tokens_output INTEGER NOT NULL DEFAULT 0,
 tokens_reasoning INTEGER NOT NULL DEFAULT 0, tokens_cache_read INTEGER NOT NULL DEFAULT 0,
 tokens_cache_write INTEGER NOT NULL DEFAULT 0, metadata TEXT, model TEXT,
 time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL, time_idle INTEGER,
 idle_outcome TEXT, time_archived INTEGER
);
CREATE TABLE session_message (
 id TEXT PRIMARY KEY, session_id TEXT NOT NULL, type TEXT NOT NULL, seq INTEGER NOT NULL,
 time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL, data TEXT NOT NULL,
 UNIQUE(session_id, seq)
);
CREATE TABLE event (
 id TEXT PRIMARY KEY, aggregate_id TEXT NOT NULL, seq INTEGER NOT NULL,
 created INTEGER NOT NULL DEFAULT 0, type TEXT NOT NULL, data TEXT NOT NULL,
 UNIQUE(aggregate_id, seq)
);`);
  const sessions = new Map();

  async function createSession(provider, key, { compressed = false, project = '/synthetic/project-a' } = {}) {
    const id = uuid(key);
    const session = { provider, key, id, sequence: 0, total: 0, project, source: null };
    if (provider === 'codex') {
      session.source = resolve(roots.codex, compressed ? 'archived_sessions' : 'sessions/2026/10/30', `rollout-2026-10-30T09-00-00-${id}.jsonl${compressed ? '.zst' : ''}`);
      const header = line({ type: 'session_meta', timestamp, payload: { id, session_id: id, timestamp, cwd: project, cli_version: '0.166.0', originator: 'synthetic-fixture', source: 'cli', history_mode: 'legacy', model_provider: 'synthetic' } });
      await mkdir(dirname(session.source), { recursive: true });
      await writeFile(session.source, compressed ? zstdCompressSync(Buffer.from(header)) : header);
    } else if (provider === 'pi') {
      session.source = resolve(roots.pi, '--synthetic-project-a--', `${timestamp.replaceAll(':','-')}_${id}.jsonl`);
      await mkdir(dirname(session.source), { recursive: true });
      await writeFile(session.source, line({ type: 'session', version: 3, id, timestamp, cwd: project }));
    } else if (provider === 'claude') {
      session.source = resolve(roots.claude, '-synthetic-project-a', `${id}.jsonl`);
      await mkdir(dirname(session.source), { recursive: true });
      await writeFile(session.source, line({ type: 'user', uuid: `user-${id}`, parentUuid: null, sessionId: id, cwd: project, timestamp, version: '2.1.216', message: { role: 'user', content: fabricatedBody } }));
    } else if (provider === 'opencode') {
      session.source = database;
      db.prepare('INSERT INTO session_v2(id,project_id,slug,directory,version,time_created,time_updated) VALUES(?,?,?,?,?,?,?)').run(id, 'synthetic-project-a', `session-${key}`, project, '2.1.4', epoch, epoch);
    } else throw new Error(`Unknown fixture provider ${provider}`);
    sessions.set(`${provider}:${key}`, session);
    return session;
  }

  async function appendUsage(provider, key, amount = 130) {
    const session = sessions.get(`${provider}:${key}`);
    if (!session) throw new Error('Create the fabricated session before appending.');
    if (session.source.endsWith('.zst')) throw new Error('Archived fixture is closed.');
    const seq = ++session.sequence;
    const time = epoch + seq * 100;
    const iso = new Date(time).toISOString();
    session.total += amount;
    let record;
    if (provider === 'codex') {
      record = { type: 'event_msg', timestamp: iso, payload: { type: 'token_usage_record', thread_id: session.id, session_id: session.id, turn_id: `turn-${seq}`, root_turn_id: `turn-${seq}`, response_id: `response-${seq}`, usage: codexUsage(amount), turn_token_usage: codexUsage(amount), thread_token_usage: codexUsage(session.total, seq) } };
    } else if (provider === 'pi') {
      record = { type: 'message', id: `entry-${seq}`, parentId: seq === 1 ? null : `entry-${seq-1}`, timestamp: iso, message: { role: 'assistant', content: [{ type: 'text', text: fabricatedBody }], provider: 'synthetic', model: 'synthetic-model', api: 'synthetic', usage: piUsage(amount), stopReason: 'stop', timestamp: time, durationMs: 25 } };
    } else if (provider === 'claude') {
      record = { type: 'assistant', uuid: `assistant-${seq}`, parentUuid: seq === 1 ? `user-${session.id}` : `assistant-${seq-1}`, sessionId: session.id, cwd: session.project, timestamp: iso, message: { id: `message-${seq}`, type: 'message', role: 'assistant', model: 'synthetic-model', content: [{ type: 'text', text: fabricatedBody }], stop_reason: 'end_turn', usage: { input_tokens: amount - 50, output_tokens: 30, cache_read_input_tokens: 20, cache_creation_input_tokens: 0 } } };
    } else {
      const data = { agent: 'synthetic', model: { providerID: 'synthetic', id: 'synthetic-model' }, content: [{ type: 'text', text: fabricatedBody }], finish: 'stop', cost: 0, tokens: { input: amount - 50, output: 25, reasoning: 5, cache: { read: 20, write: 0 } }, time: { created: time, streamed: time+20, completed: time+25 } };
      db.exec('BEGIN IMMEDIATE');
      try {
        db.prepare('INSERT INTO session_message VALUES(?,?,?,?,?,?,?)').run(`msg_${session.id}_${seq}`, session.id, 'assistant', seq, time, time, JSON.stringify(data));
        db.prepare('UPDATE session_v2 SET time_updated=?,tokens_input=tokens_input+?,tokens_output=tokens_output+25,tokens_reasoning=tokens_reasoning+5,tokens_cache_read=tokens_cache_read+20 WHERE id=?').run(time, amount - 50, session.id);
        db.exec('COMMIT');
      } catch (error) { db.exec('ROLLBACK'); throw error; }
    }
    if (record) await appendFile(session.source, line(record));
    return { provider, key, id: session.id, sequence: seq, total: session.total, committedAt: Date.now() };
  }

  async function appendTool(provider, key, completed = false) {
    const session = sessions.get(`${provider}:${key}`);
    if (!session || session.sequence !== 1) throw new Error('Tool fixture expects one initial response.');
    const time = epoch + 200;
    const iso = new Date(time).toISOString();
    const id = 'synthetic-tool-1';
    const name = 'synthetic_tool';
    const input = { probe: fabricatedBody };
    let record;
    if (provider === 'codex') {
      record = { type: 'response_item', timestamp: iso, payload: completed
        ? { type: 'function_call_output', call_id: id, output: fabricatedBody }
        : { type: 'function_call', call_id: id, name, arguments: JSON.stringify(input) } };
    } else if (provider === 'pi') {
      record = completed
        ? { type: 'message', id: 'tool-result', parentId: 'entry-1', timestamp: iso, message: { role: 'toolResult', toolCallId: id, toolName: name, content: [{ type: 'text', text: fabricatedBody }], isError: false, timestamp: time, durationMs: 5 } }
        : { type: 'message', id: 'entry-1', parentId: null, timestamp: iso, message: { role: 'assistant', content: [{ type: 'toolCall', id, name, arguments: input }], model: 'synthetic-model', usage: piUsage(130), stopReason: 'toolUse', timestamp: time, durationMs: 25 } };
    } else if (provider === 'claude') {
      record = completed
        ? { type: 'user', uuid: 'tool-result', parentUuid: 'assistant-1', sessionId: session.id, timestamp: iso, message: { role: 'user', content: [{ type: 'tool_result', tool_use_id: id, content: fabricatedBody, is_error: false }] } }
        : { type: 'assistant', uuid: 'assistant-1', parentUuid: `user-${session.id}`, sessionId: session.id, cwd: session.project, timestamp: iso, message: { id: 'message-1', type: 'message', role: 'assistant', model: 'synthetic-model', content: [{ type: 'tool_use', id, name, input }], usage: { input_tokens: 80, output_tokens: 30, cache_read_input_tokens: 20, cache_creation_input_tokens: 0 } } };
    } else if (provider === 'opencode') {
      const rowId = `msg_${session.id}_1`;
      const data = JSON.parse(db.prepare('SELECT data FROM session_message WHERE id=?').get(rowId).data);
      data.content = [{ type: 'tool', id, name, state: { status: completed ? 'completed' : 'running', input, ...(completed ? { content: fabricatedBody } : {}) }, time: { ran: time, ...(completed ? { completed: time + 5 } : {}) } }];
      db.prepare('UPDATE session_message SET data=? WHERE id=?').run(JSON.stringify(data), rowId);
    } else throw new Error(`Unknown fixture provider ${provider}`);
    if (record) await appendFile(session.source, line(record));
    return { provider, id, completed, committedAt: Date.now() };
  }

  function changeSchema(unsupported) {
    db.exec(unsupported ? 'ALTER TABLE event RENAME TO event_retained' : 'ALTER TABLE event_retained RENAME TO event');
  }

  async function fingerprint() {
    const result = {};
    async function visit(path) {
      for (const entry of await readdir(path, { withFileTypes: true })) {
        const child = resolve(path, entry.name);
        if (entry.isDirectory()) await visit(child);
        else if (entry.isFile()) {
          const [bytes, metadata] = await Promise.all([readFile(child), stat(child)]);
          result[relative(directory, child)] = { sha256: createHash('sha256').update(bytes).digest('hex'), size: bytes.length, mode: metadata.mode };
        }
      }
    }
    await visit(directory);
    return result;
  }

  return { roots, database, sessions, createSession, appendUsage, appendTool, changeSchema, fingerprint, close: () => db.close(), args: ['system', ...providers.flatMap((provider) => ['--root', `${provider}=${roots[provider]}`])] };
}
