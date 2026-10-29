import { spawnSync } from 'node:child_process'
import { readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = fileURLToPath(new URL('../', import.meta.url))
const mode = process.argv[2]
const number = '(?:0|[1-9][0-9]*)'
const identifier = '(?:0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)'
const versionPattern = new RegExp(`^${number}\\.${number}\\.${number}(?:-${identifier}(?:\\.${identifier})*)?$`)

function json(path) {
  return JSON.parse(readFileSync(join(root, path), 'utf8'))
}

function field(value, label) {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${label} must contain one version string.`)
  }
  return { label, version: value }
}

function tomlTable(text, name) {
  const lines = text.split(/(?<=\n)/)
  const header = `[${name}]`
  const starts = lines.flatMap((line, index) => line.trim().replace(/\s+#.*$/, '') === header ? [index] : [])
  if (starts.length !== 1) throw new Error(`Cargo.toml must contain one ${header} table.`)
  const start = starts[0] + 1
  const following = lines.findIndex((line, index) => index >= start && /^\s*\[/.test(line))
  const end = following === -1 ? lines.length : following
  return { lines, start, end }
}

function tomlString(lines, start, end, key, label) {
  const matches = []
  for (let index = start; index < end; index += 1) {
    if (new RegExp(`^\\s*${key}\\s*=`).test(lines[index])) matches.push(index)
  }
  if (matches.length !== 1) throw new Error(`${label} must contain one ${key} field.`)
  const index = matches[0]
  const match = lines[index].match(new RegExp(`^(\\s*${key}\\s*=\\s*")([^"\\r\\n]+)("\\s*(?:#.*)?(?:\\r?\\n)?)$`))
  if (!match) throw new Error(`${label}.${key} must use a double-quoted string on one line.`)
  return { index, prefix: match[1], value: match[2], suffix: match[3] }
}

function cargoVersion() {
  const text = readFileSync(join(root, 'Cargo.toml'), 'utf8')
  const table = tomlTable(text, 'workspace.package')
  const entry = tomlString(table.lines, table.start, table.end, 'version', 'Cargo.toml [workspace.package]')
  return { ...table, ...entry }
}

function cargoLockVersion() {
  const text = readFileSync(join(root, 'Cargo.lock'), 'utf8')
  const packages = text.split(/^\[\[package\]\]\s*$/m).slice(1)
  const local = packages.filter(block => /^name\s*=\s*"pi-trace"\s*$/m.test(block) && !/^source\s*=/m.test(block))
  if (local.length !== 1) throw new Error('Cargo.lock must contain one local pi-trace package.')
  const lines = local[0].split(/(?<=\n)/)
  return tomlString(lines, 0, lines.length, 'version', 'Cargo.lock pi-trace').value
}

function readState() {
  const manifest = json('package.json')
  const canonical = field(manifest.version, 'package.json').version
  if (!versionPattern.test(canonical)) {
    throw new Error(`Unsupported version ${JSON.stringify(canonical)}. Use canonical MAJOR.MINOR.PATCH with an optional prerelease. Build metadata is not supported.`)
  }
  const crate = readFileSync(join(root, 'crates/pi-trace/Cargo.toml'), 'utf8')
  const crateTable = tomlTable(crate, 'package')
  const crateBody = crateTable.lines.slice(crateTable.start, crateTable.end).join('')
  if (!/^version\.workspace\s*=\s*true\s*(?:#.*)?$/m.test(crateBody)) {
    throw new Error('crates/pi-trace/Cargo.toml must inherit version.workspace = true.')
  }
  const cargo = cargoVersion()
  const entries = [field(canonical, 'package.json'), field(cargo.value, 'Cargo.toml [workspace.package]'), field(cargoLockVersion(), 'Cargo.lock pi-trace')]
  for (const directory of ['', 'web/']) {
    const lock = json(`${directory}package-lock.json`)
    if (lock.lockfileVersion !== 3) throw new Error(`${directory}package-lock.json must use lockfileVersion 3.`)
    entries.push(field(lock.version, `${directory}package-lock.json`))
    entries.push(field(lock.packages?.['']?.version, `${directory}package-lock.json packages[""]`))
    if (directory) entries.push(field(json(`${directory}package.json`).version, `${directory}package.json`))
  }
  return { canonical, entries, cargo }
}

function check(state) {
  const drift = state.entries.filter(entry => entry.version !== state.canonical)
  if (drift.length) {
    throw new Error(`Version drift from package.json ${state.canonical}:\n${drift.map(entry => `  ${entry.label}: ${entry.version}`).join('\n')}\nRun npm run version:sync to repair derived versions.`)
  }
}

function run(command, args, cwd = root) {
  const result = spawnSync(command, args, { cwd, stdio: 'inherit', shell: false })
  if (result.error) throw result.error
  if (result.status !== 0) throw new Error(`${command} ${args.join(' ')} failed with status ${result.status}.`)
}

function npm(args, cwd) {
  if (process.env.npm_execpath) run(process.execPath, [process.env.npm_execpath, ...args], cwd)
  else run('npm', args, cwd)
}

function sync(state) {
  if (state.entries.every(entry => entry.version === state.canonical)) return
  const cargo = state.cargo
  cargo.lines[cargo.index] = `${cargo.prefix}${state.canonical}${cargo.suffix}`
  writeFileSync(join(root, 'Cargo.toml'), cargo.lines.join(''))
  npm(['version', state.canonical, '--no-git-tag-version', '--ignore-scripts', '--allow-same-version'], join(root, 'web'))
  npm(['install', '--package-lock-only', '--ignore-scripts', '--offline', '--no-audit', '--no-fund', '--workspaces=false'])
  run('cargo', ['update', '--workspace', '--offline'])
  check(readState())
}

try {
  if (process.argv.length !== 3 || !['--check', '--sync'].includes(mode)) {
    throw new Error('Usage: node scripts/version.mjs --check|--sync')
  }
  const state = readState()
  if (mode === '--sync') sync(state)
  else check(state)
  console.log(`Version ${state.canonical} is synchronized.`)
} catch (error) {
  console.error(error.message)
  if (mode === '--sync') {
    console.error('No commit, tag, push, or publication was performed. A failed synchronization may leave version files changed. Fix the reported error and rerun npm run version:sync.')
  }
  process.exitCode = 1
}
