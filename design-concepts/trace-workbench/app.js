// Synthetic application views. No upstream JSONL parsing occurs in this prototype.
const observations = [
  { id: 'session', parent: null, type: 'session', name: 'Fix authentication flow', start: 0, end: 42.8, line: 1, outcome: 'completed', timing: 'derived', description: 'Session 8f2c41a', input: 'Fix the expired-session redirect and verify the authentication tests.', output: 'Authentication flow fixed. All 12 tests pass.' },
  { id: 'turn-1', parent: 'session', type: 'turn', name: 'Turn 1', start: 0, end: 19.6, line: 3, outcome: 'completed', timing: 'derived', description: 'Inspect, edit, and run the tests', input: 'Investigate the expired-session redirect.', output: 'A test failed. Continue with a corrected redirect.' },
  { id: 'gen-1', parent: 'turn-1', type: 'generation', name: 'llm-generation', start: .3, end: 5.9, line: 5, outcome: 'ok', timing: 'recorded', inputTokens: 8400, outputTokens: 580, description: 'claude-sonnet-4-5 / Step 1', input: 'Inspect the authentication implementation and tests.', output: 'Read the session handler and locate authentication tests.', stop: 'tool_use' },
  { id: 'read-1', parent: 'turn-1', type: 'tool', name: 'read', start: 6.1, end: 6.5, line: 9, outcome: 'ok', timing: 'recorded', description: 'src/auth/session.ts', input: '{\n  "path": "src/auth/session.ts"\n}', output: 'export function requireSession(request) {\n  const session = getSession(request);\n  if (!session) return redirect("/login");\n  return session;\n}' },
  { id: 'grep-1', parent: 'turn-1', type: 'tool', name: 'grep', start: 6.6, end: 7.3, line: 11, outcome: 'ok', timing: 'recorded', description: 'Find authentication tests', input: '{ "pattern": "expired session", "path": "tests/" }', output: 'tests/auth.test.ts:24: redirects expired sessions to /auth/login' },
  { id: 'gen-2', parent: 'turn-1', type: 'generation', name: 'llm-generation', start: 7.6, end: 13.1, line: 14, outcome: 'ok', timing: 'recorded', inputTokens: 9120, outputTokens: 920, description: 'claude-sonnet-4-5 / Step 2', input: 'The redirect path differs from the expected authentication route.', output: 'Update the session redirect and execute the authentication tests.', stop: 'tool_use' },
  { id: 'edit-1', parent: 'turn-1', type: 'tool', name: 'edit', start: 13.4, end: 13.8, line: 18, outcome: 'ok', timing: 'recorded', description: 'src/auth/session.ts', input: '{ "path": "src/auth/session.ts", "oldText": "/login", "newText": "/auth" }', output: 'Updated src/auth/session.ts.' },
  { id: 'test-1', parent: 'turn-1', type: 'tool', name: 'bash', start: 14.1, end: 19.4, line: 21, outcome: 'error', timing: 'recorded', description: 'npm test -- auth', input: '{\n  "command": "npm test -- auth",\n  "timeout": 120\n}', output: 'FAIL tests/auth.test.ts\n\n× redirects expired sessions\n\nExpected: "/auth/login"\nReceived: "/auth"\n\nTests: 1 failed, 11 passed\nProcess exited with code 1', recovery: 'test-2' },
  { id: 'turn-2', parent: 'session', type: 'turn', name: 'Turn 2', start: 20, end: 42.8, line: 26, outcome: 'completed', timing: 'derived', description: 'Correct the redirect and verify', input: 'The test expects the full /auth/login route.', output: 'Corrected the route and verified all authentication tests.' },
  { id: 'gen-3', parent: 'turn-2', type: 'generation', name: 'llm-generation', start: 20.3, end: 26.7, line: 28, outcome: 'ok', timing: 'recorded', inputTokens: 11400, outputTokens: 1040, description: 'claude-sonnet-4-5 / Step 3', input: 'The test failed because the redirect route is incomplete.', output: 'Read the route declaration and fix the path to /auth/login.', stop: 'tool_use' },
  { id: 'read-2', parent: 'turn-2', type: 'tool', name: 'read', start: 27, end: 27.4, line: 32, outcome: 'ok', timing: 'recorded', description: 'src/auth/routes.ts', input: '{ "path": "src/auth/routes.ts" }', output: 'export const LOGIN_ROUTE = "/auth/login";' },
  { id: 'edit-2', parent: 'turn-2', type: 'tool', name: 'edit', start: 27.8, end: 28.4, line: 35, outcome: 'ok', timing: 'recorded', description: 'src/auth/session.ts', input: '{ "path": "src/auth/session.ts", "oldText": "/auth", "newText": "/auth/login" }', output: 'Updated src/auth/session.ts.' },
  { id: 'test-2', parent: 'turn-2', type: 'tool', name: 'bash', start: 29, end: 39.3, line: 38, outcome: 'ok', timing: 'recorded', description: 'npm test -- auth', input: '{\n  "command": "npm test -- auth",\n  "timeout": 120\n}', output: 'PASS tests/auth.test.ts\n\n✓ redirects expired sessions\n✓ preserves active sessions\n✓ clears invalid cookies\n\nTests: 12 passed, 12 total\nProcess exited with code 0' },
  { id: 'gen-4', parent: 'turn-2', type: 'generation', name: 'llm-generation', start: 39.7, end: 42.6, line: 41, outcome: 'ok', timing: 'recorded', inputTokens: 12400, outputTokens: 460, description: 'claude-sonnet-4-5 / Step 4', input: 'All 12 authentication tests passed.', output: 'Fixed the expired-session redirect to /auth/login. All authentication tests pass.', stop: 'end_turn' },
];
const byId = new Map(observations.map(node => [node.id, node]));
const session = byId.get('session');
const generations = observations.filter(node => node.type === 'generation');
const generationNames = new Map(generations.map((node, index) => [node.id, `Generation ${index + 1}`]));
const tools = observations.filter(node => node.type === 'tool');
const totalInput = generations.reduce((sum, node) => sum + node.inputTokens, 0);
const totalOutput = generations.reduce((sum, node) => sum + node.outputTokens, 0);
const params = new URLSearchParams(location.hash.slice(1));
const state = {
  selected: byId.has(params.get('node')) ? params.get('node') : 'test-1',
  query: params.get('q') || '',
  filter: ['error', 'tool', 'slow'].includes(params.get('filter')) ? params.get('filter') : null,
  view: params.get('view') === 'list' ? 'list' : 'waterfall',
  tab: params.get('tab') === 'metadata' ? 'metadata' : 'io',
  theme: params.get('theme') === 'dark' ? 'dark' : 'light',
  collapsed: new Set(), revealed: new Set(), page: 'trace', preview: 'full',
};
let visibleNodes = [], toastTimer, loadingTimer;
const $ = selector => document.querySelector(selector);
const icon = (name, className = '') => `<svg class="${className}" aria-hidden="true"><use href="#i-${name}"/></svg>`;
const duration = node => `${(node.end - node.start).toFixed(1)} s`;
const formatTokens = value => `${(value / 1000).toFixed(1)}k`;
const typeIcons = { session: 'session', turn: 'turn', generation: 'spark', tool: 'tool' };

