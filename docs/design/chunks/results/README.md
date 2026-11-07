# Replaying the native results

See [the report](../RESULTS.md), [launch guide](../PROTOTYPE.md) and
[manifest](manifest.json). The archive contains 147 runs, including complete
callback files with warmup, and configuration/state/media checks. Summaries select
exactly 384 consecutive callbacks per run; slow callbacks are retained.

From the repository root:

```sh
mkdir -p work/chunk-replay
tar -xzf docs/design/chunks/results/native-2026-11-07.tar.gz -C work/chunk-replay
python3 - <<'PY'
import hashlib, json, runpy
from pathlib import Path
root = Path('work/chunk-replay')
manifest = json.loads(Path('docs/design/chunks/results/manifest.json').read_text())
for name, expected in manifest['file_sha256'].items():
    assert hashlib.sha256((root / name).read_bytes()).hexdigest() == expected, name
summarize = runpy.run_path('scripts/benchmark_audio_chunks.py')['summarize']
for callbacks in root.rglob('callbacks.csv'):
    summarize(callbacks.parent)
print('Verified checksums, callback windows, mode identity, state and media for 147 runs')
PY
```

Re-summarizing changes the summary's local `directory` field to the extracted
path. Verify original checksums before re-summarizing. The four standalone
`*-summary.csv` files are also provided for comparison without unpacking.

The manifest's `source_sha` identifies the benchmarked Rust code. Subsequent
report/plan commits do not change that implementation. Connection profiles,
driver preferences, temporary authority material and host logs are excluded.
The archived machine commands record the original target paths; use the guide's
build command and your local target paths for new runs.
