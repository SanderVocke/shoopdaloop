#!/usr/bin/env python3
"""Sample the existing native render thread without changing its executable."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import subprocess
import time

from benchmark_audio_chunks import MODES, summarize


def render_thread(launcher_pid, deadline):
    while time.monotonic() < deadline:
        for proc in Path('/proc').glob('[0-9]*'):
            try:
                stat = (proc / 'stat').read_text()
                if int(stat[stat.rfind(')') + 2:].split()[1]) != launcher_pid:
                    continue
                for task in (proc / 'task').iterdir():
                    if (task / 'comm').read_text().strip() == 'shoop-rt-dummy':
                        return int(proc.name), int(task.name)
            except (FileNotFoundError, ProcessLookupError):
                continue
        time.sleep(0.05)
    raise RuntimeError('native dummy render thread did not become ready')


def stop_owned_host(pid, host, output):
    if pid is None:
        return
    proc = Path('/proc') / str(pid)
    try:
        if ((proc / 'exe').resolve() == host
                and str(output / 'config.json').encode() in (proc / 'cmdline').read_bytes()):
            os.kill(pid, 15)
    except (FileNotFoundError, ProcessLookupError):
        pass


def collect(args, directory, mode, scenario):
    directory.mkdir(parents=True, exist_ok=False)
    callbacks = math.ceil((args.settle + args.duration + 5) * 48000 / 256)
    launch = ['taskset', '-c', str(args.host_cpu), str(args.launcher),
              '--host', str(args.host), '--mode', mode, '--scenario', scenario,
              '--input', 'silence' if scenario == 'idle' else 'signal',
              '--callbacks', str(callbacks), '--warmup', '0.5', '--output', str(directory)]
    host_pid = None
    with (directory / 'launcher.log').open('w') as output:
        started = time.monotonic()
        process = subprocess.Popen(launch, stdout=output, stderr=subprocess.STDOUT)
        try:
            host_pid, tid = render_thread(process.pid, started + 30)
            time.sleep(max(0, args.settle - (time.monotonic() - started)))
            if process.poll() is not None:
                raise RuntimeError('benchmark ended before sampling began')
            sample = ['taskset', '-c', str(args.collector_cpu), str(args.perf), 'record',
                      '--per-thread', '--no-inherit', '--no-bpf-event',
                      '--no-kernel-callchains', '--user-callchains',
                      '-e', 'cpu-clock:u', '-F', str(args.frequency), '-T',
                      '--call-graph', f'dwarf,{args.stack_bytes}', '-t', str(tid),
                      '-o', str(directory / 'perf.data'), '--', 'sleep', str(args.duration)]
            env = os.environ.copy()
            env['DEBUGINFOD_URLS'] = ''
            if args.perf_library_path:
                env['LD_LIBRARY_PATH'] = str(args.perf_library_path) + ':' + env.get('LD_LIBRARY_PATH', '')
            sampled_at = time.monotonic()
            with (directory / 'perf.log').open('w') as log:
                subprocess.run(sample, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
            sampled_until = time.monotonic()
            result = process.wait(timeout=180)
            if result:
                raise RuntimeError(f'benchmark failed with exit status {result}')
            validation = summarize(directory)
            metadata = {
                'mode': mode, 'scenario': scenario, 'render_tid': tid, 'host_pid': host_pid,
                'launcher_command': launch, 'perf_command': sample,
                'sample_start_after_launch_seconds': sampled_at - started,
                'sampling_wall_seconds': sampled_until - sampled_at,
                'source_sha': subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
                'source_dirty': bool(subprocess.check_output(['git', 'status', '--porcelain'], text=True).strip()),
                'host_sha256': hashlib.sha256(args.host.read_bytes()).hexdigest(),
                'launcher_sha256': hashlib.sha256(args.launcher.read_bytes()).hexdigest(),
                'scope': 'Render-thread on-CPU user time; profiling timings are not unprofiled benchmark results.',
                'validation': {key: validation[key] for key in ('callbacks', 'overruns', 'dropped')},
            }
            (directory / 'sampling.json').write_text(json.dumps(metadata, indent=2) + '\n')
            print(f'{scenario}/{mode}: captured and validated', flush=True)
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
            stop_owned_host(host_pid, args.host, directory)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--host', required=True, type=Path)
    parser.add_argument('--launcher', required=True, type=Path)
    parser.add_argument('--perf', required=True, type=Path)
    parser.add_argument('--perf-library-path', type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--host-cpu', type=int, default=2)
    parser.add_argument('--collector-cpu', type=int, default=0)
    parser.add_argument('--duration', type=float, default=30)
    parser.add_argument('--settle', type=float, default=8)
    parser.add_argument('--frequency', type=int, default=997)
    parser.add_argument('--stack-bytes', type=int, default=32768)
    parser.add_argument('--modes', nargs='+', choices=MODES, default=list(MODES))
    parser.add_argument('--scenarios', nargs='+', choices=('idle', 'play', 'ramps'),
                        default=['idle', 'play', 'ramps'])
    args = parser.parse_args()
    if args.duration <= 0 or args.settle < 5 or args.frequency <= 0:
        parser.error('duration/frequency must be positive and settle must be at least five seconds')
    allowed = os.sched_getaffinity(0)
    if args.host_cpu not in allowed or args.collector_cpu not in allowed or args.host_cpu == args.collector_cpu:
        parser.error('choose two distinct allowed CPUs')
    for name in ('host', 'launcher', 'perf', 'output'):
        setattr(args, name, getattr(args, name).resolve())
    args.output.mkdir(parents=True, exist_ok=True)
    for scenario in args.scenarios:
        for mode in args.modes:
            collect(args, args.output / scenario / mode, mode, scenario)


if __name__ == '__main__':
    main()
