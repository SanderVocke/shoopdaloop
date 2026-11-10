#!/usr/bin/env python3
"""Measure engine callback CPU cost and actual heap operations in release builds."""

import argparse
import csv
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess

from benchmark_audio_chunks import percentile

ROOT = Path(__file__).resolve().parents[1]


def summarize(path):
    with path.open() as source:
        rows = list(csv.DictReader(source))
    assert rows
    times = [int(row["elapsed_ns"]) / 1000 for row in rows]
    growth = [duration for row, duration in zip(rows, times) if int(row["elastic_arena_growths"])]
    reclaim = [duration for row, duration in zip(rows, times) if int(row["elastic_reclaimed_chunks"])]
    heap = [duration for row, duration in zip(rows, times)
            if any(int(row[key]) for key in ("allocations", "reallocations", "deallocations"))]
    return {
        **{key: rows[0][key] for key in ("mode", "phase", "voices")},
        "callbacks": len(rows), "p50_us": statistics.median(times),
        "p99_us": percentile(times, .99), "max_us": max(times),
        "overruns": sum(duration > 128 / 48_000 * 1e6 for duration in times),
        **{key: sum(int(row[key]) for row in rows)
           for key in ("allocations", "reallocations", "deallocations", "elastic_arena_growths", "elastic_reclaimed_chunks")},
        "heap_callbacks": len(heap), "heap_max_us": max(heap, default=0),
        "growth_callbacks": len(growth), "growth_max_us": max(growth, default=0),
        "reclaim_callbacks": len(reclaim), "reclaim_max_us": max(reclaim, default=0),
        "peak_slots": max(int(row["elastic_slots"]) for row in rows),
        "capacity_slots": max(int(row["elastic_capacity_slots"]) for row in rows),
        "final_slots": int(rows[-1]["elastic_slots"]),
        "final_free_slots": int(rows[-1]["elastic_free_slots"]),
        "csv": str(path),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/examples/elastic_cpu_benchmark")
    parser.add_argument("--output", type=Path, default=ROOT / "work/elastic-cpu-benchmark")
    parser.add_argument("--voices", type=int, nargs="+", default=[1, 4, 8])
    parser.add_argument("--phases", nargs="+", choices=["play", "record-play", "record", "clear"],
                        default=["play", "record-play", "record", "clear"])
    parser.add_argument("--callbacks", type=int, default=1024)
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--cpu", type=int)
    args = parser.parse_args()
    if not (1 <= args.callbacks <= 4096 and args.repeats > 0 and all(1 <= n <= 16 for n in args.voices)):
        parser.error("callbacks: 1..4096, voices: 1..16, repeats: positive")
    if args.cpu is not None:
        os.sched_setaffinity(0, {args.cpu})
    args.output.mkdir(parents=True, exist_ok=True)
    metadata = {
        "commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "dirty": bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True)),
        "platform": platform.platform(), "machine": platform.machine(),
        "cpu_model": [line for line in Path("/proc/cpuinfo").read_text().splitlines()
                      if line.startswith("model name")][:1] if Path("/proc/cpuinfo").exists() else [],
        "affinity": sorted(os.sched_getaffinity(0)) if hasattr(os, "sched_getaffinity") else None,
        "quantum": 32, "buffer": 128, "sample_rate": 48_000,
        "callback_budget_us": 128 / 48_000 * 1e6,
        "kind": "synchronous offline engine endpoint; allocator counts are thread-local actual operations",
        "binary": str(args.binary.resolve()),
    }
    (args.output / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")
    results = []
    for voices in args.voices:
        for phase in args.phases:
            for repeat in range(args.repeats):
                # Alternate pair order to reduce a consistent ordering bias.
                modes = ["current", "fpga-reference"][::1 if repeat % 2 == 0 else -1]
                for mode in modes:
                    path = args.output / f"{mode}-{phase}-v{voices}-r{repeat}.csv"
                    with path.open("w") as output:
                        subprocess.run([str(args.binary.resolve()), "--mode", mode, "--voices", str(voices),
                                        "--phase", phase, "--callbacks", str(args.callbacks)],
                                       cwd=ROOT, stdout=output, check=True)
                    result = summarize(path)
                    assert result["callbacks"] == args.callbacks
                    result["repeat"] = repeat
                    results.append(result)
                    print(f"{mode} {phase} voices={voices}: p99={result['p99_us']:.2f}us "
                          f"max={result['max_us']:.2f}us heap callbacks={result['heap_callbacks']}", flush=True)
    (args.output / "summary.json").write_text(json.dumps(results, indent=2) + "\n")
    with (args.output / "summary.csv").open("w") as output:
        writer = csv.DictWriter(output, fieldnames=list(results[0]))
        writer.writeheader()
        writer.writerows(results)


if __name__ == "__main__":
    main()
