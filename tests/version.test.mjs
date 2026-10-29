import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { copyFileSync, mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'

const script = new URL('../scripts/version.mjs', import.meta.url)
const ownedFiles = ['package.json', 'package-lock.json', 'web/package.json', 'web/package-lock.json', 'Cargo.toml', 'Cargo.lock']

function command(root, command, args, success = true) {
  const result = spawnSync(command, args, { cwd: root, encoding: 'utf8', env: { ...process.env, npm_config_cache: join(root, '.npm-cache') } })
  if (result.error) throw result.error
  if (success) assert.equal(result.status, 0, `${command} ${args.join(' ')}\n${result.stdout}\n${result.stderr}`)
  return result
}

function writeJson(path, value) {
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`)
}

function readJson(root, path) {
  return JSON.parse(readFileSync(join(root, path), 'utf8'))
}

function snapshot(root) {
  return Object.fromEntries(ownedFiles.map(path => [path, readFileSync(join(root, path), 'utf8')]))
}

function versions(root) {
  const cargo = readFileSync(join(root, 'Cargo.toml'), 'utf8').match(/^version = "([^"]+)"$/m)[1]
  const lock = readFileSync(join(root, 'Cargo.lock'), 'utf8').match(/name = "pi-trace"\nversion = "([^"]+)"/)[1]
  return [readJson(root, 'package.json').version, readJson(root, 'package-lock.json').version, readJson(root, 'package-lock.json').packages[''].version, readJson(root, 'web/package.json').version, readJson(root, 'web/package-lock.json').version, readJson(root, 'web/package-lock.json').packages[''].version, cargo, lock]
}

function dependencies(root) {
  const npm = Object.fromEntries(['package-lock.json', 'web/package-lock.json'].map(path => [path, Object.fromEntries(Object.entries(readJson(root, path).packages).filter(([key]) => key).map(([key, value]) => [key, value.version]))]))
  const cargo = readFileSync(join(root, 'Cargo.lock'), 'utf8').split('[[package]]').filter(block => /name = "fixture-support"/.test(block))
  return { npm, cargo }
}

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'pi-trace-version-'))
  t.after(() => rmSync(root, { recursive: true, force: true }))
  for (const directory of ['scripts', 'web', 'vendor/npm-fixture', 'vendor/rust-fixture/src', 'crates/pi-trace/src']) mkdirSync(join(root, directory), { recursive: true })
  copyFileSync(script, join(root, 'scripts/version.mjs'))
  writeJson(join(root, 'package.json'), { name: 'pi-trace-version-fixture', private: true, version: '0.1.0', scripts: { preversion: 'node scripts/version.mjs --check', version: 'node scripts/version.mjs --sync' }, dependencies: { 'npm-fixture': 'file:vendor/npm-fixture' }, peerDependencies: { '@pi-trace/host-fixture': '*' }, peerDependenciesMeta: { '@pi-trace/host-fixture': { optional: true } } })
  writeJson(join(root, 'web/package.json'), { name: 'pi-trace-web-fixture', private: true, version: '0.1.0', dependencies: { 'npm-fixture': 'file:../vendor/npm-fixture' } })
  writeJson(join(root, 'vendor/npm-fixture/package.json'), { name: 'npm-fixture', version: '2.3.4', private: true })
  writeFileSync(join(root, 'Cargo.toml'), '[workspace]\nmembers = ["crates/pi-trace"]\nexclude = ["vendor/rust-fixture"]\nresolver = "3"\n\n[workspace.package]\nversion = "0.1.0"\nedition = "2024"\n')
  writeFileSync(join(root, 'crates/pi-trace/Cargo.toml'), '[package]\nname = "pi-trace"\nversion.workspace = true\nedition.workspace = true\n\n[dependencies]\nfixture-support = { path = "../../vendor/rust-fixture" }\n')
  writeFileSync(join(root, 'crates/pi-trace/src/main.rs'), 'fn main() {}\n')
  writeFileSync(join(root, 'vendor/rust-fixture/Cargo.toml'), '[package]\nname = "fixture-support"\nversion = "2.3.4"\nedition = "2024"\n')
  writeFileSync(join(root, 'vendor/rust-fixture/src/lib.rs'), '')
  for (const directory of [root, join(root, 'web')]) command(directory, 'npm', ['install', '--package-lock-only', '--ignore-scripts', '--offline', '--no-audit', '--no-fund'])
  command(root, 'cargo', ['generate-lockfile', '--offline'])
  return root
}

test('npm stable bump synchronizes all versions and preserves locked dependencies', t => {
  const root = fixture(t)
  const before = dependencies(root)
  command(root, 'npm', ['version', 'patch', '--no-git-tag-version'])
  assert.deepEqual(versions(root), Array(8).fill('0.1.1'))
  assert.deepEqual(dependencies(root), before)
  command(root, 'node', ['scripts/version.mjs', '--check'])
  const synced = snapshot(root)
  command(root, 'node', ['scripts/version.mjs', '--sync'])
  assert.deepEqual(snapshot(root), synced)
})

test('npm prerelease increments and stable promotion preserve canonical Cargo versions', t => {
  const root = fixture(t)
  const before = dependencies(root)
  for (const [args, expected] of [[['prepatch', '--preid', 'alpha'], '0.1.1-alpha.0'], [['prerelease'], '0.1.1-alpha.1'], [['patch'], '0.1.1']]) {
    command(root, 'npm', ['version', ...args, '--no-git-tag-version'])
    assert.deepEqual(versions(root), Array(8).fill(expected))
    assert.deepEqual(dependencies(root), before)
  }
})

test('drift check is read-only and explicit synchronization repairs every derived version', t => {
  const root = fixture(t)
  const manifest = readJson(root, 'package.json')
  manifest.version = '0.2.0-rc.2'
  writeJson(join(root, 'package.json'), manifest)
  const drifted = snapshot(root)
  const checked = command(root, 'node', ['scripts/version.mjs', '--check'], false)
  assert.equal(checked.status, 1)
  assert.match(checked.stderr, /Version drift from package.json 0\.2\.0-rc\.2/)
  assert.deepEqual(snapshot(root), drifted)
  command(root, 'node', ['scripts/version.mjs', '--sync'])
  assert.deepEqual(versions(root), Array(8).fill(manifest.version))
})

test('noncanonical versions fail before changing derived files', t => {
  const root = fixture(t)
  for (const version of ['1.2', 'v1.2.3', '01.2.3', '1.2.3-alpha.01', '1.2.3+build.4', '1.2.3-']) {
    const manifest = readJson(root, 'package.json')
    manifest.version = version
    writeJson(join(root, 'package.json'), manifest)
    const before = snapshot(root)
    const result = command(root, 'node', ['scripts/version.mjs', '--sync'], false)
    assert.equal(result.status, 1, version)
    assert.match(result.stderr, /Unsupported version/)
    assert.deepEqual(snapshot(root), before)
  }
})

test('a missing version field fails before synchronization writes', t => {
  const root = fixture(t)
  const lock = readJson(root, 'web/package-lock.json')
  delete lock.packages[''].version
  writeJson(join(root, 'web/package-lock.json'), lock)
  const before = snapshot(root)
  const result = command(root, 'node', ['scripts/version.mjs', '--sync'], false)
  assert.equal(result.status, 1)
  assert.match(result.stderr, /web\/package-lock.json packages\[""\] must contain one version string/)
  assert.deepEqual(snapshot(root), before)
})
