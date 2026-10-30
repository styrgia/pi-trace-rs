import assert from 'node:assert/strict'
import test from 'node:test'
import { setTimeout as delay } from 'node:timers/promises'
import { WorkspacePoller } from '../web/src/lib/workspace-poller.ts'

async function until(predicate) {
  const deadline = Date.now() + 2000
  while (!predicate()) { assert.ok(Date.now() < deadline, 'poller condition did not settle'); await delay(1) }
}

test('poller has one active status request and rejects stale replies during visibility resync', async (t) => {
  const requests = [], values = [], states = []
  let active = 0, maximum = 0
  const poller = new WorkspacePoller({ intervalMs: 10, timeoutMs: 1000,
    request: (signal) => new Promise((resolve) => {
      active++; maximum = Math.max(maximum, active)
      let ended = false
      const finish = () => { if (!ended) { active--; ended = true } }
      signal.addEventListener('abort', finish, { once: true })
      requests.push({ signal, resolve: (value) => { finish(); resolve(value) } })
    }), onworkspace: (value) => values.push(value), onconnection: (state) => states.push(state),
  })
  t.after(() => poller.stop())
  poller.start(); await until(() => requests.length === 1)
  poller.resync(); await until(() => requests.length === 2)
  assert.equal(requests[0].signal.aborted, true)
  requests[0].resolve({ state: 'empty' })
  requests[1].resolve({ state: 'ready', attemptId: 'a1', snapshotId: 'current' })
  await until(() => values.length === 1)
  assert.equal(values[0].snapshotId, 'current')
  poller.setVisible(false)
  const count = requests.length
  await delay(30)
  assert.equal(requests.length, count)
  assert.equal(states.at(-1).kind, 'suspended')
  poller.setVisible(true); await until(() => requests.length === count + 1)
  assert.equal(maximum, 1)
  poller.stop()
  assert.equal(requests.at(-1).signal.aborted, true)
})

test('timeouts abort requests, back off and permit explicit reconnect without stale publication', async (t) => {
  const requests = [], states = [], values = []
  const poller = new WorkspacePoller({ intervalMs: 10, timeoutMs: 10, maxBackoffMs: 40,
    request: (signal) => new Promise((resolve) => requests.push({ signal, resolve })),
    onworkspace: (value) => values.push(value), onconnection: (state) => states.push(state),
  })
  t.after(() => poller.stop())
  poller.start(); await until(() => states.some((state) => state.kind === 'reconnecting'))
  assert.equal(requests[0].signal.aborted, true)
  assert.equal(states[0].retryInMs, 20)
  await until(() => requests.length === 2)
  requests[0].resolve({ state: 'empty' })
  requests[1].resolve({ state: 'ready', attemptId: 'a1', snapshotId: 's1' })
  await until(() => values.length === 1)
  assert.equal(values[0].snapshotId, 's1')
  poller.resync(); await until(() => requests.length === 3)
  requests[2].resolve({ state: 'ready', attemptId: 'a1', snapshotId: 's1' })
  await until(() => values.length === 2)
  assert.equal(states.at(-1).kind, 'connected')
})
