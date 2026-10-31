#!/usr/bin/env python3
"""Collect one native release process per case/repetition; retain every raw row."""

import argparse
import csv
from datetime import datetime, timezone
import gzip
import hashlib
import io
import json
import math
import os
from pathlib import Path
import platform
import re
import statistics
import subprocess

CASES = [("service", mode) for mode in ("idle", "steady", "preset", "poll", "export")]
CASES += [("current", "idle"), ("current", "steady")]
COLUMNS = (
    "engine,workload,index,frame,callback_ns,control_ns,meter_ns,control_attempts,"
    "refusals,meter_reads,midi_packets,copied_bytes,forwarded,owning_returns,executions"
).split(",")
PROTECTED = ["src/rust/rt_runtime", "src/rust/shoop_loops", "tests/loops_host",
             "PLAN_LOOP_LAYER.md", "docs/loop_layer", "scripts/run_loops_host*"]


def require(condition, message):
    if not condition:
        raise ValueError(message)


def distribution(values):
    ordered = sorted(values)
    require(bool(ordered), "empty distribution")
    require(all(math.isfinite(v) and v >= 0 for v in ordered), "invalid timing")
    return {"n": len(ordered), "median": statistics.median(ordered),
            "p95": ordered[math.ceil(len(ordered) * .95) - 1],
            "p99": ordered[math.ceil(len(ordered) * .99) - 1], "max": ordered[-1]}


def parse_rows(text, engine, mode, samples, warmup):
    reader = csv.DictReader(io.StringIO(text))
    require(reader.fieldnames == COLUMNS, "unexpected CSV schema")
    rows = list(reader)
    require(len(rows) == samples, "wrong raw sample count")
    for index, row in enumerate(rows):
        require(set(row) == set(COLUMNS), f"row {index}: unexpected width")
        require(row["engine"] == engine and row["workload"] == mode, "wrong case")
        for key in COLUMNS[2:]:
            value = row[key]
            require(isinstance(value, str) and value.isascii() and value.isdecimal(),
                    f"row {index}: invalid {key}")
            row[key] = int(value)
        require(row["index"] == index, "non-contiguous sample indices")
        require(row["frame"] == (warmup + index) * 64, "wrong frame progression")
        require(row["callback_ns"] > 0, "zero callback clock")
        attempts = 2 if mode == "preset" else int(mode in ("poll", "export"))
        if mode == "export" and row["frame"] < 128:
            attempts = 0
        require(row["control_attempts"] == attempts, "wrong command attempts")
        require(row["refusals"] == int(mode == "preset"), "wrong refusals")
        require(row["meter_reads"] == (64 if mode == "poll" else 1), "wrong meter work")
        require(row["copied_bytes"] <= 256, "read exceeds declared buffer")
        if mode != "export":
            require(row["copied_bytes"] == 0, "unexpected media transfer")
        if engine == "service":
            packets = 64 if mode in ("steady", "preset", "poll") and row["frame"] % 4096 == 0 else 0
            require(row["midi_packets"] == packets, "wrong canonical restoration work")
    if mode == "export":
        require(sum(row["copied_bytes"] for row in rows) > 0, "no live-export progress")
    return rows


def parse_metadata(text, engine, mode, samples, warmup):
    def token(key):
        found = re.findall(rf"\b{re.escape(key)}=([^\s]+)", text)
        require(len(found) == 1, f"missing/duplicate metadata: {key}")
        return found[0]
    require(token("engine") == engine and token("mode") == mode, "metadata case mismatch")
    require(token("debug") == "false", "debug build is not release evidence")
    for key, value in (("frames", 64), ("loop_frames", 4096), ("sample_rate", 48000),
                       ("samples", samples), ("warmup", warmup)):
        require(token(key) == str(value), f"metadata mismatch: {key}")
    result = {}
    for key in ("clock_median_ns", "elapsed_wall_ns", "rss_constructed_kib",
                "rss_after_warmup_kib", "rss_live_kib", "rss_after_cleanup_kib"):
        value = token(key)
        if key.startswith("rss_"):
            require(value == "None" or re.fullmatch(r"Some\([0-9]+\)", value), "invalid RSS")
            result[key] = None if value == "None" else int(value[5:-1])
        else:
            require(value.isascii() and value.isdecimal(), f"invalid {key}")
            result[key] = int(value)
    require(result["elapsed_wall_ns"] > 0, "zero wall clock")
    return result


