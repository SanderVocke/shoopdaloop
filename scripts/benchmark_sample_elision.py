#!/usr/bin/env python3
"""Pair compact callbacks with and without compile-time sample kernels."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import statistics
import sys

from benchmark_audio_chunks import run_launcher, source_info, summarize


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--normal-host', required=True, type=Path)
    parser.add_argument('--normal-launcher', required=True, type=Path)
    parser.add_argument('--elided-host', required=True, type=Path)
    parser.add_argument('--elided-launcher', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--buffers', nargs='+', type=int, default=[256, 32])
    parser.add_argument('--scenarios', nargs='+', default=['idle:silence', 'play', 'play:silence', 'ramps'])
    parser.add_argument('--repeats', type=int, default=4)
    parser.add_argument('--seconds', type=float, default=1.024)
    parser.add_argument('--warmup', type=float, default=.5)
    parser.add_argument('--host-cpu', required=True, type=int)
    parser.add_argument('--render-cpu', required=True, type=int)
    args = parser.parse_args()
    allowed = os.sched_getaffinity(0)
    assert args.host_cpu in allowed and args.render_cpu in allowed
    assert args.host_cpu != args.render_cpu
    assert args.repeats > 0 and args.seconds > 0
    assert all(buffer > 0 and buffer % 32 == 0 for buffer in args.buffers)
    args.output.mkdir(parents=True, exist_ok=False)
    variants = {
        'samples-on': (args.normal_host.resolve(), args.normal_launcher.resolve()),
        'samples-off': (args.elided_host.resolve(), args.elided_launcher.resolve()),
    }
    identity = {
        **source_info(), 'command': sys.argv, 'platform': platform.platform(),
        'allowed_cpus': sorted(allowed), 'host_cpu': args.host_cpu, 'render_cpu': args.render_cpu,
        'scheduler': os.sched_getscheduler(0),
        'cpu_quota': Path('/sys/fs/cgroup/cpu.max').read_text().strip(),
        'cpu_stat_before': Path('/sys/fs/cgroup/cpu.stat').read_text(),
        'builds': {label: {'host': str(host), 'host_sha256': digest(host),
                           'launcher': str(launcher), 'launcher_sha256': digest(launcher)}
                  for label, (host, launcher) in variants.items()},
        'scope': 'Elapsed dummy callbacks, including input generation. Sample-free builds preserve metadata but intentionally produce invalid audio.',
    }
    (args.output / 'machine.json').write_text(json.dumps(identity, indent=2) + '\n')
    summaries = []
    for buffer in args.buffers:
        callbacks = math.ceil(args.seconds * 48000 / buffer)
        for case in args.scenarios:
            scenario, _, signal = case.partition(':')
            signal = signal or 'signal'
            for repeat in range(args.repeats):
                order = list(variants)
                if repeat % 2:
                    order.reverse()
                for label in order:
                    directory = args.output / str(buffer) / f'{scenario}-{signal}' / f'{repeat}-{label}'
                    directory.mkdir(parents=True)
                    host, launcher = variants[label]
                    command = ['taskset', '-c', str(args.host_cpu), str(launcher),
                               '--host', str(host), '--mode', 'compact', '--scenario', scenario,
                               '--input', signal, '--callbacks', str(callbacks), '--warmup', str(args.warmup),
                               '--output', str(directory), '--buffer', str(buffer), '--quantum', '32']
                    run_launcher(command, directory, args.render_cpu)
                    result = summarize(directory)
                    assert result['sample_processing'] == (label == 'samples-on'), 'wrong diagnostic build'
                    summaries.append({**result, 'buffer': buffer, 'repeat': repeat, 'build': label})
                    (args.output / 'results.json').write_text(json.dumps(summaries, indent=2) + '\n')
    groups = []
    for buffer, scenario, signal in sorted({(r['buffer'], r['scenario'], r['input']) for r in summaries}):
        group = {'buffer': buffer, 'scenario': scenario, 'input': signal}
        for label in variants:
            rows = [r for r in summaries if (r['buffer'], r['scenario'], r['input'], r['build'])
                    == (buffer, scenario, signal, label)]
            group[label] = {'p50_us': statistics.median(r['p50_us'] for r in rows),
                            'p99_us': statistics.median(r['p99_us'] for r in rows),
                            'max_us': max(r['max_us'] for r in rows),
                            'overruns': sum(r['overruns'] for r in rows)}
        group['residual_fraction'] = group['samples-off']['p50_us'] / group['samples-on']['p50_us']
        groups.append(group)
    (args.output / 'summary.json').write_text(json.dumps(groups, indent=2) + '\n')
    identity['cpu_stat_after'] = Path('/sys/fs/cgroup/cpu.stat').read_text()
    (args.output / 'machine.json').write_text(json.dumps(identity, indent=2) + '\n')
    print(json.dumps(groups, indent=2))


if __name__ == '__main__':
    main()
