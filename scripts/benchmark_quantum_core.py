#!/usr/bin/env python3
"""Run checked fixed-core callbacks; retain tails and every deadline miss."""
import argparse
import csv
import hashlib
import json
import os
from pathlib import Path
import random
import statistics
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--iterations", type=int, default=8192)
    p.add_argument("--trials", type=int, default=5)
    p.add_argument("--cpu", type=int, default=2)
    a = p.parse_args()
    assert 1024 <= a.iterations <= 1000000 and 1 <= a.trials <= 10
    assert a.cpu in os.sched_getaffinity(0)
    out = a.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    if not target.is_absolute():
        target = ROOT / target
    binary = target / "release/examples/quantum_benchmark"
    binary_sha = digest(binary)
    paths = [ROOT / "Cargo.toml", ROOT / "Cargo.lock", Path(__file__).resolve()]
    for crate in ["rt_runtime", "shoop_loops"]:
        base = ROOT / "src/rust" / crate
        paths += [base / "Cargo.toml", *sorted((base / "src").rglob("*.rs"))]
    paths.append(ROOT / "src/rust/shoop_loops/examples/quantum_benchmark.rs")
    sources = {str(path.relative_to(ROOT)): digest(path) for path in paths}
    records = []
    started = time.monotonic()
    for trial in range(a.trials):
        cases = [(q, n, s) for q in [16, 32] for n in [1, 4, 16, 64]
                 for s in ["steady", "two-seeks", "two-bursts"]]
        random.Random(20261103 + trial).shuffle(cases)
        for q, n, scenario in cases:
            stem = f"trial-{trial}-q{q}-n{n}-{scenario}"
            raw = out / (stem + ".csv")
            command = [str(binary), str(q), str(n), scenario, str(a.iterations), str(raw)]
            r = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=120,
                               preexec_fn=lambda: os.sched_setaffinity(0, {a.cpu}))
            (out / (stem + ".log")).write_text(r.stdout + r.stderr)
            if r.returncode:
                raise RuntimeError(r.stdout + r.stderr)
            with raw.open() as f:
                rows = [{k: int(v) for k, v in row.items()} for row in csv.DictReader(f)]
            assert len(rows) == a.iterations and [v["callback"] for v in rows] == list(range(a.iterations))
            times = sorted(v["nanoseconds"] for v in rows)
            mean = statistics.mean(times)
            records.append({"trial": trial, "quantum": q, "loops": n, "scenario": scenario,
                            "callbacks": a.iterations, "mean_ns": mean,
                            "budget_percent": mean * 48000 / 128 / 10000000,
                            "p99_ns": times[(len(times)-1)*990//1000],
                            "p999_ns": times[(len(times)-1)*999//1000], "max_ns": times[-1],
                            "deadline_misses": sum(t * 48000 > 128000000000 for t in times),
                            "raw": raw.name, "raw_sha256": digest(raw)})
        print(f"trial={trial} all 24 configurations checked", flush=True)
    assert digest(binary) == binary_sha and sources == {str(path.relative_to(ROOT)): digest(path) for path in paths}
    result = {"kind": "fixed-core-full-callback", "source_head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
              "source_sha256": sources, "binary_sha256": binary_sha, "sample_rate": 48000,
              "frames_per_callback": 128, "warmup_callbacks": 1024, "trials": a.trials,
              "cpu": a.cpu, "allowed_cpus": sorted(os.sched_getaffinity(0)),
              "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(),
              "rustflags": os.environ.get("RUSTFLAGS"), "elapsed_seconds": time.monotonic()-started,
              "records": records,
              "limitations": ["Elapsed wall time includes preemption and the real heap guard; not device acceptance or a worst-case execution bound.",
                              "Preparation, command submission, scalar verification, result draining, cleanup and raw I/O are outside timing.",
                              "Native synthetic playback with warm immutable media and cheap FX; recording, MIDI and live graph/plugin performance are not covered.",
                              "This is the fixed implementation, not a paired measurement of the previous interval executor; historical comparisons include different runs and harnesses."]}
    (out / "results.json").write_text(json.dumps(result, indent=2))
    with (out / "summary.csv").open("w") as f:
        writer = csv.DictWriter(f, fieldnames=list(records[0]))
        writer.writeheader()
        writer.writerows(records)
    print(f"Results: {out}")


if __name__ == "__main__":
    main()
