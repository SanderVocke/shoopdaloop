#!/usr/bin/env python3
"""Verify and replay local four-renderer archives without changing the evidence."""
import argparse
import csv
import hashlib
import json
from pathlib import Path
import statistics
import tempfile
import zipfile

from analyze_audio_chunk_results import equivalent
from benchmark_audio_chunks import summarize


def digest(data):
    return hashlib.sha256(data).hexdigest()


def replay(manifest_path, datasets=None):
    manifest = json.loads(manifest_path.read_text())
    runs = []
    totals = {}
    for name, entry in manifest.items():
        if datasets and name not in datasets:
            continue
        archive = manifest_path.parent / entry['archive']
        assert archive.stat().st_size == entry['bytes'], archive
        assert digest(archive.read_bytes()) == entry['sha256'], archive
        with tempfile.TemporaryDirectory(prefix='shoop-fourth-') as temporary:
            root = Path(temporary)
            with zipfile.ZipFile(archive) as source:
                assert set(source.namelist()) == set(entry['members']), archive
                for member in source.namelist():
                    path = Path(member)
                    assert not path.is_absolute() and '..' not in path.parts, member
                    assert digest(source.read(member)) == entry['members'][member], member
                source.extractall(root)
            matrix = next(root.glob('*/machine.json')).parent
            machine = json.loads((matrix / 'machine.json').read_text())
            assert machine == entry['machine'], name
            rows = json.loads((matrix / 'results.json').read_text())
            assert len(rows) == entry['runs']
            assert len(list(matrix.rglob('callbacks.csv'))) == len(rows)
            selected = []
            for saved in rows:
                directory = matrix / saved['interference'] / f"{saved['scenario']}-{saved['input']}" / f"{saved['repeat']}-{saved['mode']}"
                actual = summarize(directory, write=False)
                individual = json.loads((directory / 'summary.json').read_text())
                for key, value in actual.items():
                    if key != 'directory':
                        assert equivalent(value, saved[key]), (name, directory, key)
                        assert equivalent(value, individual[key]), (name, directory, key)
                setup = json.loads((directory / 'run.json').read_text())
                scheduling = json.loads((directory / 'scheduling.json').read_text())
                assert scheduling['render_affinity'] == [machine['render_cpu']]
                assert scheduling['render_scheduler'] == machine['scheduler'] == 0
                memory = {line.split()[0].rstrip(':'): int(line.split()[1]) for line in setup['process_memory']}
                selected.append({**actual, 'dataset': name, 'repeat': saved['repeat'],
                                 'interference': saved['interference'], 'buffer': setup['buffer'],
                                 'host_rss_mib': memory['VmRSS'] / 1024,
                                 'directory': str(directory.relative_to(root))})
            totals[name] = {'runs': len(selected),
                            'selected_callbacks': sum(r['callbacks'] for r in selected),
                            'overruns': sum(r['overruns'] for r in selected),
                            'drops': sum(r['dropped'] for r in selected)}
            assert totals[name] == {key: entry[key] for key in totals[name]}, name
            runs.extend(selected)
            print(f"{name}: verified {len(entry['members'])} hashes and {len(selected)} runs", flush=True)
    assert runs, 'no selected archives'
    groups = []
    for key in sorted({(r['dataset'], r['interference'], r['scenario'], r['input'], r['mode']) for r in runs}):
        chosen = [r for r in runs if (r['dataset'], r['interference'], r['scenario'], r['input'], r['mode']) == key]
        groups.append({**dict(zip(('dataset', 'interference', 'scenario', 'input', 'mode'), key)),
                       'runs': len(chosen), 'callbacks': sum(r['callbacks'] for r in chosen),
                       **{field: statistics.median(r[field] for r in chosen) for field in
                          ('budget_us', 'p50_us', 'p95_us', 'p99_us', 'late_p99_us', 'host_rss_mib')},
                       'max_us': max(r['max_us'] for r in chosen),
                       'late_max_us': max(r['late_max_us'] for r in chosen),
                       'overruns': sum(r['overruns'] for r in chosen),
                       'resource_copy_bytes': statistics.median(r['counters']['resource_copy_bytes']['mean'] for r in chosen),
                       'payload_live_max': max(r['counters']['payload_live']['max'] for r in chosen),
                       'index_live_max': max(r['counters']['index_live']['max'] for r in chosen)})
    return runs, groups, totals


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--datasets', nargs='+')
    args = parser.parse_args()
    runs, groups, totals = replay(args.manifest, args.datasets)
    args.output.mkdir(parents=True, exist_ok=True)
    for name, value in [('runs', runs), ('groups', groups), ('validation', totals)]:
        (args.output / f'{name}.json').write_text(json.dumps(value, indent=2) + '\n')
    with (args.output / 'groups.csv').open('w') as target:
        writer = csv.DictWriter(target, list(groups[0]), lineterminator='\n')
        writer.writeheader()
        writer.writerows(groups)


if __name__ == '__main__':
    main()
