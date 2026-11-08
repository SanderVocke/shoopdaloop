#!/usr/bin/env python3
"""Replay archived app/callback checks and summarize audio renderer comparisons."""
import argparse
import csv
import hashlib
import json
import math
from pathlib import Path
import statistics

from benchmark_audio_chunks import MODES, summarize


def equivalent(actual, expected):
    if isinstance(actual, dict):
        return actual.keys() == expected.keys() and all(equivalent(value, expected[key]) for key, value in actual.items())
    if isinstance(actual, float) or isinstance(expected, float):
        return math.isclose(actual, expected, rel_tol=1e-12, abs_tol=1e-9)
    return actual == expected


def timing_diagnostics(root, runs):
    output = {'shared_loop_cycles': [], 'long_stalls': []}
    for result in runs:
        shared = result['matrix'] == 'looping' and result['scenario'] == 'shared'
        loaded = result['buffer'] == 32 and result['interference'] == 'memory' and result['scenario'] in ('play', 'ramps')
        if not shared and not loaded:
            continue
        directory = root / result['directory']
        setup = json.loads((directory / 'run.json').read_text())
        with (directory / 'callbacks.csv').open() as source:
            rows = [{k: int(v) for k, v in row.items()} for row in
                    csv.DictReader(line for line in source if not line.startswith('#'))
                    if setup['first_frame'] <= int(row['frame']) < setup['last_frame']]
        identity = {key: result[key] for key in ('directory', 'mode', 'repeat', 'scenario')}
        if shared:
            track = next(t for t in setup['setup']['tracks'] if t['name'] == 'Benchmark')
            lengths = sorted({loop['length_frames'] for loop in track['loops'][:setup['voices']]})
            peaks = [r for r in rows if r['elapsed_ns'] > result['budget_us'] * 1000]
            gaps = [peaks[i]['frame'] - peaks[i-1]['frame'] for i in range(1, len(peaks))]
            period = lengths[0] if len(lengths) == 1 else None
            loop_position = track['loops'][0]['position']
            snapshot_frame = setup['setup']['status']['processed_frames']
            positions = [((r['frame'] - snapshot_frame) + loop_position * period) % period
                         for r in peaks] if period else []
            output['shared_loop_cycles'].append({
                **identity, 'loop_lengths_frames': lengths,
                'peak_frames': [r['frame'] for r in peaks],
                'peak_elapsed_us': [r['elapsed_ns'] / 1000 for r in peaks],
                'peak_frame_gaps': gaps, 'exact_loop_gaps': sum(gap == period for gap in gaps),
                'peak_loop_positions_from_snapshot_frames': positions,
                'median_resource_copy_bytes': statistics.median(r['resource_copy_bytes'] for r in rows),
                'peak_resource_copy_bytes': [r['resource_copy_bytes'] for r in peaks],
            })
        if loaded:
            period = result['buffer'] / 48000 * 1e9
            times = [0.0]
            for i, row in enumerate(rows[:-1]):
                reset = row['elapsed_ns'] + row['late_ns'] > 2 * period
                interval = (row['elapsed_ns'] + rows[i+1]['late_ns'] if reset else
                            period + rows[i+1]['late_ns'] - row['late_ns'])
                times.append(times[-1] + interval)
            peaks = [i for i, row in enumerate(rows) if row['elapsed_ns'] >= 20_000_000]
            gaps = [(times[peaks[i]] - times[peaks[i-1]]) / 1e9 for i in range(1, len(peaks))]
            output['long_stalls'].append({
                **identity, 'threshold_ms': 20, 'count': len(peaks),
                'elapsed_ms': [rows[i]['elapsed_ns'] / 1e6 for i in peaks],
                'audio_frame_gaps': [rows[peaks[i]]['frame'] - rows[peaks[i-1]]['frame'] for i in range(1, len(peaks))],
                'approximate_wall_gap_seconds': gaps,
                'median_approximate_wall_gap_seconds': statistics.median(gaps) if gaps else None,
            })
    output['wall_reconstruction_limits'] = ('Approximate dummy-driver deadline increments/resets from elapsed and late columns; '
                                             'excludes post-timer recording/bookkeeping and can misclassify resets near their threshold. '
                                             'No scheduler trace or RT bandwidth configuration was captured.')
    return output


