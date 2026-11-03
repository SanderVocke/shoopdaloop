#!/usr/bin/env python3
"""Publish and audit a local DSP reproduction, with like-for-like cloud tables."""
from __future__ import annotations

import argparse
from collections import Counter
import gzip
import hashlib
import html
import json
import math
from pathlib import Path
import shutil

import sweep_remote_dsp as sweep
import tune_remote_dsp as tune

ROOT = sweep.ROOT
CAMPAIGNS = ["sweep", "tuning", "matched-tuning", "matched-sweep"]


def read_archive(path):
    with gzip.open(path, "rt") as handle:
        return [json.loads(line) for line in handle if line.strip()]


def config(row):
    return tuple(row["requested"][key] for key in ["P", "k", "N"])


def check_distribution(distribution):
    count = distribution["count"]
    histogram = distribution["histogram_us"]
    assert sum(n for _, n in histogram) == count
    assert histogram == sorted(histogram) and len({b for b, _ in histogram}) == len(histogram)
    assert all(b >= 0 and n > 0 for b, n in histogram)
    quantiles = [("p50_ns", 0.5), ("p95_ns", 0.95), ("p99_ns", 0.99), ("p999_ns", 0.999)]
    if not count:
        assert all(distribution[key] is None for key in ["mean_ns", "max_ns", *(key for key, _ in quantiles)])
        return
    previous = 0
    for key, fraction in quantiles:
        rank = math.ceil(count * fraction)
        cumulative = 0
        for bucket, n in histogram:
            cumulative += n
            if cumulative >= rank:
                assert distribution[key] // 1000 == bucket
                break
        assert previous <= distribution[key] <= distribution["max_ns"]
        previous = distribution[key]
    assert distribution["max_ns"] // 1000 == histogram[-1][0]
    lower = sum(bucket * 1000 * n for bucket, n in histogram) / count
    assert lower <= distribution["mean_ns"] < lower + 1000


def audit(data, archive):
    assert len({row["case"] for row in data}) == len(data)
    for row in data:
        assert row["status"] in ["measured", "rejected"], (row["case"], row.get("error"))
        assert not row.get("client_exit_code", 0) and not row.get("server_exit_code", 0)
        if row["status"] == "rejected":
            assert row.get("reason")
            continue
        p, k, n = config(row)
        assert row["config"]["packet_frames"] == p and row["config"]["multiplier"] == k
        assert row["config"]["allowance"] == n
        assert row["measured_packets"] == row["on_time"] + row["late"] + row["missing"]
        assert math.isclose(row["deadline_miss_fraction"], (row["late"] + row["missing"]) / row["measured_packets"])
        assert row["verified_samples"] + row["unverified_samples"] == (row["on_time"] + row["late"]) * p * row["config"]["channels"]
        assert row["correctness_errors"] == 0
        assert row["sender_lateness"]["count"] == row["sent"]
        assert row["rtt"]["count"] <= min(row["sent"], row["on_time"] + row["late"])
        assert row["batch_turnaround"]["count"] <= row["dsp_processing"]["count"] <= row["measured_packets"] // k
        for name in ["rtt", "sender_lateness", "batch_turnaround", "dsp_processing"]:
            check_distribution(row[name])
        if not row["server"]["diagnostics_truncated"]:
            assert sum(mask.bit_count() for _, mask in row["concealment"]) == row["server"]["concealed_packets"]
        assert len({base for base, _ in row["concealment"]}) == len(row["concealment"])
        assert all(0 < mask < (1 << k) for _, mask in row["concealment"])
        if "resources" in row:
            assert all(value >= 0 for key, value in row["resources"].items() if key != "cgroup_delta")
            assert all(value >= 0 for value in row["resources"]["cgroup_delta"].values())
    measured = [r for r in data if r["status"] == "measured"]
    return {"records": len(data), "by_suite": dict(Counter(r["suite"] for r in data)),
            "by_status": dict(Counter(r["status"] for r in data)),
            "fully_healthy": sum(sweep.healthy(r) for r in measured),
            **{key: sum(r.get(key, 0) for r in measured) for key in
               ["measured_packets", "verified_samples", "unverified_samples", "correctness_errors", "late", "missing", "send_errors_or_skipped"]},
            "concealed_packets": sum(r["server"]["concealed_packets"] for r in measured),
            "retired_runs": sum(r["server"]["retired"] for r in measured),
            "decompressed_sha256": hashlib.sha256(gzip.decompress(archive.read_bytes())).hexdigest(),
            "checks": ["unique cases", "status and process exits", "packet outcome sums and fractions",
                       "independent sample counts", "histogram counts, means, quantile bins and maxima",
                       "bounded concealment masks", "nonnegative resource deltas"]}


