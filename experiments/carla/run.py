#!/usr/bin/env python3
"""Build and run controlled Carla experiments; no audio device or GUI required."""
import argparse
import csv
import hashlib
import json
import math
import os
from pathlib import Path
import random
import shutil
import statistics
import struct
import subprocess
import time
import sys

ROOT = Path(__file__).resolve().parents[2]
ARCHIVE_SHA = 'ae2835b12081f7271a6b0b25d34b87d36b022c40370028ca4a10f90fcedfa661'


def command(args, log, env=None, cwd=None, timeout=1800):
    with open(log, 'w') as output:
        result = subprocess.run([str(v) for v in args], cwd=cwd or ROOT, env=env,
                                stdout=output, stderr=subprocess.STDOUT, timeout=timeout)
    return result.returncode


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def states(work):
    directory = work / 'states'
    directory.mkdir(exist_ok=True)
    start = "<?xml version='1.0'?><!DOCTYPE CARLA-PROJECT><CARLA-PROJECT VERSION='2.5'>"
    (directory / 'empty.xml').write_text(start + '</CARLA-PROJECT>')
    (directory / 'gain.xml').write_text(start + '''<Plugin><Info><Type>INTERNAL</Type><Name>Audio Gain (Stereo)</Name><Label>audiogain_s</Label></Info><Data><Active>Yes</Active><ControlChannel>1</ControlChannel><Options>0x0</Options><Parameter><Index>0</Index><Name>Gain</Name><Value>0.5</Value></Parameter><Parameter><Index>1</Index><Name>Apply Left</Name><Value>1</Value></Parameter><Parameter><Index>2</Index><Name>Apply Right</Name><Value>1</Value></Parameter></Data></Plugin></CARLA-PROJECT>''')
    (directory / 'synthetic.xml').write_text(start + f'''<Plugin><Info><Type>LADSPA</Type><Name>Shoop deterministic DSP workload</Name><Label>shoop_benchmark</Label><Binary>{work}/workload.so</Binary><UniqueID>590001</UniqueID></Info><Data><Active>Yes</Active><ControlChannel>1</ControlChannel><Options>0x0</Options></Data></Plugin></CARLA-PROJECT>''')


def prepare(work):
    """Sources/runtimes must be prepared as documented in README.md."""
    states(work)
    include = work / 'Carla-reference/source/includes'
    for source, flags, output in [
        ('workload.c', ['gcc', '-O3', '-fPIC', '-shared'], 'workload.so'),
        ('native_timing.c', ['gcc', '-O2', '-fPIC', '-shared'], 'native_timing.so'),
        ('abi_benchmark.cpp', ['g++', '-O2', '-std=c++17'], 'abi_benchmark'),
    ]:
        args = flags + ['-I', include, ROOT / 'experiments/carla' / source, '-ldl', '-o', work / output]
        if command(args, work / (output + '.build.log')):
            raise RuntimeError('build failed: ' + output)
    manifest = {'baseline': '580f6191c9b2d00c6ad43fcbf30445e590235a49',
                'pre': '9b169b5ec4f3a9ea86b4e0bdffbbd71882be4069',
                'post': '0cec63cc03ce0e3d3046e8f06f8de1b3104675be',
                'v0.2.0': 'd08aafcb049f6c7a848c97a18b5c3a1456046696',
                'cpu_max': Path('/sys/fs/cgroup/cpu.max').read_text(),
                'affinity': sorted(os.sched_getaffinity(0)),
                'scheduler': os.sched_getscheduler(0), 'artifacts': {}}
    for path in [work / 'workload.so', work / 'native_timing.so', work / 'abi_benchmark'] + [
        work / f'Carla-{recipe}/bin/{library}' for recipe in ['reference', 'packaged']
        for library in ['libcarla_native-plugin.so', 'carla.lv2/carla.so']
    ]:
        manifest['artifacts'][str(path)] = digest(path)
    if (work / 'carla.tar.gz').exists():
        assert digest(work / 'carla.tar.gz') == ARCHIVE_SHA
        manifest['carla_archive_sha256'] = ARCHIVE_SHA
    for tool in ['gcc', 'g++', 'rustc', 'cargo']:
        manifest[tool] = subprocess.check_output([tool, '--version'], text=True).splitlines()[0]
    (work / 'manifest.json').write_text(json.dumps(manifest, indent=2))


def environment(work, capture=None, recipe='reference', shim=True):
    env = os.environ.copy()
    runtime = work / f'Carla-{recipe}/bin'
    env['LD_LIBRARY_PATH'] = str(work / 'deps/usr/lib/x86_64-linux-gnu') + ':' + env.get('LD_LIBRARY_PATH', '')
    env['SHOOP_CARLA_NATIVE_LIBRARY'] = str(work / 'native_timing.so' if shim else runtime / 'libcarla_native-plugin.so')
    env['SHOOP_CARLA_RESOURCE_DIR'] = str(work / f'Carla-{recipe}/data')
    env['SHOOP_BENCH_REAL_CARLA'] = str(runtime / 'libcarla_native-plugin.so')
    if capture:
        env['SHOOP_BENCH_CAPTURE'] = str(capture)
    else:
        env.pop('SHOOP_BENCH_CAPTURE', None)
    env['LV2_PATH'] = str(runtime)
    return env