def summarize(rows, metadata):
    copied = sum(row["copied_bytes"] for row in rows)
    wall = sum(item["elapsed_wall_ns"] for item in metadata)
    return {
        "timing_ns": {key: distribution([row[key] for row in rows])
                      for key in ("callback_ns", "control_ns", "meter_ns")},
        "meter_batch_mean_per_read_ns": distribution([
            row["meter_ns"] / row["meter_reads"] for row in rows]),
        "control_attempts": sum(row["control_attempts"] for row in rows),
        "refusals": sum(row["refusals"] for row in rows),
        "midi_packets": sum(row["midi_packets"] for row in rows),
        "copied_bytes": copied,
        "forwarded": sum(row["forwarded"] for row in rows),
        "owning_returns": sum(row["owning_returns"] for row in rows),
        "executions": sum(row["executions"] for row in rows),
        "elapsed_wall_ns": wall,
        "unpaced_host_copy_bytes_per_second": copied * 1e9 / wall,
        "nominal_48khz_copy_bytes_per_second": copied * 48000 / (len(rows) * 64),
        "rss_kib": {key: [item[key] for item in metadata] for key in metadata[0]
                    if key.startswith("rss_")},
        "clock_median_ns": [item["clock_median_ns"] for item in metadata],
    }


def probe(command, root):
    try:
        result = subprocess.run(command, cwd=root, capture_output=True, text=True, check=False, timeout=10)
        return {"command": command, "exit": result.returncode,
                "stdout": result.stdout, "stderr": result.stderr}
    except (OSError, subprocess.TimeoutExpired) as error:
        return {"command": command, "unavailable": str(error)}


def git(root, *args):
    return subprocess.check_output(["git", *args], cwd=root, text=True).strip()


def read_optional(path):
    try:
        return Path(path).read_text(errors="replace").strip()
    except OSError:
        return None


def digest(data):
    return hashlib.sha256(data).hexdigest()


def affinity():
    return sorted(os.sched_getaffinity(0)) if hasattr(os, "sched_getaffinity") else None


def environment(root, cpu):
    governor_cpu = cpu if cpu is not None else (affinity() or [0])[0]
    return {
        "platform": platform.platform(), "affinity": affinity(),
        "loadavg": read_optional("/proc/loadavg"),
        "cpu_max": read_optional("/sys/fs/cgroup/cpu.max"),
        "clocksource": read_optional("/sys/devices/system/clocksource/clocksource0/current_clocksource"),
        "governor": read_optional(f"/sys/devices/system/cpu/cpu{governor_cpu}/cpufreq/scaling_governor"),
        "tools": [probe(command, root) for command in (
            ["rustc", "-Vv"], ["cargo", "-V"], ["lscpu", "--json"],
            ["ps", "-eo", "pid,comm,pcpu,pmem", "--sort=-pcpu"])],
        "build_environment": {key: os.environ.get(key) for key in (
            "RUSTFLAGS", "CARGO_BUILD_JOBS", "CARGO_PROFILE_RELEASE_OPT_LEVEL",
            "CARGO_PROFILE_RELEASE_LTO", "CARGO_PROFILE_RELEASE_CODEGEN_UNITS")},
    }