def check_coverage(groups):
    baseline = [r for r in groups["sweep"] if r["suite"] == "baseline"]
    expected = {(p, k, n, repeat) for p in [8, 16, 32, 64, 128] for k in [1, 2, 4, 8]
                for n in [1, 2, 4, 8, 16] for repeat in range(3 if n >= k else 1)}
    assert {(*config(r), r["repetition"]) for r in baseline} == expected
    assert all((r["status"] == "rejected") == (config(r)[2] < config(r)[1]) for r in baseline)
    assert len([r for r in groups["sweep"] if r["suite"] == "delay"]) == 40
    assert len([r for r in groups["sweep"] if r["suite"] == "mtu"]) == 2
    assert len([r for r in groups["sweep"] if r["suite"] == "followup"]) == 6
    screen = [r for r in groups["tuning"] if r["suite"] == "screen"]
    manifest = json.loads((groups["root"] / "tuning/manifest.json").read_text())
    expected = {(c, m, rep) for c in tune.CONFIGS for m in tune.modes(manifest) for rep in range(3)}
    assert {(config(r), r["mode"], r["repetition"]) for r in screen} == expected
    assert all(r["status"] == "measured" for r in screen)
    followups = [r for r in groups["tuning"] + groups["matched-tuning"] if r["suite"] == "followup"]
    for c in tune.CONFIGS:
        for mode in ["yield-free", "yield-pinned", "yield-pinned-fifo1", "yield-pinned-rr1"]:
            if mode in tune.modes(manifest):
                assert len([r for r in followups if config(r) == c and r["mode"] == mode]) == 1
    selection = json.loads((groups["root"] / "tuning/selection.json").read_text())
    assert selection == json.loads(json.dumps(tune.select(screen, manifest)))
    for row in groups["tuning"] + groups["matched-tuning"]:
        client, server, pinned, gap = tune.modes(manifest)[row["mode"]]
        assert row["polling"] == client and row["config"]["gap_timeout_us"] == gap
        assert row["client_affinity"] == ([manifest["client_cpu"]] if pinned else manifest["allowed_cpus"])
        assert row["server_affinity"] == ([manifest["server_cpu"]] if pinned else manifest["allowed_cpus"])
    observations = []
    for path in sorted((groups["root"] / "matched-tuning/scheduling").glob("*.json")):
        data = json.loads(path.read_text())
        mode = next(r["mode"] for r in groups["matched-tuning"] if r["case"] == path.stem)
        expected_policy = 1 if mode.endswith("fifo1") else 2 if mode.endswith("rr1") else 0
        assert len(data["processes"]) == 2
        for proc in data["processes"]:
            role = "client" if "client" in proc["command"] else "server"
            for thread in proc["threads"]:
                assert thread["policy"] == expected_policy and thread["priority"] == (1 if expected_policy else 0)
                assert thread["nice"] == 0
                pinned = tune.modes(manifest)[mode][2]
                assert thread["affinity"] == ([manifest[f"{role}_cpu"]] if pinned else manifest["allowed_cpus"])
        observations.append(path.name)
    return {"complete_baseline_matrix": True, "complete_delay_mtu_followup_coverage": True,
            "complete_tuning_screen": True, "selection_recomputed_identically": True,
            "complete_fixed_priority_followups": True, "affinity_polling_gap_verified": True,
            "live_process_thread_scheduling_verified": observations}


def find_long(data, c, mode=None):
    matches = [r for r in data if r["suite"] == "followup" and config(r) == c
               and (mode is None or r.get("mode") == mode)]
    assert len(matches) == 1, (c, mode, [r["case"] for r in matches])
    return matches[0]


def outcomes(row):
    return f"{row['late'] + row['missing']:,} / {row['server']['concealed_packets']:,}"


def timing_ms(p, k, n, delay=0, sample_rate=48000):
    return {"packetization": p * 1000 / sample_rate,
            "aggregation": (k - 1) * p * 1000 / sample_rate,
            "allowance": n * p * 1000 / sample_rate,
            "audio_latency": ((n + 1) * p + delay) * 1000 / sample_rate}