def percentile(values, fraction):
    values = sorted(values)
    return values[round((len(values)-1)*fraction)]


def summarize(directory):
    rows = []
    for metadata in sorted(directory.glob('*.json')):
        if metadata.name == 'summary.json':
            continue
        data = json.loads(metadata.read_text())
        if 'case' not in data:
            continue
        row = {k: data[k] for k in ['case', 'repeat', 'status']}
        path = metadata.with_suffix('.csv')
        if data['status'] == 'ok' and path.exists():
            samples = list(csv.DictReader(path.open()))
            wall = [int(v['wall_ns']) for v in samples]
            cpu = [int(v.get('cpu_ns', v.get('caller_cpu_ns', 0))) for v in samples]
            row.update(blocks=len(wall), mean_wall_ns=statistics.mean(wall),
                       p99_wall_ns=percentile(wall, .99), p50_wall_ns=percentile(wall, .5),
                       p95_wall_ns=percentile(wall, .95), max_wall_ns=max(wall), mean_caller_cpu_ns=statistics.mean(cpu),
                       invalid=sum(v.get('valid', 'true') != 'true' for v in samples),
                       misses=max(int(v.get('bridge_deadline_misses', 0)) for v in samples))
            capture = metadata.with_suffix('.bin')
            if capture.exists():
                blob = capture.read_bytes()
                count, capacity = struct.unpack_from('QQ', blob)
                inner = [struct.unpack_from('QQQQ', blob, 32+i*32) for i in range(min(count, capacity))]
                # Join to the caller's measured interval; do not mix warm-up or late stale completions.
                intervals = [(int(v['start_ns']), int(v['end_ns'])) for v in samples if 'start_ns' in v]
                selected = []
                j = 0
                for item in inner:
                    while j < len(intervals) and intervals[j][1] < item[0]:
                        j += 1
                    if j < len(intervals) and intervals[j][0] <= item[0] and item[1] <= intervals[j][1]:
                        selected.append((item, intervals[j]))
                row['capture_count'] = count
                row['capture_overflow'] = count >= capacity
                row['matched_calls'] = len(selected)
                if selected:
                    row['mean_inner_cpu_ns'] = statistics.mean(v[0][2] for v in selected)
                    row['mean_inner_wall_ns'] = statistics.mean(v[0][1]-v[0][0] for v in selected)
                    row['p99_inner_wall_ns'] = percentile([v[0][1]-v[0][0] for v in selected], .99)
                    row['mean_before_inner_ns'] = statistics.mean(v[0][0]-v[1][0] for v in selected)
                    row['actual_frames'] = sorted(set(v[0][3] for v in selected))
        rows.append(row)
    (directory / 'summary.json').write_text(json.dumps(rows, indent=2))
    fields = sorted(set().union(*(v.keys() for v in rows))) if rows else ['case']
    with (directory / 'summary.csv').open('w') as output:
        writer = csv.DictWriter(output, fieldnames=fields); writer.writeheader(); writer.writerows(rows)
    return rows


