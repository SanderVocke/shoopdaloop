#!/usr/bin/env python3
"""Measure guarded rt_runtime callbacks; correctness/bounds, not timing, gate success."""
from __future__ import annotations

import argparse
import csv
import hashlib
import heapq
import json
import os
from pathlib import Path
import platform
import subprocess
import time
import tomllib

ROOT = Path(__file__).resolve().parents[1]
REPORTS = ROOT / "target/rt-runtime-bench"
CASES = {"idle", "dag-16", "dense-dag-64", "midi-64-through-8", "stereo-capture", "graph-transfers", "resource-roundtrip"}


def run(command, log, env, affinity=None):
    kwargs = {"preexec_fn": (lambda: os.sched_setaffinity(0, {affinity}))} if affinity is not None else {}
    result = subprocess.run(command, cwd=ROOT, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, timeout=300, **kwargs)
    log.write_text(result.stdout + result.stderr)
    return result


def text(path):
    try:
        return Path(path).read_text().strip()
    except OSError:
        return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--iterations", type=int, default=20000)
    parser.add_argument("--trials", type=int, default=3)
    parser.add_argument("--cpu", type=int, help="Linux affinity CPU; default first available CPU")
    parser.add_argument("--no-build", action="store_true")
    args = parser.parse_args()
    if not 1024 <= args.iterations <= 1000000 or not 1 <= args.trials <= 10:
        parser.error("iterations must be 1024..1000000 and trials 1..10")
    available = sorted(os.sched_getaffinity(0)) if hasattr(os, "sched_getaffinity") else []
    cpu = args.cpu if args.cpu is not None else (available[0] if available else None)
    if cpu is not None and cpu not in available:
        parser.error("requested CPU is not available for affinity")
    REPORTS.mkdir(parents=True, exist_ok=True)
    env = {**os.environ, "RUSTFLAGS": os.environ.get("RUSTFLAGS", "-D warnings")}
    suffix = ".exe" if os.name == "nt" else ""
    canaries = []
    for profile in ("debug", "release"):
        if not args.no_build:
            command = ["cargo", "build", "--locked", "-p", "rt_runtime", "--example", "bounded_runtime"] + (["--release"] if profile == "release" else [])
            built = run(command, REPORTS / f"build-{profile}.log", env)
            if built.returncode:
                raise RuntimeError(built.stderr)
        binary = ROOT / "target" / profile / "examples" / ("bounded_runtime" + suffix)
        for kind in ("allocation", "reallocation", "deallocation"):
            result = run([str(binary), "--fail-canary", kind], REPORTS / f"canary-{profile}-{kind}.log", env)
            if result.returncode == 0 or "heap operation during rendering" not in result.stderr:
                raise RuntimeError(f"{profile} {kind} did not fail through the real heap guard\n{result.stderr}")
            canaries.append({"profile": profile, "kind": kind, "expected_failure_observed": True, "exit_code": result.returncode})
    trials = []
    binary = ROOT / "target/release/examples" / ("bounded_runtime" + suffix)
    for trial in range(args.trials):
        raw = REPORTS / f"trial-{trial}-raw"
        started = time.monotonic()
        result = run([str(binary), "--iterations", str(args.iterations), "--raw-dir", str(raw)], REPORTS / f"trial-{trial}.log", env, cpu)
        if result.returncode:
            raise RuntimeError(result.stderr)
        records = [json.loads(line) for line in result.stdout.splitlines() if line.startswith("{")]
        if len(records) != len(CASES) or {record["case"] for record in records} != CASES:
            raise RuntimeError("missing benchmark cases")
        for record in records:
            if record["callbacks"] != args.iterations or record["rust_heap_operations"] != 0 or record["copy_bytes_max"] > 2048 or record["validation_steps_max"] > 128:
                raise RuntimeError(f"callback count/heap/work contract failure: {record}")
            with (raw / f"{record['case']}.csv").open() as source:
                rows = [{name: int(value) for name, value in row.items()} for row in csv.DictReader(source)]
            if len(rows) != args.iterations or max(row["nanoseconds"] for row in rows) != record["max_ns"]:
                raise RuntimeError("raw timing evidence differs from summary")
            record["slowest_callbacks"] = heapq.nlargest(5, rows, key=lambda row: row["nanoseconds"])
            record["tail_candidates"] = sum(row["nanoseconds"] > max(50000, record["p99_ns"] * 8) for row in rows)
            print(f"trial={trial} {record['case']}: p50={record['p50_ns']}ns p99={record['p99_ns']}ns max={record['max_ns']}ns misses={record['deadline_misses']}", flush=True)
        trials.append({"trial": trial, "elapsed_host_seconds": time.monotonic() - started, "records": records, "raw_directory": str(raw.relative_to(ROOT))})
    cpuinfo = text("/proc/cpuinfo") or ""
    cpu_model = next((line.split(":", 1)[1].strip() for line in cpuinfo.splitlines() if line.startswith("model name")), platform.processor())
    source = ROOT / "src/rust/rt_runtime/examples/bounded_runtime.rs"
    report = {"passed": True, "iterations_per_case": args.iterations, "warmup_callbacks_per_case": 512, "trials": trials, "canaries": canaries,
              "host": {"platform": platform.platform(), "cpu_model": cpu_model, "logical_cpus": os.cpu_count(), "allowed_cpus": available, "affinity_cpu": cpu,
                       "scaling_governor": text(f"/sys/devices/system/cpu/cpu{cpu}/cpufreq/scaling_governor")},
              "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(), "rustflags": env["RUSTFLAGS"],
              "release_profile": tomllib.loads((ROOT / "Cargo.toml").read_text()).get("profile", {}).get("release", {}),
              "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
              "limitations": ["Instant measures elapsed wall time around the guarded core render path, including preemption and guard overhead, not hardware CPU cycles.",
                              "Control submission, output verification, owner destruction, and raw-file I/O are outside measured callbacks.",
                              "No device/OS priority/deadline or JavaScript hard-real-time guarantee; no comparison with the Shoop engine.",
                              "Resource arena sizes are fixed prepared allocations. Used-page peaks are sampled at callback boundaries; VmHWM is cumulative whole-process RSS including measurement buffers.",
                              "Throughput uses both summed measured callback time and simulated audio time; it is not MessagePort/device/network throughput.",
                              "Deadline misses and tail candidates are recorded, never used as brittle CI pass/fail thresholds."]}
    (REPORTS / "result.json").write_text(json.dumps(report, indent=2))
    print(f"reports: {REPORTS.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