def tables(groups, cloud_sweep, cloud_tuning):
    lines = ["# Reproduced DSP comparison tables", "",
             "Output misses = late + missing measured output slots. Concealments cover the whole stream, including warm-up. "
             "These are different defects and their counts must not be conflated.", "",
             "## Calculated digital audio latency", "",
             "At 48 kHz, packetization is P/fs, the allowance after packet readiness is N*P/fs, and modeled source-to-output "
             "audio latency is ((N+1)*P+A)/fs. All fixed-comparison rows use A=0. Aggregation (k-1)*P/fs and "
             "network/DSP turnaround must fit inside the allowance; do not add them again. This assumes sample-paced "
             "playback beginning at each packet's consumption deadline. It is not measured analogue input-to-output latency: "
             "ADC/DAC, audio-device/callback and additional endpoint buffers are not included. Earlier reply arrival does not "
             "reduce the fixed presentation delay.", "",
             "## Complete campaign totals", "",
             "| Campaign | Records | Measured slots | Verified samples | Output misses | Concealed input | Retired | Healthy measured runs |",
             "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |"]
    for name, data in [("Cloud sweep", cloud_sweep), ("Local sweep", groups["sweep"]),
                       ("Cloud tuning", cloud_tuning), ("Local tuning", groups["tuning"]),
                       ("Local matched tuning additions", groups["matched-tuning"]),
                       ("Local matched sweep additions", groups["matched-sweep"])]:
        lines.append(f"| {name} | {len(data)} | {sum(r.get('measured_packets',0) for r in data):,} | "
                     f"{sum(r.get('verified_samples',0) for r in data):,} | {sum(r.get('late',0)+r.get('missing',0) for r in data):,} | "
                     f"{sum(r.get('server',{}).get('concealed_packets',0) for r in data):,} | "
                     f"{sum(r.get('server',{}).get('retired',False) for r in data)} | {sum(sweep.healthy(r) for r in data)} |")
    lines += ["", "Totals mix configurations; local tuning also has two extra priority conditions. Use matched comparisons below.", "",
              "## Strict baseline boundary (all three repetitions fully healthy)", "",
              "Each cell is cloud → local minimum N, with calculated audio latency in parentheses. An em dash means no tested allowance was fully healthy in all repetitions.", "",
              "| P | k=1 | k=2 | k=4 | k=8 |", "| --- | --- | --- | --- | --- |"]
    for p in [8, 16, 32, 64, 128]:
        cells = []
        for k in [1, 2, 4, 8]:
            values = []
            for data in [cloud_sweep, groups["sweep"]]:
                valid = []
                for n in [1, 2, 4, 8, 16]:
                    runs = [r for r in data if r["suite"] == "baseline" and config(r) == (p, k, n)]
                    if len(runs) == 3 and all(sweep.healthy(r) for r in runs):
                        valid.append(n)
                values.append(f"{min(valid)} ({timing_ms(p,k,min(valid))['audio_latency']:.3f} ms)" if valid else "—")
            cells.append(" → ".join(values))
        lines.append(f"| {p} | " + " | ".join(cells) + " |")
    lines += ["", "## Original sweep: fixed 60-second configurations", "",
              "| P/k/N | Audio latency ms | Cloud miss % | Local miss % | Cloud late/missing | Local late/missing | Cloud/local concealments | Cloud/local retired | Cloud/local batch p99 us |",
              "| --- | ---: | ---: | ---: | --- | --- | --- | --- | --- |"]
    for cloud in sorted((r for r in cloud_sweep if r["suite"] == "followup"), key=config):
        local = find_long(groups["sweep"] + groups["matched-sweep"], config(cloud))
        lines.append(f"| {'/'.join(map(str,config(cloud)))} | {timing_ms(*config(cloud))['audio_latency']:.3f} | {100*cloud['deadline_miss_fraction']:.5f} | {100*local['deadline_miss_fraction']:.5f} | "
                     f"{cloud['late']:,}/{cloud['missing']:,} | {local['late']:,}/{local['missing']:,} | "
                     f"{cloud['server']['concealed_packets']:,}/{local['server']['concealed_packets']:,} | "
                     f"{cloud['server']['retired']}/{local['server']['retired']} | "
                     f"{cloud['batch_turnaround']['p99_ns']/1000:.1f}/{local['batch_turnaround']['p99_ns']/1000:.1f} |")
    all_local = groups["tuning"] + groups["matched-tuning"]
    lines += ["", "## Original tuning: same 60-second conditions on both hosts", "",
              "Counts are output misses / concealed input. Baselines are fresh yield-free runs using the tuning executable.", "",
              "| P/k/N | Audio latency ms | Cloud-selected alternative (held fixed locally) | Cloud baseline | Local baseline | Cloud alternative | Local same alternative |",
              "| --- | ---: | --- | ---: | ---: | ---: | ---: |"]
    for c in tune.CONFIGS:
        cloud_base = find_long(cloud_tuning, c, "yield-free")
        cloud_alt = next(r for r in cloud_tuning if r["suite"] == "followup" and config(r) == c and r["mode"] != "yield-free")
        local_base = find_long(all_local, c, "yield-free")
        local_alt = find_long(all_local, c, cloud_alt["mode"])
        lines.append(f"| {'/'.join(map(str,c))} | {timing_ms(*c)['audio_latency']:.3f} | {cloud_alt['mode']} | {outcomes(cloud_base)} | {outcomes(local_base)} | {outcomes(cloud_alt)} | {outcomes(local_alt)} |")
    lines += ["", "## Newly available real-time scheduling: matched 60-second runs", "",
              "All three conditions use yielding, the same pinned CPUs, priority 1 for FIFO/RR, and a 200 us input gap. "
              "Counts are output misses / concealed input. Ordinary scheduling has priority 0 and niceness 0.", "",
              "| P/k/N | Allowance ms | Audio latency ms | Ordinary pinned | FIFO pinned | RR pinned |",
              "| --- | ---: | ---: | ---: | ---: | ---: |"]
    for c in tune.CONFIGS:
        values = [outcomes(find_long(all_local, c, mode)) for mode in
                  ["yield-pinned", "yield-pinned-fifo1", "yield-pinned-rr1"]]
        times = timing_ms(*c)
        lines.append(f"| {'/'.join(map(str,c))} | {times['allowance']:.3f} | {times['audio_latency']:.3f} | " + " | ".join(values) + " |")
    lines += ["", "## Short screening, matched workloads (24 runs per condition)", "",
              "Each condition has all eight configurations × three repetitions, mixing calculated audio latencies of 0.833–45.333 ms. "
              "CPU cost covers both processes and their full lifetime, not DSP alone.", "",
              "| Host | Condition | Output misses / slots | Concealed input | Retired | Fully healthy | Mean pair CPU cores |",
              "| --- | --- | ---: | ---: | ---: | ---: | ---: |"]
    for mode in list(tune.MODES) + ["yield-pinned-fifo1", "yield-pinned-rr1"]:
        for host, data in [("Cloud", cloud_tuning), ("Local", groups["tuning"])]:
            runs = [r for r in data if r["suite"] == "screen" and r["mode"] == mode]
            if not runs:
                continue
            lines.append(f"| {host} | {mode} | {sum(r['late']+r['missing'] for r in runs):,} / {sum(r['measured_packets'] for r in runs):,} | "
                         f"{sum(r['server']['concealed_packets'] for r in runs):,} | {sum(r['server']['retired'] for r in runs)} | "
                         f"{sum(sweep.healthy(r) for r in runs)} | {sum(r['resources']['pair_cpu_cores'] for r in runs)/len(runs):.3f} |")
    return "\n".join(lines) + "\n"


