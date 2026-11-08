#!/usr/bin/env python3
"""Validate the targeted Pi scheduler, PMU and sampled-stack evidence archive."""

import argparse
from collections import Counter
import csv
import gzip
import hashlib
import json
import math
import re
import statistics
from pathlib import Path
from benchmark_audio_chunks import summarize
from analyze_audio_chunk_profiles import CATEGORIES, category


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    root = args.input.resolve()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    checks = {}
    for line in (root / "TARGETED_SHA256SUMS").read_text().splitlines():
        digest, name = line.split("  ", 1)
        assert not Path(name).is_absolute() and ".." not in Path(name).parts
        assert hashlib.sha256((root / name).read_bytes()).hexdigest() == digest, name
        checks[name] = digest
    checks["TARGETED_SHA256SUMS"] = hashlib.sha256(
        (root / "TARGETED_SHA256SUMS").read_bytes()
    ).hexdigest()
    assert set(checks) == {
        str(p.relative_to(root)) for p in root.rglob("*") if p.is_file()
    }
    build = root / "targeted-profile-analysis/BUILD_INFO.json"
    assert (
        hashlib.sha256(build.read_bytes()).hexdigest()
        == "c5e089239727eab3dc8419432db211b2ecfb7b680a4a299c6e066aec3548e482"
    )
    run_results = []
    boundaries = []
    for p in sorted((root / "targeted-profiles").glob("*/*/*/run.json")):
        replay = summarize(p.parent, write=False)
        saved = json.loads(p.with_name("summary.json").read_text())
        assert {k: v for k, v in replay.items() if k != "directory"} == {
            k: v for k, v in saved.items() if k != "directory"
        }, p
        meta = json.loads(p.with_name("profile.json").read_text())
        s = meta["scheduling"]
        assert (
            s["render_affinity"] == [1]
            and s["render_scheduler"] == 1
            and s["render_priority"] == 80
        ), p
        assert meta["validation"]["dropped"] == 0
        run_results.append(
            {
                **replay,
                "directory": str(p.parent.relative_to(root)),
                "kind": meta["kind"],
                "case": p.parent.parent.name,
            }
        )
        if p.parent.parent.parent.name == "dwarf" and meta["scenario"] == "shared":
            run = json.loads(p.read_text())
            track = next(t for t in run["setup"]["tracks"] if t["name"] == "Benchmark")
            length = track["loops"][0]["length_frames"]
            with p.with_name("callbacks.csv").open() as f:
                rows = [
                    {k: int(v) for k, v in row.items()}
                    for row in csv.DictReader(
                        line for line in f if not line.startswith("#")
                    )
                ]
            selected = [
                r for r in rows if run["first_frame"] <= r["frame"] < run["last_frame"]
            ]
            peaks = [r for r in selected if r["elapsed_ns"] > 5_000_000]
            residue, count = Counter(r["frame"] % length for r in peaks).most_common(1)[
                0
            ]
            dominant = [r for r in peaks if r["frame"] % length == residue]
            whole_dominant = [r for r in rows if r["frame"] % length == residue]
            whole_max = max(whole_dominant, key=lambda r: r["elapsed_ns"])
            boundaries.append(
                {
                    "mode": meta["mode"],
                    "loop_frames": length,
                    "selected_callbacks": len(selected),
                    "selected_spikes_over_5ms": len(peaks),
                    "selected_dominant_residue": residue,
                    "selected_dominant_count": count,
                    "selected_boundary_median_ms": statistics.median(
                        r["elapsed_ns"] for r in dominant
                    )
                    / 1e6,
                    "selected_max_ms": max(r["elapsed_ns"] for r in selected) / 1e6,
                    "whole_csv_boundary_max_ms": whole_max["elapsed_ns"] / 1e6,
                    "whole_csv_boundary_max_inside_selected": run["first_frame"]
                    <= whole_max["frame"]
                    < run["last_frame"],
                }
            )
    assert len(run_results) == 21
    pmu = []
    provided = json.loads(
        (root / "targeted-profile-analysis/pmu-summary.json").read_text()
    )
    for p in sorted((root / "targeted-profiles/stat").glob("*/*/perf-stat.csv")):
        meta = json.loads(p.with_name("profile.json").read_text())
        counts = {}
        runtimes = {}
        with p.open() as f:
            for row in csv.reader(
                line for line in f if line.strip() and not line.startswith("#")
            ):
                assert float(row[4]) == 100.0, (p, row)
                counts[row[2]] = int(row[0])
                runtimes[row[2]] = int(row[3])
        nominal = meta["duration_s"] * 48000
        r = {
            "case": p.parent.parent.name,
            "mode": meta["mode"],
            "frames_nominal": nominal,
            "cycles_per_frame": counts["cycles"] / nominal,
            "instructions_per_frame": counts["instructions"] / nominal,
            "ipc": counts["instructions"] / counts["cycles"],
            "l1d_refills_per_frame": counts["armv8_cortex_a72/l1d_cache_refill/"]
            / nominal,
            "l2d_refills_per_frame": counts["armv8_cortex_a72/l2d_cache_refill/"]
            / nominal,
            "l2_refills_per_1000_instructions": 1000
            * counts["armv8_cortex_a72/l2d_cache_refill/"]
            / counts["instructions"],
            "raw_counts": counts,
            "counter_runtime_ns": runtimes,
            "duration_s": meta["duration_s"],
            "normalization": "Nominal 48000*duration_s frames; exact PMU-window processed-frame count not captured",
        }
        old = next(
            x for x in provided if (x["case"], x["mode"]) == (r["case"], r["mode"])
        )
        for key, value in old.items():
            if isinstance(value, (int, float)):
                assert math.isclose(r[key], value, rel_tol=1e-12, abs_tol=1e-9), (
                    p,
                    key,
                )
        pmu.append(r)
    scheduler = []
    for p in sorted(
        (root / "targeted-profiles/sched/play-b32-memory").glob("*/sched-script.txt")
    ):
        meta = json.loads(p.with_name("profile.json").read_text())
        tid = meta["render_tid"]
        pending = None
        pauses = []
        for line in p.read_text().splitlines():
            m = re.search(
                r"\s([\d.]+):\s+sched:sched_switch:.*prev_pid=(\d+) .*prev_state=(\S+) ==> .*next_pid=(\d+) ",
                line,
            )
            if not m:
                continue
            t, prev, state, nxt = float(m[1]), int(m[2]), m[3], int(m[4])
            if prev == tid:
                pending = (t, state)
            if nxt == tid and pending:
                dt = (t - pending[0]) * 1000
                if pending[1] == "R" and dt >= 40:
                    pauses.append(
                        {
                            "switch_out": pending[0],
                            "switch_in": t,
                            "delay_ms": dt,
                            "state": "R",
                        }
                    )
                pending = None
        gaps = [
            pauses[i]["switch_in"] - pauses[i - 1]["switch_in"]
            for i in range(1, len(pauses))
        ]
        assert pauses and all(abs(g - 1) < 0.001 for g in gaps), (p, gaps)
        scheduler.append(
            {
                "mode": meta["mode"],
                "render_tid": tid,
                "pauses": pauses,
                "resume_intervals_s": gaps,
            }
        )
    profiles = []
    for p in sorted(
        (root / "targeted-profile-analysis/dwarf").glob("*/*/stacks.jsonl.gz")
    ):
        saved = json.loads(p.with_name("summary.json").read_text())
        weights = Counter()
        with gzip.open(p, "rt") as f:
            stacks = [json.loads(line) for line in f]
        assert sum(r["count"] for r in stacks) == saved["samples"]
        assert sum(r["period_ns"] for r in stacks) == saved["total_period_ns"]
        assert saved["lost_samples"] == 0
        for r in stacks:
            group = next(
                (g for frame in r["frames_leaf_first"] if (g := category(frame))),
                CATEGORIES[-1],
            )
            weights[group] += r["period_ns"]
        for key, value in saved["category_percent"].items():
            assert abs(100 * weights[key] / saved["total_period_ns"] - value) < 1e-9, (
                p,
                key,
            )
        profiles.append({**saved, "case": p.parent.parent.name})
    settings = json.loads((root / "targeted-profiles/settings.json").read_text())
    assert (
        int(settings["sched_rt_runtime_us"]) == 950000
        and int(settings["sched_rt_period_us"]) == 1000000
    )
    assert all(
        x["cpu.rt_runtime_us"] is None and x["cpu.rt_period_us"] is None
        for x in settings["cgroup_ancestors"]
    )
    summary = {
        "member_hashes_verified": len(checks),
        "runs_verified": len(run_results),
        "callbacks_verified": sum(r["callbacks"] for r in run_results),
        "samples_verified": sum(r["samples"] for r in profiles),
        "lost_samples": sum(r["lost_samples"] for r in profiles),
        "runnable_pauses_over_40ms": sum(len(r["pauses"]) for r in scheduler),
        "settings": settings,
        "normalization_limit": "PMU cycles/frame and refills/frame use nominal audio time; IPC and refill/instruction ratios do not depend on that denominator. One PMU capture per mode/case; profiling is separate from timing.",
    }
    for name, value in [
        ("checksums.json", checks),
        ("validation.json", summary),
        ("pmu.json", pmu),
        ("scheduler.json", scheduler),
        ("profiles.json", profiles),
        ("runs.json", run_results),
        ("boundaries.json", boundaries),
    ]:
        (output / name).write_text(json.dumps(value, indent=2) + "\n")
    with (output / "pmu.csv").open("w") as f:
        columns = [
            "case",
            "mode",
            "frames_nominal",
            "cycles_per_frame",
            "instructions_per_frame",
            "ipc",
            "l1d_refills_per_frame",
            "l2d_refills_per_frame",
            "l2_refills_per_1000_instructions",
        ]
        w = csv.DictWriter(f, columns, extrasaction="ignore", lineterminator="\n")
        w.writeheader()
        w.writerows(pmu)
    print(
        json.dumps(
            {k: v for k, v in summary.items() if k not in ("settings",)}, indent=2
        )
    )


if __name__ == "__main__":
    main()
