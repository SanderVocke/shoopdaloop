#!/usr/bin/env python3
"""Compare production callbacks with an independently built pre-feature harness."""
import argparse
import gzip
import json
import pathlib
import random
import statistics
import subprocess

parser = argparse.ArgumentParser(__doc__)
parser.add_argument('--baseline', type=pathlib.Path, required=True)
parser.add_argument('--current', type=pathlib.Path, default=pathlib.Path('target/release/examples/profiling_benchmark'))
parser.add_argument('--output', type=pathlib.Path, default=pathlib.Path('artifacts/runtime-profiling'))
parser.add_argument('--callbacks', type=int, default=128)
parser.add_argument('--repetitions', type=int, default=3)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
cases = [(n, q, 'steady') for n in [2, 20, 100] for q in [1, 16, 32, 64, 128]]
cases += [(n, q, scenario) for n, q in [(2, 1), (20, 32), (100, 128)] for scenario in ['midi', 'recording', 'edits', 'partitions']]
jobs = [(n, q, scenario, mode, repetition) for repetition in range(args.repetitions)
        for n, q, scenario in cases
        for mode in (['baseline', 'off', 'on', 'stalled'] if scenario == 'steady' else ['baseline', 'off', 'on'])]
random.Random(4321).shuffle(jobs)
rows = []
with gzip.open(args.output / 'callbacks.jsonl.gz', 'wt') as raw:
    for index, (n, q, scenario, mode, repetition) in enumerate(jobs):
        binary = args.baseline if mode == 'baseline' else args.current
        result = subprocess.run([str(binary.resolve()), str(n), str(q), str(args.callbacks),
                                 'off' if mode == 'baseline' else mode, scenario],
                                check=True, text=True, capture_output=True, timeout=120)
        row = json.loads(result.stdout)
        row.update(mode=mode, repetition=repetition)
        rows.append(row)
        raw.write(json.dumps(row, separators=(',', ':')) + '\n')
        raw.flush()
        if index % 12 == 0:
            print(f'{index + 1}/{len(jobs)} cases', flush=True)

def quantile(values, fraction):
    ordered = sorted(values)
    return ordered[min(len(ordered) - 1, int(fraction * len(ordered)))]

summary = []
for n, q, scenario in cases:
    baseline = [r for r in rows if (r['strips'], r['quantum'], r['scenario'], r['mode']) == (n, q, scenario, 'baseline')]
    base = statistics.median(statistics.median(r['callback_ns']) for r in baseline)
    for mode in ['baseline', 'off', 'on', 'stalled']:
        runs = [r for r in rows if (r['strips'], r['quantum'], r['scenario'], r['mode']) == (n, q, scenario, mode)]
        if not runs:
            continue
        times = [t for r in runs for t in r['callback_ns']]
        median = statistics.median(statistics.median(r['callback_ns']) for r in runs)
        budget = statistics.median([b for r in runs for b in r['budget_ns']])
        audio_ns = sum(r['frames'] for r in runs) * 1e9 / 48000
        summary.append(dict(strips=n, nodes=runs[0]['nodes'], quantum=q, scenario=scenario, mode=mode,
                            median_ns=median, p99_ns=quantile(times, .99), max_ns=max(times),
                            misses=sum(r['misses'] for r in runs), callbacks=len(times),
                            delta_budget_pp=100 * (median - base) / budget,
                            relative_percent=100 * (median / base - 1),
                            collection_budget_percent=100 * sum(r['control_collection_ns'] for r in runs) / audio_ns,
                            bytes_per_audio_second=sum(r['wire_bytes'] for r in runs) * 1e9 / audio_ns,
                            dropped_batches=sum(r['dropped_batches'] for r in runs),
                            high_water_pages=max(r['high_water_pages'] for r in runs),
                            sample_storage_bytes=max(r['sample_storage_bytes'] for r in runs),
                            captures=sum(r['capture_complete'] for r in runs)))
(args.output / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
lines = ['| Nodes | Q | Mode | Median µs | P99 µs | Max µs | Misses | Δ budget pp | Δ DSP % | Collector % budget | MB/s | Drops |',
         '| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |']
for r in summary:
    if r['scenario'] == 'steady':
        lines.append(f"| {r['nodes']} | {r['quantum']} | {r['mode']} | {r['median_ns']/1000:.2f} | {r['p99_ns']/1000:.2f} | {r['max_ns']/1000:.2f} | {r['misses']}/{r['callbacks']} | {r['delta_budget_pp']:+.3f} | {r['relative_percent']:+.2f} | {r['collection_budget_percent']:.3f} | {r['bytes_per_audio_second']/1e6:.3f} | {r['dropped_batches']} |")
(args.output / 'table.md').write_text('\n'.join(lines) + '\n')
print(f'Completed {len(rows)} runs; raw durations and summaries: {args.output}', flush=True)