def plot_groups(groups, destination):
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt

    colors = ('#444e86', '#e18a32', '#329b84')
    cases = [('none', 'idle'), ('none', 'play'), ('none', 'ramps'),
             ('memory', 'idle'), ('memory', 'play'), ('memory', 'ramps')]
    with plt.rc_context({'svg.hashsalt': 'shoop-pi4-2026-11-08', 'font.size': 10}):
        fig, axes = plt.subplots(1, 2, figsize=(12, 4.3), sharey=True)
        for ax, buffer in zip(axes, (256, 32)):
            ax.axhspan(0, 1, color='#f2f7f3', zorder=0)
            ax.axhline(1, color='#b63d42', linestyle='--', linewidth=1.4, label='Callback budget')
            for offset, (mode, color) in enumerate(zip(MODES, colors)):
                values = []
                for load, scenario in cases:
                    matrix = f'timing-{buffer}' if load == 'none' else f'interference-{buffer}'
                    group = next(g for g in groups if (g['matrix'], g['interference'], g['scenario'], g['mode']) ==
                                 (matrix, load, scenario, mode))
                    values.append(group['p99_us'] / group['budget_us'])
                ax.bar([i + (offset-1) * .24 for i in range(len(cases))], values, width=.23,
                       color=color, label=mode, zorder=2)
            ax.set_xticks(range(len(cases)), [f'{scenario}\n' + ('quiet' if load == 'none' else 'memory\nload')
                                             for load, scenario in cases])
            ax.set_title(f'{buffer} frames ({buffer / 48:.3f} ms budget)')
            ax.grid(axis='y', alpha=.2)
            ax.set_ylim(0, 1.7)
        axes[0].set_ylabel('Median run p99 / callback budget')
        handles, labels = axes[0].get_legend_handles_labels()
        fig.legend(handles, labels, loc='upper center', ncols=4, frameon=False)
        fig.tight_layout(rect=(0, 0, 1, .88))
        fig.savefig(destination, metadata={'Date': None} if destination.suffix == '.svg' else {})
        if destination.suffix == '.svg':
            destination.write_text('\n'.join(line.rstrip() for line in destination.read_text().splitlines()) + '\n')
        plt.close(fig)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', required=True, type=Path, help='Unpacked evidence root containing results/ and BUILD_INFO.json')
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--reference-build-info', type=Path)
    parser.add_argument('--manifest', type=Path, help='Verify all unpacked member hashes against a published result manifest')
    parser.add_argument('--diagnostics', action='store_true')
    parser.add_argument('--plot', type=Path, help='Optional SVG chart; requires matplotlib')
    args = parser.parse_args()
    root = args.input.resolve()
    verified_members = None
    if args.manifest:
        expected = json.loads(args.manifest.read_text())['file_sha256']
        for name, digest in expected.items():
            relative = Path(name)
            assert not relative.is_absolute() and '..' not in relative.parts
            path = root / relative
            assert path.is_file() and not path.is_symlink()
            assert hashlib.sha256(path.read_bytes()).hexdigest() == digest, name
        actual = {str(p.relative_to(root)) for p in root.rglob('*') if p.is_file()}
        assert actual == set(expected), 'unindexed or missing input members'
        verified_members = len(expected)
        print(f'{verified_members} input member hashes verified', flush=True)
    info_path = root / 'BUILD_INFO.json'
    build = json.loads(info_path.read_text())
    manifest_sha = hashlib.sha256(info_path.read_bytes()).hexdigest()
    if args.reference_build_info:
        assert info_path.read_bytes() == args.reference_build_info.read_bytes(), 'build manifest differs from supplied bundle'
    args.output.mkdir(parents=True, exist_ok=True)
    matrices = {}
    runs = []
    for matrix in sorted((root / 'results').iterdir()):
        if not (matrix / 'results.json').exists():
            continue
        machine = json.loads((matrix / 'machine.json').read_text())
        assert machine['bundle_manifest_sha256'] == manifest_sha
        assert machine['git_sha'] == build['binary_source_sha']
        assert machine['bundle_helper_source_sha'] == build['helper_source_sha']
        assert machine['host_sha256'] == build['files_sha256']['bin/shoop_host']
        assert machine['launcher_sha256'] == build['files_sha256']['bin/chunk_benchmark']
        rows = json.loads((matrix / 'results.json').read_text())
        for saved in rows:
            directory = matrix / saved['interference'] / f"{saved['scenario']}-{saved['input']}" / f"{saved['repeat']}-{saved['mode']}"
            replay = summarize(directory, write=False)
            individual = json.loads((directory / 'summary.json').read_text())
            for key, value in replay.items():
                if key != 'directory':
                    assert equivalent(value, saved[key]), (directory, key, 'matrix summary mismatch')
                    assert equivalent(value, individual[key]), (directory, key, 'run summary mismatch')
            setup = json.loads((directory / 'run.json').read_text())
            scheduling = json.loads((directory / 'scheduling.json').read_text())
            assert scheduling['render_affinity'] == [machine['render_cpu']]
            priority = machine['rt_priority']
            if priority is not None:
                assert scheduling['render_scheduler'] == 1 and scheduling['render_priority'] == priority
            else:
                assert scheduling['render_scheduler'] == machine['scheduler']
                if machine['scheduler'] == 0:
                    assert scheduling['render_priority'] == 0
            memory = {line.split()[0].rstrip(':'): int(line.split()[1]) for line in setup['process_memory']}
            result = {**replay, 'directory': str(directory.relative_to(root)),
                      'matrix': matrix.name, 'repeat': saved['repeat'], 'interference': saved['interference'],
                      'buffer': setup['buffer'], 'quantum': setup['quantum'], 'warmup_seconds': setup['warmup_seconds'],
                      'host_rss_mib': memory['VmRSS'] / 1024,
                      'render_scheduler': scheduling['render_scheduler'],
                      'render_priority': scheduling['render_priority'],
                      'render_affinity': scheduling['render_affinity']}
            runs.append(result)
        matrices[matrix.name] = {'runs': len(rows), 'callbacks': sum(r['callbacks'] for r in rows),
                                 'machine': machine}
        print(f'{matrix.name}: {len(rows)} runs validated', flush=True)
    assert runs, 'no archived runs found'
    assert len(list((root / 'results').rglob('run.json'))) == len(runs), 'extra/unindexed run evidence'
    groups = []
    keys = sorted({(r['matrix'], r['interference'], r['scenario'], r['input'], r['mode']) for r in runs})
    for matrix, interference, scenario, signal, mode in keys:
        selected = [r for r in runs if (r['matrix'], r['interference'], r['scenario'], r['input'], r['mode']) ==
                    (matrix, interference, scenario, signal, mode)]
        callbacks = sum(r['callbacks'] for r in selected)
        overruns = sum(r['overruns'] for r in selected)
        groups.append({
            'matrix': matrix, 'interference': interference, 'scenario': scenario, 'input': signal, 'mode': mode,
            'runs': len(selected), 'buffer': selected[0]['buffer'], 'callbacks': callbacks,
            **{key: statistics.median(r[key] for r in selected) for key in
               ('budget_us', 'p50_us', 'p95_us', 'p99_us', 'late_p99_us', 'host_rss_mib')},
            'max_us': max(r['max_us'] for r in selected),
            'late_max_us': max(r['late_max_us'] for r in selected),
            'overruns': overruns, 'overrun_percent': 100 * overruns / callbacks,
            'resource_copy_kib': statistics.median(r['counters']['resource_copy_bytes']['mean'] for r in selected) / 1024,
            'index_allocated': statistics.median(r['counters']['index_allocated']['mean'] for r in selected),
            'index_reclaimed': statistics.median(r['counters']['index_reclaimed']['mean'] for r in selected),
        })
    validation = {
        'runs': len(runs), 'callbacks': sum(r['callbacks'] for r in runs),
        'matrices': matrices, 'build_manifest_sha256': manifest_sha,
        'verified_member_hashes': verified_members,
        'overruns': {mode: sum(r['overruns'] for r in runs if r['mode'] == mode) for mode in MODES},
        'validated': ['callback identity/continuity/frame counts and zero selected-window render faults',
                      'zero recorder drops and complete CSV termination',
                      'scenario modes, controls, stereo readback and capture completeness',
                      'all run and matrix summary statistics/counters replay',
                      'bundle/binary/source identity and recorded render scheduling'],
    }
    for name, value in [('runs.json', runs), ('groups.json', groups), ('validation.json', validation)]:
        (args.output / name).write_text(json.dumps(value, indent=2) + '\n')
    with (args.output / 'groups.csv').open('w') as output:
        writer = csv.DictWriter(output, list(groups[0]), lineterminator='\n')
        writer.writeheader()
        writer.writerows(groups)
    if args.diagnostics:
        (args.output / 'diagnostics.json').write_text(json.dumps(timing_diagnostics(root, runs), indent=2) + '\n')
    if args.plot:
        plot_groups(groups, args.plot)
    print(f"Validated {len(runs)} runs / {validation['callbacks']:,} callbacks", flush=True)


if __name__ == '__main__':
    main()
