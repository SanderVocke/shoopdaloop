# Replaying benchmark evidence

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
    summarize(callbacks.parent, write=False)
print('Verified checksums, callback windows, mode identity, state and media for 147 runs')
PY
```

The read-only replay preserves the original summaries and their recorded
`directory` paths. The four standalone
`*-summary.csv` files are also provided for comparison without unpacking.

The manifest's `source_sha` identifies the benchmarked Rust code. Subsequent
report/plan commits do not change that implementation. Connection profiles,
driver preferences, temporary authority material and host logs are excluded.
The archived machine commands record the original target paths; use the guide's
build command and your local target paths for new runs.


## Pi 4 hardware dataset

See [the Pi report](../PI_RESULTS.md) and
[manifest](pi4-2026-11-08/manifest.json). The recompressed archive retains all
1,621 original ZIP members byte for byte, including warmup callbacks. The
manifest records the original release-asset checksum, each file checksum and
derived-artifact checksums. Six matrix CSV copies (LF line endings) and a grouped CSV are
available without unpacking.

```sh
mkdir -p work/pi4-replay
tar -xJf docs/design/chunks/results/pi4-2026-11-08/evidence.tar.xz -C work/pi4-replay
python3 scripts/analyze_audio_chunk_results.py --input work/pi4-replay \
  --manifest docs/design/chunks/results/pi4-2026-11-08/manifest.json \
  --output work/pi4-replayed --diagnostics
```

The analyzer verifies app/media/callback checks, statistics against both stored
summary levels, build identity and per-run render scheduling, without rewriting
the source evidence. Optional `--plot work/pi4-replayed/budget-p99.svg` requires
matplotlib; the remaining replay uses only Python's standard library.
