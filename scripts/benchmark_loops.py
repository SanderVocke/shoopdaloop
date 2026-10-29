#!/usr/bin/env python3
"""Measure real guarded loop callbacks; correctness, never deadline misses, gates success."""
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
CASES = {
    "no-planner-idle", "loop-idle", "audio-1-257", "audio-4-257", "audio-16-257",
    "audio-4-4", "midi-1-4", "midi-4-4", "cascade-4-1", "nested-regular",
    "seek-replace", "fx-capture", "upload-export", "cascade-bursts",
}
SUMS = {"intervals": "intervals", "nodes": "nodes", "transitions": "transitions",
        "render_bytes": "render_bytes", "maintenance_bytes": "maintenance_bytes",
        "cascade_starts": "cascade_starts"}


def verify_records(stdout, raw, iterations):
    records = [json.loads(line) for line in stdout.splitlines() if line.startswith("{")]
    if len(records) != len(CASES) or {r.get("case") for r in records} != CASES:
        raise RuntimeError("missing or duplicate loop benchmark cases")
    for record in records:
        fixed = {"callbacks": iterations, "frames_per_callback": 128, "sample_rate": 48000,
                 "warmup_callbacks": 128, "rust_heap_operations": 0, "deadline_ns": 2666666,
                 "pending_requests_after_drain": 0}
        if any(type(record.get(k)) is not int or record[k] != v for k, v in fixed.items()):
            raise RuntimeError("malformed callback/guard/configuration evidence")
        with (raw / f"{record['case']}.csv").open() as source:
            rows = [{k: int(v) for k, v in row.items()} for row in csv.DictReader(source)]
        if len(rows) != iterations or [r["callback"] for r in rows] != list(range(iterations)):
            raise RuntimeError("missing, duplicated or reordered raw callbacks")
        if any(any(value < 0 for value in row.values()) for row in rows):
            raise RuntimeError("negative raw callback metric")
        times = sorted(r["nanoseconds"] for r in rows)
        expected = {"p50_ns": times[(iterations-1)*500//1000],
                    "p99_ns": times[(iterations-1)*990//1000],
                    "p999_ns": times[(iterations-1)*999//1000], "max_ns": times[-1],
                    "deadline_misses": sum(t > record["deadline_ns"] for t in times),
                    "pages_at_boundary_max": max(r["pages"] for r in rows)}
        expected.update({out: sum(r[column] for r in rows) for out, column in SUMS.items()})
        if any(type(record.get(k)) is not int or record[k] != v for k, v in expected.items()):
            raise RuntimeError("raw callback evidence differs from summary")
        config = record["configuration"]
        if any(type(v) is not int or v < 0 for v in config.values()):
            raise RuntimeError("malformed prepared configuration")
        if any(r["maintenance_bytes"] > config["transfer_bytes"] or
               r["maintenance_steps"] > config["validation_steps"] or
               r["pages"] > config["resource_pages"] or
               r["reserved_pages"] > config["resource_pages"] or
               r["domain_pending"] > config["domain_credits"] or
               r["domain_pending"] + r["resource_pending"] > config["command_slots"] for r in rows):
            raise RuntimeError("prepared storage/maintenance contract violated")
        if record["case"] == "upload-export" and not (record["uploaded_bytes"] > 0 and record["exported_bytes"] > 0):
            raise RuntimeError("missing simultaneous upload/export progress")
        if record["case"] == "cascade-bursts" and record["cascade_starts"] != iterations * 4:
            raise RuntimeError("missing measured same-frame cascade starts")
        record["credit_usage"] = {"domain_pending_max": max(r["domain_pending"] for r in rows),
                                  "resource_pending_max": max(r["resource_pending"] for r in rows)}
        record["work"] = {"graph_edge_visits": sum(r["edges"] for r in rows),
                          "render_steps": sum(r["render_steps"] for r in rows),
                          "maintenance_steps": sum(r["maintenance_steps"] for r in rows)}
        record["mean_ns"] = sum(times) / iterations
        record["slowest_callbacks"] = heapq.nlargest(5, rows, key=lambda r: r["nanoseconds"])
        record["sampled_storage"] = {"reserved_pages_max": max(r["reserved_pages"] for r in rows),
                                     "free_versions_min": min(r["free_versions"] for r in rows),
                                     "free_snapshots_min": min(r["free_snapshots"] for r in rows)}
    return records


def run(command, log, env, cpu=None):
    affinity = {"preexec_fn": lambda: os.sched_setaffinity(0, {cpu})} if cpu is not None else {}
    try:
        result = subprocess.run(command, cwd=ROOT, env=env, text=True, stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE, timeout=600, **affinity)
    except subprocess.TimeoutExpired as error:
        def text(value):
            return value.decode(errors="replace") if isinstance(value, bytes) else value or ""
        log.write_text(text(error.stdout) + text(error.stderr) + "\nbenchmark command timed out\n")
        raise RuntimeError(f"benchmark command timed out; partial output: {log}") from error
    log.write_text(result.stdout + result.stderr)
    return result


def read_optional(path):
    try:
        return Path(path).read_text().strip()
    except OSError:
        return None


def source_files():
    paths = [ROOT / "Cargo.toml", ROOT / "Cargo.lock", ROOT / ".cargo/config.toml", Path(__file__).resolve()]
    for package in ("rt_runtime", "shoop_loops", "shoop_wasm_test_support"):
        base = ROOT / "src/rust" / package
        paths += [base / "Cargo.toml", *sorted((base / "src").rglob("*.rs"))]
    paths.append(ROOT / "src/rust/shoop_loops/examples/benchmark_loop.rs")
    return {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--iterations", type=int, default=4096)
    parser.add_argument("--trials", type=int, default=3)
    parser.add_argument("--cpu", type=int, help="optional Linux affinity CPU; unpinned by default")
    parser.add_argument("--report-dir", type=Path, default=ROOT / "target/loops-bench")
    args = parser.parse_args()
    if not 1024 <= args.iterations <= 1000000 or not 1 <= args.trials <= 10:
        parser.error("iterations must be 1024..1000000 and trials 1..10")
    allowed = sorted(os.sched_getaffinity(0)) if hasattr(os, "sched_getaffinity") else []
    if args.cpu is not None and args.cpu not in allowed:
        parser.error("requested affinity CPU is unavailable")
    reports = args.report_dir.resolve()
    reports.mkdir(parents=True, exist_ok=False)
    sources = source_files()
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    dirty = bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True).strip())
    env = {**os.environ, "RUSTFLAGS": os.environ.get("RUSTFLAGS", "-D warnings")}
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    if not target.is_absolute():
        target = ROOT / target
    suffix = ".exe" if os.name == "nt" else ""
    canaries = []
    binaries = {}
    for profile in ("debug", "release"):
        command = ["cargo", "build", "--locked", "-p", "shoop_loops", "--example", "benchmark_loop"]
        if profile == "release":
            command.append("--release")
        built = run(command, reports / f"build-{profile}.log", env)
        if built.returncode:
            raise RuntimeError(built.stderr)
        binary = target / profile / "examples" / ("benchmark_loop" + suffix)
        binaries[profile] = hashlib.sha256(binary.read_bytes()).hexdigest()
        for kind in ("allocation", "reallocation", "deallocation"):
            result = run([str(binary), "--fail-canary", kind], reports / f"canary-{profile}-{kind}.log", env)
            if result.returncode == 0 or "heap operation during rendering" not in result.stderr:
                raise RuntimeError(f"{profile} {kind} did not fail through the Rust heap guard")
            canaries.append({"profile": profile, "kind": kind, "exit_code": result.returncode,
                             "specific_heap_failure_observed": True})
    trials = []
    for trial in range(args.trials):
        raw = reports / f"trial-{trial}-raw"
        started = time.monotonic()
        result = run([str(target / "release/examples" / ("benchmark_loop" + suffix)),
                      "--iterations", str(args.iterations), "--raw-dir", str(raw)],
                     reports / f"trial-{trial}.log", env, args.cpu)
        if result.returncode:
            raise RuntimeError(result.stderr)
        records = verify_records(result.stdout, raw, args.iterations)
        for r in records:
            print(f"trial={trial} {r['case']}: p50={r['p50_ns']}ns p99={r['p99_ns']}ns max={r['max_ns']}ns misses={r['deadline_misses']}", flush=True)
        trials.append({"trial": trial, "host_seconds": time.monotonic()-started,
                       "raw_directory": str(raw), "records": records})
    cpuinfo = read_optional("/proc/cpuinfo") or ""
    cpu_model = next((line.split(":", 1)[1].strip() for line in cpuinfo.splitlines()
                      if line.startswith("model name")), platform.processor())
    if source_files() != sources:
        raise RuntimeError("benchmark source changed during measurement")
    if any(hashlib.sha256((target / profile / "examples" / ("benchmark_loop" + suffix)).read_bytes()).hexdigest() != digest
           for profile, digest in binaries.items()):
        raise RuntimeError("benchmark binary changed during measurement")
    report = {"passed": True, "kind": "native-loop-callback-benchmark", "trials": trials,
              "canaries": canaries, "source_head": head, "source_dirty": dirty,
              "source_files": sources, "binary_sha256": binaries,
              "cargo": subprocess.check_output(["cargo", "--version"], text=True).strip(),
              "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(),
              "rustflags": env["RUSTFLAGS"], "release_profile": tomllib.loads((ROOT / "Cargo.toml").read_text()).get("profile", {}).get("release", {}),
              "host": {"platform": platform.platform(), "cpu_model": cpu_model, "logical_cpus": os.cpu_count(),
                       "allowed_cpus": allowed, "affinity_cpu": args.cpu,
                       "scaling_governor": read_optional(f"/sys/devices/system/cpu/cpu{args.cpu}/cpufreq/scaling_governor") if args.cpu is not None else None},
              "limitations": ["Native elapsed wall time includes guard/observer overhead and preemption; no device, browser, cold-cache or deadline guarantee.",
                              "Preparation, control submission, scalar full sample/packet/read checks, reliable owner destruction and raw I/O are outside measured callbacks.",
                              "NoPlanner and loop idle are distinct prepared engines, not a paired statistical speedup experiment.",
                              "Pool metrics are callback-boundary samples, not unsampled peaks. Linux VmHWM is process-lifetime peak RSS including prior cases and off-render work; unavailable elsewhere.",
                              "Credit peaks count submitted domain/resource requests awaiting checked returns before each callback; after every drain both counters must be zero.",
                              "Warmup is 128 full checked callbacks per case; measured commands/bytes exclude warmup. Continuous one-frame wraps and separate repeated same-frame cascade bursts are measured; burst stops at frame127 are test input, not an engine quantum.",
                              "Deadline misses remain in raw/summary output and never determine success. No comparison with the legacy engine."]}
    (reports / "result.json").write_text(json.dumps(report, indent=2)+"\n")
    print(f"reports: {reports}")


if __name__ == "__main__":
    main()