function depth(node) {
  let level = 0;
  while (node.parent) { level++; node = byId.get(node.parent); }
  return level;
}
function updateUrl() {
  const next = new URLSearchParams(); next.set('node', state.selected);
  if (state.query) next.set('q', state.query);
  if (state.filter) next.set('filter', state.filter);
  if (state.view !== 'waterfall') next.set('view', state.view);
  if (state.tab !== 'io') next.set('tab', state.tab);
  if (state.theme !== 'light') next.set('theme', state.theme);
  history.replaceState(null, '', `#${next}`);
}
function matches(node) {
  const query = state.query.toLowerCase().trim();
  const text = `${node.name} ${node.type} ${node.id} ${node.description} ${generationNames.get(node.id) || ''}`.toLowerCase();
  return (!query || text.includes(query)) && (!state.filter ||
    (state.filter === 'error' && node.outcome === 'error') ||
    (state.filter === 'tool' && node.type === 'tool') ||
    (state.filter === 'slow' && !['session', 'turn'].includes(node.type) && node.end - node.start > 5));
}
function filteredNodes() {
  const keep = new Set();
  observations.filter(matches).forEach(node => {
    keep.add(node.id);
    while (node.parent) { keep.add(node.parent); node = byId.get(node.parent); }
  });
  return observations.filter(node => {
    if (!keep.has(node.id)) return false;
    if (state.query || state.filter) return true;
    while (node.parent) {
      if (state.collapsed.has(node.parent)) return false;
      node = byId.get(node.parent);
    }
    return true;
  });
}
function selectNode(id, focus = false) {
  if (!byId.has(id)) return;
  state.selected = id; state.page = 'trace'; render();
  if (focus) document.querySelector(`[data-node="${id}"]`)?.focus({ preventScroll: true });
}
function renderRows() {
  const container = $('#trace-rows'); container.classList.toggle('list-view', state.view === 'list');
  visibleNodes = filteredNodes();
  if (state.preview !== 'full') {
    const options = {
      empty: ['file', 'Your next investigation starts here', 'Open the synthetic example to explore a session.', 'Open example trace'],
      error: ['warning', 'Could not import the recording', 'example/events.jsonl:24 contains an incomplete JSON record.', 'Retry with example'],
      loading: ['clock', 'Preparing execution views', 'Loading preview. The example will appear in a moment.', 'Show example now'],
    };
    const [mark, title, message, action] = options[state.preview];
    container.innerHTML = `<div class="empty-state" ${state.preview === 'loading' ? 'aria-busy="true"' : ''}>${icon(mark)}<h2>${title}</h2><p>${message}</p><button class="primary-button" id="open-example">${action}</button></div>`;
    $('#open-example').onclick = () => { clearTimeout(loadingTimer); state.preview = 'full'; $('#preview-state').value = 'full'; render(); };
    $('#span-count').textContent = '0';
    $('#result-count').textContent = state.preview === 'loading' ? 'Loading synthetic example…' : 'No trace loaded';
    return;
  }
  if (!visibleNodes.length) {
    container.innerHTML = `<div class="empty-state">${icon('search')}<h2>No matching observations</h2><p>Try another name, type, or observation ID.</p><button class="quiet-button" id="clear-filters">Clear filters</button></div>`;
    $('#clear-filters').onclick = reset;
  } else {
    container.innerHTML = visibleNodes.map(node => {
      const isParent = ['session', 'turn'].includes(node.type);
      const nodeIcon = node.outcome === 'error' ? 'error' : typeIcons[node.type];
      const name = node.type === 'session' ? 'Session' : generationNames.get(node.id) || node.name;
      const context = node.type === 'tool' ? node.description.split('/').pop() : '';
      const left = node.start / session.end * 100, width = Math.max((node.end - node.start) / session.end * 100, .8);
      const chart = state.view === 'waterfall'
        ? `<div class="chart-track"><span class="span-bar ${node.type} ${node.outcome === 'error' ? 'error' : ''}" style="--start:${left}%;--size:${width}%"></span></div>`
        : `<span class="list-cell">${node.type}</span><span class="list-cell">+${node.start.toFixed(1)} s</span><span class="pill ${node.outcome === 'error' ? 'error' : ''}">${node.outcome}</span>`;
      return `<button class="trace-row" data-node="${node.id}" data-type="${node.type}" aria-pressed="${state.selected === node.id}" aria-label="${name}, ${node.description}, ${duration(node)}, ${node.outcome}" title="${node.description}${isParent ? '. Double-click to collapse or expand.' : ''}" style="--depth:${depth(node)}"><span class="row-name">${isParent ? icon(state.collapsed.has(node.id) ? 'chevron' : 'down', 'chevron') : ''}${icon(nodeIcon, `node-icon ${node.type === 'generation' ? 'generation-icon' : ''} ${node.outcome === 'error' ? 'error-icon' : ''}`)}<span class="name-text"><span class="node-name">${name}</span>${context ? `<span class="node-context">${context}</span>` : ''}</span><span class="duration">${duration(node)}</span></span><span class="row-chart">${chart}</span></button>`;
    }).join('');
    container.querySelectorAll('[data-node]').forEach(button => {
      button.onclick = event => {
        if (event.target.closest('.chevron')) toggleCollapse(button.dataset.node);
        else selectNode(button.dataset.node, true);
      };
      button.ondblclick = () => { if (['turn', 'session'].includes(byId.get(button.dataset.node).type)) toggleCollapse(button.dataset.node); };
    });
  }
  const matchCount = observations.filter(matches).length;
  $('#result-count').textContent = state.query || state.filter ? `${matchCount} matched / ${visibleNodes.length} shown with parents` : `${observations.length} observations / 1 recovered error`;
  $('#span-count').textContent = visibleNodes.length;
}
function renderInspector() {
  const node = byId.get(state.selected), revealed = state.revealed.has(node.id);
  const isError = node.outcome === 'error', isGeneration = node.type === 'generation';
  const kind = isGeneration ? 'Generation' : node.type === 'tool' ? 'Tool call' : node.type === 'turn' ? 'Turn' : 'Session';
  const status = isError ? 'Failed' : node.outcome === 'completed' ? 'Completed' : 'Successful';
  const body = $('#inspector-body');
  if (state.preview !== 'full') {
    body.innerHTML = '<div class="empty-state"><h3>No observation selected</h3><p>Details appear when a trace is loaded.</p></div>';
    $('#selection-notice').hidden = true; return;
  }
  body.innerHTML = `<div class="detail-main"><div class="node-title"><h3>${isGeneration ? 'LLM generation' : node.type === 'session' ? 'Session' : node.name}</h3><span class="node-kind">${kind}</span></div><p class="node-description">${node.description}</p><span class="node-status ${isError ? 'attention' : ''}">${icon(isError ? 'error' : 'check')}${status}</span><dl class="detail-stats"><div><dt>Duration</dt><dd>${duration(node)} <small>${node.timing}</small></dd></div><div><dt>${isGeneration ? 'Total tokens' : 'Start offset'}</dt><dd>${isGeneration ? formatTokens(node.inputTokens + node.outputTokens) : `+${node.start.toFixed(1)} <small>s</small>`}</dd></div></dl></div><div class="detail-content"><div class="inspector-tabs" aria-label="Detail view"><button data-tab="io" aria-pressed="${state.tab === 'io'}">Input & output</button><button data-tab="metadata" aria-pressed="${state.tab === 'metadata'}">Metadata</button></div><div id="detail-panel"></div></div><div class="detail-bottom">${node.recovery ? `<div class="recovery"><p>${icon('check')}Recovered in Turn 2</p><button id="recovery">Inspect the successful retry</button></div>` : ''}<div class="provenance">${icon('file')}example/events.jsonl:${node.line}</div></div>`;
  const panel = $('#detail-panel');
  if (state.tab === 'io') {
    if (revealed) {
      panel.innerHTML = `<div class="field-heading">Input <span>${isGeneration ? 'Prompt' : 'Arguments'}</span></div><div class="content-box" id="input-content"></div><div class="field-heading">Output <span>${isError ? 'Exit code 1' : isGeneration ? node.stop : 'Result'}</span></div><div class="content-box ${isError ? 'failed' : ''}" id="output-content"></div>`;
      ['input', 'output'].forEach(field => {
        const pre = document.createElement('pre'); pre.textContent = node[field]; $(`#${field}-content`).append(pre);
      });
      const hide = document.createElement('button'); hide.className = 'quiet-button'; hide.style.marginTop = '10px'; hide.textContent = 'Hide content';
      hide.onclick = () => { state.revealed.delete(node.id); renderInspector(); }; panel.append(hide);
    } else {
      panel.innerHTML = `<div class="locked-content"><span>Input and output hidden</span><button data-reveal="${node.id}">Reveal content</button></div>`;
      panel.querySelector('[data-reveal]').onclick = () => { state.revealed.add(node.id); renderInspector(); };
    }
  } else {
    const fields = [ ['Observation ID', node.id], ['Parent', node.parent || 'None'], ['Source', `events.jsonl:${node.line}`], ['Timing origin', node.timing === 'recorded' ? 'Recorded duration' : 'Derived from timestamps'], ['Outcome', node.outcome], ['Recorded cost', 'Unknown'], ...(isGeneration ? [['Input tokens', node.inputTokens.toLocaleString('en-US')], ['Output tokens', node.outputTokens.toLocaleString('en-US')], ['First token time', 'Unknown'], ['Stop reason', node.stop]] : []) ];
    const list = document.createElement('dl'); list.className = 'metadata';
    for (const [key, value] of fields) {
      const row = document.createElement('div'), dt = document.createElement('dt'), dd = document.createElement('dd');
      dt.textContent = key; dd.textContent = value; row.append(dt, dd); list.append(row);
    }
    panel.append(list);
  }
  body.querySelectorAll('[data-tab]').forEach(button => button.onclick = () => { state.tab = button.dataset.tab; renderInspector(); updateUrl(); });
  $('#recovery')?.addEventListener('click', () => {
    state.filter = null; state.query = ''; state.collapsed.clear(); selectNode(node.recovery, true);
    $('.inspector').scrollIntoView({ block: 'nearest' });
  });
  $('#selection-notice').hidden = visibleNodes.some(item => item.id === node.id);
}
function render() {
  document.documentElement.dataset.theme = state.theme; $('#search').value = state.query;
  document.querySelectorAll('[data-filter]').forEach(button => button.setAttribute('aria-pressed', button.dataset.filter === state.filter));
  document.querySelectorAll('[data-view]').forEach(button => button.setAttribute('aria-pressed', button.dataset.view === state.view));
  document.querySelectorAll('[data-page]').forEach(button => button.setAttribute('aria-pressed', button.dataset.page === state.page));
  $('#trace-page').hidden = state.page !== 'trace'; $('#diagnostics-page').hidden = state.page !== 'diagnostics';
  $('#ruler').innerHTML = state.view === 'waterfall' ? '<span>0 s</span><span>10.7 s</span><span>21.4 s</span><span>32.1 s</span><span>42.8 s</span>' : '<span>Type</span><span>Start</span><span>Outcome</span>';
  $('#ruler').setAttribute('aria-label', state.view === 'waterfall' ? 'Session timeline in seconds' : 'Observation type, start offset and outcome');
  $('#ruler').classList.toggle('is-waterfall', state.view === 'waterfall');
  $('#theme-toggle').setAttribute('aria-pressed', state.theme === 'dark');
  $('#theme-toggle').setAttribute('aria-label', `Switch to ${state.theme === 'dark' ? 'light' : 'dark'} theme`);
  $('#theme-toggle').innerHTML = icon(state.theme === 'dark' ? 'sun' : 'moon');
  $('#collapse').setAttribute('aria-label', state.collapsed.size ? 'Expand all turns' : 'Collapse all turns');
  $('#collapse').innerHTML = icon(state.collapsed.size ? 'expand' : 'collapse');
  const hasTrace = state.preview === 'full';
  $('#total-duration').innerHTML = `${session.end.toFixed(1)}<span class="unit">s</span>`;
  $('#total-tokens').innerHTML = `${((totalInput + totalOutput) / 1000).toFixed(1)}<span class="unit">k</span>`;
  $('#token-breakdown').textContent = `${formatTokens(totalInput)} in / ${formatTokens(totalOutput)} out`;
  $('#generation-count').textContent = generations.length; $('#tool-count').textContent = tools.length;
  $('.ruler').hidden = !hasTrace; $('.trace-meta').hidden = !hasTrace;
  $('.legend').hidden = !hasTrace; $('.stats').hidden = !hasTrace;
  $('#span-count').textContent = hasTrace ? visibleNodes.length : '0';
  $('.title-group .pill').hidden = !hasTrace; $('#copy-id').disabled = !hasTrace;
  $('#trace-title').textContent = hasTrace ? 'Fix authentication flow' : 'Trace workspace';
  renderRows(); renderInspector(); updateUrl();
}
function reset() { state.query = ''; state.filter = null; state.collapsed.clear(); render(); }
function toggleCollapse(id) { state.collapsed.has(id) ? state.collapsed.delete(id) : state.collapsed.add(id); render(); }
function toast(message) { clearTimeout(toastTimer); $('#toast').textContent = message; $('#toast').hidden = false; toastTimer = setTimeout(() => $('#toast').hidden = true, 2800); }
async function copyText(value, success) {
  try { if (!navigator.clipboard?.writeText) throw new Error('Clipboard unavailable'); await navigator.clipboard.writeText(value); toast(success); }
  catch { toast('Clipboard unavailable in this browser context.'); }
}
$('#search').addEventListener('input', event => { state.query = event.target.value; render(); });
document.querySelectorAll('[data-filter]').forEach(button => button.onclick = () => { state.filter = state.filter === button.dataset.filter ? null : button.dataset.filter; render(); });
document.querySelectorAll('[data-view]').forEach(button => button.onclick = () => { state.view = button.dataset.view; render(); });
document.querySelectorAll('[data-page]').forEach(button => button.onclick = () => { state.page = button.dataset.page; render(); });
document.querySelectorAll('[data-select]').forEach(button => button.onclick = () => { reset(); selectNode(button.dataset.select); });
$('#reset').onclick = reset;
$('#collapse').onclick = () => { if (state.collapsed.size) state.collapsed.clear(); else ['turn-1', 'turn-2'].forEach(id => state.collapsed.add(id)); render(); };
$('#theme-toggle').onclick = () => { state.theme = state.theme === 'light' ? 'dark' : 'light'; render(); };
$('#help').onclick = () => $('#shortcuts').showModal();
$('#copy-link').onclick = () => copyText(location.href, 'View link copied with selection and filters.');
$('#copy-id').onclick = () => copyText(state.selected, 'Observation ID copied.');
$('#preview-state').onchange = event => {
  clearTimeout(loadingTimer); state.preview = event.target.value; state.page = 'trace'; render();
  if (state.preview === 'loading') loadingTimer = setTimeout(() => { state.preview = 'full'; $('#preview-state').value = 'full'; render(); }, 1800);
};
document.addEventListener('keydown', event => {
  if (event.target.matches('input, textarea, select') || $('#shortcuts').open) return;
  if (event.key === '/') { event.preventDefault(); $('#search').focus(); }
  if (event.target.matches('[data-node]') && ['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'].includes(event.key)) {
    event.preventDefault();
    const index = visibleNodes.findIndex(node => node.id === event.target.dataset.node), node = visibleNodes[index];
    if (event.key === 'ArrowUp' || event.key === 'ArrowDown') {
      const next = visibleNodes[index + (event.key === 'ArrowDown' ? 1 : -1)]; if (next) selectNode(next.id, true);
    } else if (['turn', 'session'].includes(node.type)) {
      if (event.key === 'ArrowLeft') state.collapsed.add(node.id); else state.collapsed.delete(node.id);
      render(); document.querySelector(`[data-node="${node.id}"]`)?.focus();
    }
  }
});
window.addEventListener('hashchange', () => {
  const next = new URLSearchParams(location.hash.slice(1));
  if (byId.has(next.get('node'))) state.selected = next.get('node');
  state.query = next.get('q') || ''; state.filter = ['error', 'tool', 'slow'].includes(next.get('filter')) ? next.get('filter') : null;
  state.view = next.get('view') === 'list' ? 'list' : 'waterfall'; state.tab = next.get('tab') === 'metadata' ? 'metadata' : 'io';
  state.theme = next.get('theme') === 'dark' ? 'dark' : 'light'; render();
});
console.assert(totalInput === 41320 && totalOutput === 3000, 'Synthetic usage totals must reconcile.');
console.assert(observations.every(node => node.end >= node.start && (!node.parent || byId.has(node.parent))), 'Synthetic intervals and parent links must be valid.');
render();
