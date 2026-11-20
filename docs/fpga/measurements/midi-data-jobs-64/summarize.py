#!/usr/bin/env python3
"""Summarize the six checked RTL runs without timing host-side preparation."""
import csv
import json
from pathlib import Path
import statistics

HERE = Path(__file__).resolve().parent
rows = []
summary = {}
for scenario in ('mixed', 'audio', 'midi'):
    for delay in (0, 7):
        name = f'{scenario}-{delay}'
        data = json.loads((HERE / f'{name}.json').read_text())
        assert data['loops'] == 64
        assert data['sequence_frames'] == 1440000
        assert data['quantum_frames'] == data['chunk_frames'] == 32
        assert data['ddr_added_delay_clocks'] == delay
        steady = [m for m in data['measurements'] if m['kind'] == 'steady']
        startup = [m for m in data['measurements'] if m['kind'] == 'playback_start']
        warmup = [m for m in data['measurements'] if m['kind'] == 'warmup']
        assert len(steady) == len(warmup) == 16 and len(startup) == 1
        values = [m['cycles'] for m in steady]
        summary[name] = {
            'median_us': statistics.median(values) / 100,
            'min_us': min(values) / 100,
            'max_us': max(values) / 100,
            'startup_us': startup[0]['cycles'] / 100,
            'median_cycles': statistics.median(values),
            'steady_samples': len(values),
            'steady_midi_packets': sorted({m['midi_packets'] for m in steady}),
            'compiled_instructions': data['compiled_instructions'],
            'maximum_jobs': data['maximum_jobs'],
            'actual_jobs': sorted({m['jobs'] for m in steady}),
        }
        for m in data['measurements']:
            rows.append(dict(scenario=scenario, delay=delay, **m))
(HERE / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
with (HERE / 'quanta.csv').open('w', newline='') as file:
    writer = csv.DictWriter(file, fieldnames=list(rows[0]), lineterminator='\n')
    writer.writeheader()
    writer.writerows(rows)
print(json.dumps(summary, indent=2))
