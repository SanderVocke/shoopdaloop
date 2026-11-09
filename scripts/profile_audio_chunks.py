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

from benchmark_audio_chunks import MODES, configure_render, render_thread, source_info, summarize


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
    callbacks = args.launcher_callbacks or math.ceil(
        (args.settle + args.duration + 5) * 48000 / args.buffer
    )
    launch = ['taskset', '-c', str(args.host_cpu), str(args.launcher),
              '--host', str(args.host), '--mode', mode, '--scenario', scenario,
              '--input', 'silence' if scenario == 'idle' else 'signal',
              '--callbacks', str(callbacks), '--warmup', str(args.warmup),
              '--output', str(directory), '--buffer', str(args.buffer),
              '--quantum', str(args.quantum), '--tracks', str(args.tracks),
              '--loops', str(args.loops), '--voices', str(args.voices)]
    if args.loop_memory_mb is not None:
        launch.extend(['--loop-memory-mb', str(args.loop_memory_mb)])
    host_pid = None
    with (directory / 'launcher.log').open('w') as output:
        started = time.monotonic()
        process = subprocess.Popen(launch, stdout=output, stderr=subprocess.STDOUT)
        try:
            host_pid, tid = render_thread(process.pid, started + 30)
            scheduling = configure_render(host_pid, tid, directory, args.render_cpu, args.rt_priority)
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
            identity = source_info()
            metadata = {
                'mode': mode, 'scenario': scenario, 'render_tid': tid, 'host_pid': host_pid,
                'scheduling': scheduling, 'buffer': args.buffer, 'quantum': args.quantum,
                'launcher_command': launch, 'perf_command': sample,
                'sample_start_after_launch_seconds': sampled_at - started,
                'sampling_wall_seconds': sampled_until - sampled_at,
                'source_sha': identity['git_sha'], 'source_dirty': identity['dirty'],
                'source': identity,
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
    parser.add_argument('--render-cpu', type=int)
    parser.add_argument('--rt-priority', type=int)
    parser.add_argument('--collector-cpu', type=int, default=0)
    parser.add_argument('--buffer', type=int, default=256)
    parser.add_argument('--quantum', type=int, default=32)
    parser.add_argument('--tracks', type=int, default=4)
    parser.add_argument('--loops', type=int, default=16)
    parser.add_argument('--voices', type=int, default=4)
    parser.add_argument('--loop-memory-mb', type=int)
    parser.add_argument('--warmup', type=float, default=0.5)
    parser.add_argument('--launcher-callbacks', type=int)
    parser.add_argument('--duration', type=float, default=30)
    parser.add_argument('--settle', type=float, default=8)
    parser.add_argument('--frequency', type=int, default=997)
    parser.add_argument('--stack-bytes', type=int, default=32768)
    parser.add_argument('--modes', nargs='+', choices=MODES, default=list(MODES))
    parser.add_argument('--scenarios', nargs='+',
                        choices=('idle', 'play', 'record', 'grab', 'ramps'),
                        default=['idle', 'play', 'ramps'])
    args = parser.parse_args()
    if args.duration <= 0 or args.settle < 5 or args.frequency <= 0:
        parser.error('duration/frequency must be positive and settle must be at least five seconds')
    if args.buffer <= 0 or args.quantum <= 0 or args.warmup < 0:
        parser.error('buffer and quantum must be positive; warmup must be non-negative')
    if args.tracks <= 0 or args.loops <= 0 or args.voices <= 0:
        parser.error('tracks, loops and voices must be positive')
    if args.loop_memory_mb is not None and args.loop_memory_mb <= 0:
        parser.error('loop memory must be positive')
    if args.launcher_callbacks is not None and args.launcher_callbacks <= 0:
        parser.error('launcher callbacks must be positive')
    if args.rt_priority is not None and not 1 <= args.rt_priority <= 99:
        parser.error('RT priority must be in 1..99')
    allowed = os.sched_getaffinity(0)
    if args.host_cpu not in allowed or args.collector_cpu not in allowed or args.host_cpu == args.collector_cpu:
        parser.error('choose two distinct allowed CPUs')
    if args.render_cpu is not None:
        if args.render_cpu not in allowed or args.render_cpu in (args.host_cpu, args.collector_cpu):
            parser.error('choose a separate allowed render CPU')
        os.sched_setaffinity(0, {args.host_cpu})
    for name in ('host', 'launcher', 'perf', 'output'):
        setattr(args, name, getattr(args, name).resolve())
    args.output.mkdir(parents=True, exist_ok=True)
    for scenario in args.scenarios:
        for mode in args.modes:
            collect(args, args.output / scenario / mode, mode, scenario)


if __name__ == '__main__':
    main()
