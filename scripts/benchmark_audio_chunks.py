#!/usr/bin/env python3
"""Run the normal app-connected host and summarize every measured callback."""
import argparse
import csv
import ctypes
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys
import time

MODES = ("current", "chunks", "chunks-descriptors")


def worker(kind, cpu):
    if cpu is not None:
        os.sched_setaffinity(0, {cpu})
    if kind == "memory":
        size = 64 * 1024 * 1024
        source = ctypes.create_string_buffer(b"x" * size)
        target = ctypes.create_string_buffer(size)
        while True:
            ctypes.memmove(target, source, size)
    else:
        while True:
            started = time.monotonic()
            # Periodic process-list-like allocation/scanning, deliberately repeatable.
            values = sorted((i * 7919) % 104729 for i in range(30000))
            assert len(values) == 30000
            time.sleep(max(0, 1 - (time.monotonic() - started)))


def percentile(values, p):
    values = sorted(values)
    return values[max(0, math.ceil(len(values) * p) - 1)]


def summarize(directory):
    run = json.loads((directory / "run.json").read_text())
    lines = (directory / "callbacks.csv").read_text().splitlines()
    assert lines[-1] == "# dropped=0", "missing completion or dropped callback records"
    assert lines[0].startswith(f"# mode={run['mode']} "), "renderer identity mismatch"
    rows = list(csv.DictReader(line for line in lines if not line.startswith("#")))
    rows = [{key: int(value) for key, value in row.items()} for row in rows]
    rows = [row for row in rows if run["first_frame"] <= row["frame"] < run["last_frame"]]
    assert len(rows) == run["callbacks"], (directory, len(rows), run["callbacks"])
    assert [row["frame"] for row in rows] == list(range(run["first_frame"], run["last_frame"], run["buffer"])), "missing/duplicate callbacks"
    assert all(row["frames"] == run["buffer"] and row["render_fault"] == 0 for row in rows)
    durations = [row["elapsed_ns"] / 1000 for row in rows]
    late = [row["late_ns"] / 1000 for row in rows]
    budget = run["buffer"] / 48000 * 1e6
    counters = {key: {"mean": statistics.mean(row[key] for row in rows), "max": max(row[key] for row in rows)}
                for key in rows[0] if key not in ("frame", "frames", "elapsed_ns", "late_ns", "render_fault")}
    result = {"mode": run["mode"], "scenario": run["scenario"], "input": run["input"],
              "callbacks": len(rows), "budget_us": budget, "p50_us": statistics.median(durations),
              "p95_us": percentile(durations, .95), "p99_us": percentile(durations, .99),
              "max_us": max(durations), "overruns": sum(value > budget for value in durations),
              "late_p99_us": percentile(late, .99), "late_max_us": max(late),
              "dropped": 0, "counters": counters, "memory": lines[1], "directory": str(directory)}
    (directory / "summary.json").write_text(json.dumps(result, indent=2) + "\n")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", type=Path)
    parser.add_argument("--launcher", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--callbacks", type=int, default=384)
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--warmup", type=float, default=.5)
    parser.add_argument("--scenarios", nargs="+", default=["idle", "record:silence", "record", "play", "record-play", "shared", "continue", "churn", "ramps"])
    parser.add_argument("--quantum", type=int, default=32)
    parser.add_argument("--buffer", type=int, default=256)
    parser.add_argument("--voices", type=int, default=4)
    parser.add_argument("--host-cpu", type=int, help="Pin the launcher and all host threads (not render-only affinity)")
    parser.add_argument("--load-cpu", type=int)
    parser.add_argument("--interference", nargs="+", choices=["none", "periodic", "memory"], default=["none"])
    parser.add_argument("--worker", choices=["periodic", "memory"], help=argparse.SUPPRESS)
    args = parser.parse_args()
    if args.worker:
        worker(args.worker, args.load_cpu)
        return
    assert args.host and args.launcher and args.output
    assert args.callbacks > 0 and args.repeats > 0
    allowed = sorted(os.sched_getaffinity(0)) if hasattr(os, "sched_getaffinity") else []
    if any(kind != "none" for kind in args.interference):
        assert args.load_cpu is not None, "select another allowed core explicitly with --load-cpu"
        assert args.load_cpu in allowed and args.load_cpu != args.host_cpu
    if args.host_cpu is not None:
        assert args.host_cpu in allowed
    args.output.mkdir(parents=True, exist_ok=True)
    def command(*argv):
        return subprocess.check_output(argv, text=True).strip()
    def proc(path):
        try:
            return Path(path).read_text().strip()
        except OSError:
            return None
    metadata = {"command": sys.argv, "platform": platform.platform(), "machine": platform.machine(),
                "cpu_model": [line for line in (proc('/proc/cpuinfo') or '').splitlines() if line.startswith(('model name', 'Hardware', 'Model'))][:2],
                "affinity": allowed, "host_cpu": args.host_cpu, "load_cpu": args.load_cpu,
                "cpu_quota": proc('/sys/fs/cgroup/cpu.max'), "memory_limit": proc('/sys/fs/cgroup/memory.max'),
                "preempt": proc('/sys/kernel/realtime'), "isolated": proc('/sys/devices/system/cpu/isolated'),
                "git_sha": command('git','rev-parse','HEAD'), "dirty": bool(command('git','status','--porcelain')),
                "host_sha256": hashlib.sha256(args.host.read_bytes()).hexdigest(),
                "launcher_sha256": hashlib.sha256(args.launcher.read_bytes()).hexdigest(),
                "clock": time.get_clock_info('perf_counter')._asdict() if hasattr(time.get_clock_info('perf_counter'), '_asdict') else vars(time.get_clock_info('perf_counter')),
                "rt_configuration": "External; this script does not change scheduling, IRQs or kernel configuration.",
                "counter_scope": "Software work, not measured DDR/PMU traffic. Graph counters describe the reference evaluator only; zeros in current are uninstrumented."}
    (args.output / 'machine.json').write_text(json.dumps(metadata, indent=2) + '\n')
    results = []
    for interference in args.interference:
        load = None
        try:
            if interference != 'none':
                load = subprocess.Popen([sys.executable, __file__, '--worker', interference, '--load-cpu', str(args.load_cpu)])
            for repeat in range(args.repeats):
                order = MODES[repeat % 3:] + MODES[:repeat % 3]
                for scenario in args.scenarios:
                    scenario, _, signal = scenario.partition(':')
                    signal = signal or 'signal'
                    for mode in order:
                        directory = args.output / interference / f'{scenario}-{signal}' / f'{repeat}-{mode}'
                        argv = [str(args.launcher.resolve()), '--host', str(args.host.resolve()), '--mode', mode,
                                '--scenario', scenario, '--input', signal, '--callbacks', str(args.callbacks),
                                '--output', str(directory), '--quantum', str(args.quantum), '--buffer', str(args.buffer), '--voices', str(args.voices), '--warmup', str(args.warmup)]
                        if args.host_cpu is not None:
                            argv = ['taskset','--cpu-list',str(args.host_cpu)] + argv
                        subprocess.run(argv, check=True)
                        result = summarize(directory)
                        result.update(repeat=repeat, interference=interference)
                        results.append(result)
                        (args.output / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
        finally:
            if load is not None:
                load.terminate()
                load.wait()
    columns = ['interference','scenario','input','mode','repeat','callbacks','p50_us','p95_us','p99_us','max_us','overruns','late_p99_us','late_max_us','dropped']
    with (args.output / 'summary.csv').open('w') as output:
        writer = csv.DictWriter(output, columns, extrasaction='ignore')
        writer.writeheader()
        writer.writerows(results)
    print(args.output / 'summary.csv')


if __name__ == '__main__':
    main()
