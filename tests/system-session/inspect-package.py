import hashlib
import json
import pathlib
import sys
import tarfile
import tempfile

log, archives, output = map(pathlib.Path, sys.argv[1:4])
text = log.read_text()
package = json.loads(text[text.index('[\n  {'):])[0]
paths = [entry['path'] for entry in package['files']]
for path in paths:
    assert not path.startswith(('.audit/', 'tests/', 'node_modules/', 'traces/', 'reports/', 'web/src/')), path
    assert '/tests/' not in path and '/fixtures/' not in path, path
    assert not path.endswith(('.jsonl', '.jsonl.zst', '.db', '.db-wal', '.db-shm', '.sqlite', '.env')), path
for required in ['package.json', 'native/linux-x64/pi-trace', 'scripts/lib/system-config.mjs', 'web/dist/index.html', 'THIRD_PARTY_NOTICES.md', 'web/dist/fonts/OFL-NunitoSans.txt']:
    assert required in paths, required
assert pathlib.Path(package['filename']).name == package['filename']
archive = (archives / package['filename']).resolve()
destination = pathlib.Path(tempfile.mkdtemp(prefix='pi-trace extracted package '))
with tarfile.open(archive) as source:
    source.extractall(destination, filter='data')
root = destination / 'package'
output.mkdir(parents=True, exist_ok=True)
(output / 'extracted-root.txt').write_text(str(root) + '\n')
report = {
    'archive': str(archive),
    'sha256': hashlib.sha256(archive.read_bytes()).hexdigest(),
    'size': package['size'],
    'unpackedSize': package['unpackedSize'],
    'entryCount': package['entryCount'],
    'paths': paths,
    'extracted': str(root),
    'runtimeHashes': {path: hashlib.sha256((root / path).read_bytes()).hexdigest() for path in paths if path.startswith(('scripts/', 'extensions/', 'native/', 'web/dist/'))},
}
(output / 'inspection.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps({key: value for key, value in report.items() if key not in ['paths', 'runtimeHashes']}, indent=2))