def run(work, directory, suite, repeats, blocks, paced_blocks):
    directory.mkdir(parents=True, exist_ok=True)
    rust = work / 'carla_regression_benchmark'
    cases = []
    if suite in ['abi', 'all']:
        for frames in [1024, 256, 128]:
            for workload in ['empty', 'gain', 'synthetic']:
                for recipe in ['reference', 'packaged']:
                    for mode in ['native', 'lv2']:
                        cases.append(('abi', recipe, mode, workload, frames, 'continuous'))
    if suite in ['engine', 'all']:
        for frames in [1024, 256, 128]:
            for workload in ['empty', 'gain', 'synthetic']:
                for mode in ['native', 'bridge', 'subprocess', 'engine']:
                    cases.append(('engine', 'reference', mode, workload, frames, 'continuous'))
            cases.append(('engine', 'reference', 'no_fx', 'empty', frames, 'continuous'))
    if suite in ['paced', 'all']:
        for frames in [1024, 256, 128]:
            for mode in ['native', 'bridge', 'subprocess', 'engine']:
                cases.append(('engine', 'reference', mode, 'synthetic', frames, 'paced'))
    if suite == 'contention':
        for mode in ['native', 'bridge', 'subprocess', 'engine']:
            cases.append(('engine', 'reference', mode, 'synthetic', 1024, 'paced'))
    if suite == 'controls':
        for frames in [1024, 128]:
            for workload in ['empty', 'synthetic']:
                for mode in ['native', 'native-bare', 'native-repeat']:
                    cases.append(('controls', 'reference', mode, workload, frames, 'continuous'))
    if suite == 'history':
        for revision in ['pre', 'post']:
            for frames in [1024, 256, 128]:
                for workload in ['empty', 'gain', 'synthetic']:
                    for mode in ['native', 'bridge', 'subprocess', 'engine']:
                        cases.append(('history', revision, mode, workload, frames, 'continuous'))
                cases.append(('history', revision, 'no_fx', 'empty', frames, 'continuous'))
        for workload in ['empty', 'gain', 'synthetic']:
            cases.append(('legacy', '020', 'lv2', workload, 128, 'continuous'))
            cases.append(('legacy', '020', 'lv2', workload, 1024, 'paced'))
    for repeat in range(repeats):
        order = cases if repeat % 2 == 0 else list(reversed(cases))
        for kind, recipe, mode, workload, frames, pacing in order:
            case = f'{kind}-{recipe}-{mode}-{workload}-{frames}-{pacing}'
            prefix = directory / f'{case}-{repeat:02d}'
            if prefix.with_suffix('.json').exists():
                continue
            count = paced_blocks if pacing == 'paced' else blocks
            if kind == 'abi':
                library = 'libcarla_native-plugin.so' if mode == 'native' else 'carla.lv2/carla.so'
                args = [work / 'abi_benchmark', mode, work / f'Carla-{recipe}/bin' / library,
                        work / f'Carla-{recipe}/bin/carla.lv2', work / 'states' / f'{workload}.xml', frames, count, prefix.with_suffix('.csv')]
            else:
                executable = work / f'{recipe}_benchmark' if kind == 'history' else rust
                args = [executable, mode.replace('-bare', '').replace('-repeat', ''), work / 'states' / f'{workload}.xml', workload, frames, count, pacing, prefix.with_suffix('.csv')]
            env = environment(work, prefix.with_suffix('.bin') if kind != 'abi' else None, recipe if kind == 'abi' else 'reference', shim=not mode.endswith('-bare'))
            if kind == 'history' and recipe == 'pre':
                env['LV2_PATH'] = str(work / 'legacy-lv2')
                env['SHOOP_BENCH_REAL_CARLA'] = str(work / 'Carla-reference/bin/carla.lv2/carla.so')
            if kind == 'legacy':
                args = ['python3', ROOT / 'experiments/carla/legacy020.py', work / 'states' / f'{workload}.xml', frames, count, prefix.with_suffix('.csv')]
                release = work / 'release-020/shoopdaloop-0.2.0.release-linux-x64.portable/lib'
                env['SHOOP_BENCH_020_LIBRARY'] = str(release / 'libshoopdaloop_backend.so')
                env['LD_LIBRARY_PATH'] = str(release) + ':' + env['LD_LIBRARY_PATH']
                env['LV2_PATH'] = str(work / 'legacy-lv2')
                env['SHOOP_BENCH_REAL_CARLA'] = str(work / 'Carla-reference/bin/carla.lv2/carla.so')
            before = Path('/sys/fs/cgroup/cpu.stat').read_text()
            started = time.time()
            code = command(args, prefix.with_suffix('.log'), env, timeout=max(120, count*frames/48000*3))
            metadata = {'case': case, 'repeat': repeat, 'status': 'ok' if code == 0 else 'failed',
                        'returncode': code, 'command': [str(v) for v in args], 'elapsed_seconds': time.time()-started,
                        'affinity': sorted(os.sched_getaffinity(0)), 'scheduler': os.sched_getscheduler(0), 'cpu_stat_before': before, 'cpu_stat_after': Path('/sys/fs/cgroup/cpu.stat').read_text()}
            prefix.with_suffix('.json').write_text(json.dumps(metadata, indent=2))
            print(f'{case} repeat={repeat} {metadata["status"]}', flush=True)
    summarize(directory)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('action', choices=['prepare', 'run', 'summarize'])
    parser.add_argument('--work', type=Path, required=True)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--suite', choices=['abi', 'engine', 'paced', 'history', 'controls', 'contention', 'all'], default='all')
    parser.add_argument('--repeats', type=int, default=10)
    parser.add_argument('--blocks', type=int, default=2000)
    parser.add_argument('--paced-blocks', type=int, default=200)
    parser.add_argument('--pin-cpu', type=int)
    parser.add_argument('--load', action='store_true')
    args = parser.parse_args()
    work = args.work.resolve()
    if args.pin_cpu is not None:
        os.sched_setaffinity(0, {args.pin_cpu})
    if args.action == 'prepare':
        prepare(work)
    elif args.action == 'summarize':
        summarize(args.output)
    else:
        load = subprocess.Popen([sys.executable, '-c', 'while True: pass']) if args.load else None
        try:
            run(work, args.output or work / 'results', args.suite, args.repeats, args.blocks, args.paced_blocks)
        finally:
            if load:
                load.terminate(); load.wait()


if __name__ == '__main__':
    main()