def collect(args):
    here = Path(__file__).resolve()
    root = Path(git(here.parent, "rev-parse", "--show-toplevel"))
    status = git(root, "status", "--porcelain", "--untracked-files=all")
    require(args.allow_dirty or not status, "source is dirty; commit before final collection")
    dependency = git(root, "rev-parse", f"{args.dependency_revision}^{{commit}}")
    subprocess.run(["git", "merge-base", "--is-ancestor", dependency, "HEAD"], cwd=root, check=True)
    require(not git(root, "diff", "--name-only", dependency, "--", *PROTECTED),
            "protected source differs from declared dependency")
    binary = Path(args.binary).resolve()
    require(binary.is_file(), "build the native release example first")
    output = Path(args.output).resolve()
    output.mkdir(parents=True, exist_ok=False)
    original_affinity = affinity()
    if args.cpu is not None:
        require(original_affinity is not None and args.cpu in original_affinity, "CPU is not allowed")
    tracked_before = subprocess.check_output(["git", "diff", "HEAD", "--binary", "--"], cwd=root)
    manifest = {
        "schema": 1, "complete": False, "started_utc": datetime.now(timezone.utc).isoformat(),
        "source": {"head": git(root, "rev-parse", "HEAD"), "dependency": dependency,
                   "branch": git(root, "branch", "--show-current"), "status_before": status,
                   "tracked_diff_sha256": digest(tracked_before), "measurement_sources": {}},
        "binary": {"path": str(binary), "sha256": digest(binary.read_bytes())},
        "configuration": {"samples": args.samples, "warmup": args.warmup,
                          "repeats": args.repeats, "cpu": args.cpu,
                          "frames": 64, "nominal_rate": 48000, "loop_frames": 4096,
                          "host_timeout_seconds": 60,
                          "percentiles": "nearest-rank; no samples removed; no clock subtraction"},
        "affinity_before": original_affinity, "runs": [],
    }
    sources = [here, *here.parent.glob("*.rs"), here.parent.parent / "measure_service.rs",
               root / "Cargo.toml", root / "Cargo.lock"]
    for path in sources:
        manifest["source"]["measurement_sources"][str(path.relative_to(root))] = digest(path.read_bytes())
    grouped = {case: [] for case in CASES}
    try:
        if args.cpu is not None:
            os.sched_setaffinity(0, {args.cpu})
        manifest["environment_before"] = environment(root, args.cpu)
        for repetition in range(args.repeats):
            # Rotate case order, retaining every process and sample, to expose drift.
            order = CASES[repetition % len(CASES):] + CASES[:repetition % len(CASES)]
            for engine, mode in order:
                stem = f"{engine}-{mode}-r{repetition + 1}"
                command = [str(binary), engine, mode, str(args.samples), str(args.warmup)]
                try:
                    run = subprocess.run(command, cwd=root, capture_output=True, check=False, timeout=60)
                except subprocess.TimeoutExpired as error:
                    run = subprocess.CompletedProcess(command, -1, error.stdout or b"", error.stderr or b"")
                raw_path = output / f"{stem}.csv.gz"
                meta_path = output / f"{stem}.meta.txt"
                raw_path.write_bytes(gzip.compress(run.stdout, mtime=0))
                meta_path.write_bytes(run.stderr)
                record = {"engine": engine, "workload": mode, "repetition": repetition + 1,
                          "command": command, "exit": run.returncode,
                          "raw": raw_path.name, "raw_sha256": digest(raw_path.read_bytes()),
                          "metadata": meta_path.name, "metadata_sha256": digest(run.stderr)}
                manifest["runs"].append(record)
                require(run.returncode == 0, f"{stem}: host failed; raw/stderr retained")
                rows = parse_rows(run.stdout.decode(), engine, mode, args.samples, args.warmup)
                metadata = parse_metadata(run.stderr.decode(), engine, mode, args.samples, args.warmup)
                record["observations"] = metadata
                record["summary"] = summarize(rows, [metadata])
                grouped[(engine, mode)].append((rows, metadata))
        manifest["environment_after"] = environment(root, args.cpu)
        manifest["source"]["head_after"] = git(root, "rev-parse", "HEAD")
        require(manifest["source"]["head_after"] == manifest["source"]["head"], "source HEAD changed")
        require(subprocess.check_output(["git", "diff", "HEAD", "--binary", "--"], cwd=root)
                == tracked_before, "tracked source changed during collection")
        require(digest(binary.read_bytes()) == manifest["binary"]["sha256"], "binary changed")
        for name, sha in manifest["source"]["measurement_sources"].items():
            require(digest((root / name).read_bytes()) == sha, f"source changed: {name}")
        summary = {}
        for (engine, mode), values in grouped.items():
            summary[f"{engine}/{mode}"] = summarize(
                [row for rows, _ in values for row in rows], [meta for _, meta in values])
        (output / "summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
        manifest["summary_sha256"] = digest((output / "summary.json").read_bytes())
        manifest["complete"] = True
    except BaseException as error:
        manifest["error"] = f"{type(error).__name__}: {error}"
        raise
    finally:
        try:
            manifest["finished_utc"] = datetime.now(timezone.utc).isoformat()
            (output / "manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
        finally:
            if args.cpu is not None:
                os.sched_setaffinity(0, original_affinity)
    print(f"Retained {args.repeats * len(CASES) * args.samples} raw rows in {output}")


def audit_bundle(output, allow_dirty=False):
    output = Path(output)
    manifest = json.loads((output / "manifest.json").read_text())
    require(manifest["schema"] == 1 and manifest["complete"], "incomplete bundle")
    require(allow_dirty or not manifest["source"]["status_before"], "dirty-source evidence")
    require(manifest["source"]["head"] == manifest["source"]["head_after"], "source changed")
    config = manifest["configuration"]
    samples, warmup, repeats = (config[key] for key in ("samples", "warmup", "repeats"))
    require(16 <= samples <= 2048 and 1 <= warmup <= 512 and repeats >= 1, "invalid bounds")
    require((config["frames"], config["nominal_rate"], config["loop_frames"]) == (64, 48000, 4096),
            "unexpected workload configuration")
    require(len(manifest["runs"]) == repeats * len(CASES), "missing run")
    seen = set()
    grouped = {case: [] for case in CASES}
    for run in manifest["runs"]:
        engine, mode, repetition = (run[key] for key in ("engine", "workload", "repetition"))
        require((engine, mode) in CASES and 1 <= repetition <= repeats, "unexpected run")
        identity = (engine, mode, repetition)
        require(identity not in seen and run["exit"] == 0, "duplicate/failed run")
        seen.add(identity)
        stem = f"{engine}-{mode}-r{repetition}"
        require(run["raw"] == f"{stem}.csv.gz" and run["metadata"] == f"{stem}.meta.txt",
                "unexpected artifact path")
        raw, meta = (output / run[key] for key in ("raw", "metadata"))
        require(digest(raw.read_bytes()) == run["raw_sha256"], "raw hash mismatch")
        require(digest(meta.read_bytes()) == run["metadata_sha256"], "metadata hash mismatch")
        rows = parse_rows(gzip.decompress(raw.read_bytes()).decode(), engine, mode, samples, warmup)
        observations = parse_metadata(meta.read_text(), engine, mode, samples, warmup)
        require(observations == run["observations"], "metadata observations mismatch")
        require(summarize(rows, [observations]) == run["summary"], "per-run summary mismatch")
        require(run["command"][-4:] == [engine, mode, str(samples), str(warmup)], "wrong command")
        grouped[(engine, mode)].append((repetition, rows, observations))
    expected = {}
    for (engine, mode), values in grouped.items():
        values.sort(key=lambda value: value[0])
        expected[f"{engine}/{mode}"] = summarize(
            [row for _, rows, _ in values for row in rows], [meta for _, _, meta in values])
    summary_bytes = (output / "summary.json").read_bytes()
    require(digest(summary_bytes) == manifest["summary_sha256"], "summary hash mismatch")
    require(json.loads(summary_bytes) == expected, "pooled summary mismatch")
    return expected


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output")
    parser.add_argument("--audit", action="store_true", help="recompute and verify an existing bundle")
    parser.add_argument("--binary", default="target/release/examples/measure_service")
    parser.add_argument("--dependency-revision")
    parser.add_argument("--samples", type=int, default=2048)
    parser.add_argument("--warmup", type=int, default=128)
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--cpu", type=int)
    parser.add_argument("--allow-dirty", action="store_true", help="validation only; recorded in manifest")
    args = parser.parse_args()
    if args.audit:
        audit_bundle(args.output, args.allow_dirty)
        print(f"Raw hashes, row/oracle checks and recomputed summaries: ok ({args.output})")
    else:
        require(args.dependency_revision, "--dependency-revision is required for collection")
        require(16 <= args.samples <= 2048 and 1 <= args.warmup <= 512 and args.repeats >= 1,
                "invalid capture/storage bounds")
        collect(args)


if __name__ == "__main__":
    main()