def heatmap(cloud, local):
    width, height = 1400, 660
    parts = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">',
             '<rect width="100%" height="100%" fill="white"/>',
             '<style>text{font-family: sans-serif;font-size:12px;fill:#222}.title{font-size:20px}.host{font-size:16px}</style>']

    def text(x, y, value, kind="", anchor="middle"):
        parts.append(f'<text x="{x}" y="{y}" class="{kind}" text-anchor="{anchor}">{html.escape(str(value))}</text>')

    text(width / 2, 28, "Cloud VM vs local PREEMPT_RT laptop: baseline output deadline misses", "title")
    text(width / 2, 50, "Three repetitions per admitted P/k/N; green means zero output misses, not necessarily zero input concealment")
    for host_index, (name, data) in enumerate([("Cloud VM", cloud), ("Local laptop", local)]):
        top = 105 + host_index * 265
        text(20, top - 15, name, "host", "start")
        for panel, k in enumerate([1, 2, 4, 8]):
            left = 80 + panel * 335
            text(left + 125, top - 15, f"k={k}")
            for y, p in enumerate([8, 16, 32, 64, 128]):
                text(left - 12, top + y * 35 + 23, p, anchor="end")
                for x, n in enumerate([1, 2, 4, 8, 16]):
                    runs = [r for r in data if r["suite"] == "baseline" and config(r) == (p, k, n) and r["status"] == "measured"]
                    fill, label = "#dddddd", "ref"
                    if runs:
                        fraction = sum(r["late"] + r["missing"] for r in runs) / sum(r["measured_packets"] for r in runs)
                        if fraction == 0:
                            fill, label = "#d9f0d3", "0"
                        else:
                            strength = max(0, min(1, (math.log10(fraction) + 5) / 5))
                            fill = f"rgb(255,{int(240-170*strength)},{int(160-90*strength)})"
                            label = f"{100*fraction:.2g}%"
                    parts.append(f'<rect x="{left+x*50}" y="{top+y*35}" width="50" height="35" fill="{fill}" stroke="white"/>')
                    text(left + x * 50 + 25, top + y * 35 + 23, label)
            for x, n in enumerate([1, 2, 4, 8, 16]):
                text(left + x * 50 + 25, top + 195, n)
            text(left + 125, top + 219, "Allowance N (packet periods)")
    text(25, 632, "P = packet frames; k = block multiplier; ref = N < k (structural refusal). Color scale: 0.001% (pale) to 100% (red).", anchor="start")
    parts.append("</svg>")
    return "\n".join(parts) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--results", type=Path, default=ROOT / "artifacts/remote-dsp-local")
    parser.add_argument("--output", type=Path, default=ROOT / "docs/remote-dsp/local-evidence")
    args = parser.parse_args()
    source, output = args.results.resolve(), args.output.resolve()
    assert (source / "all-finished.json").exists(), "full campaign must finish before publication"
    groups = {name: read_archive(source / name / "results.jsonl.gz") for name in CAMPAIGNS}
    groups["root"] = source
    checks = check_coverage(groups)
    audits = {name: audit(groups[name], source / name / "results.jsonl.gz") for name in CAMPAIGNS}
    cloud_sweep = read_archive(ROOT / "docs/remote-dsp/evidence/results.jsonl.gz")
    cloud_tuning = read_archive(ROOT / "docs/remote-dsp/tuning-evidence/results.jsonl.gz")
    for cloud in cloud_sweep:
        if cloud["suite"] == "followup":
            find_long(groups["sweep"] + groups["matched-sweep"], config(cloud))
    for cloud in cloud_tuning:
        if cloud["suite"] == "followup":
            find_long(groups["tuning"] + groups["matched-tuning"], config(cloud), cloud["mode"])
    output.mkdir(parents=True, exist_ok=True)
    regeneration = {}
    for name in CAMPAIGNS:
        before = gzip.decompress((source / name / "results.jsonl.gz").read_bytes())
        if "tuning" in name:
            tune.report(source / name)
        else:
            sweep.write_report(source / name)
        assert gzip.decompress((source / name / "results.jsonl.gz").read_bytes()) == before
        regeneration[name] = True
        dest = output / name
        dest.mkdir(exist_ok=True)
        for filename in ["manifest.json", "results.jsonl.gz", "summary.csv", "report.md", "selection.json", "reused.json"]:
            path = source / name / filename
            if path.exists():
                shutil.copy2(path, dest / filename)
        observations = source / name / "scheduling"
        if observations.exists():
            shutil.copytree(observations, dest / "scheduling", dirs_exist_ok=True)
    for filename in ["campaign.json", "all-finished.json", "matched-sweep-reused.json"]:
        shutil.copy2(source / filename, output / filename)
    totals = {key: sum(data[key] for data in audits.values()) for key in
              ["records", "measured_packets", "verified_samples", "unverified_samples", "correctness_errors", "late", "missing", "concealed_packets", "retired_runs"]}
    result = {"campaigns": audits, "totals": totals, "coverage": checks,
              "report_regeneration_payload_identical": regeneration,
              "comparison_script_sha256": sweep.digest(Path(__file__)),
              "credentials_excluded": True, "original_cloud_evidence_modified": False}
    (output / "audit.json").write_text(json.dumps(result, indent=2) + "\n")
    (output / "comparison-tables.md").write_text(tables(groups, cloud_sweep, cloud_tuning))
    (output / "baseline-cloud-vs-local.svg").write_text(heatmap(cloud_sweep, groups["sweep"]))
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
