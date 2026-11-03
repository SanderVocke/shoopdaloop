#!/usr/bin/env python3
"""Reproduce the archived DSP campaigns and add matched, permitted priority runs."""
from __future__ import annotations

import argparse
import datetime
import gzip
import json
import os
from pathlib import Path
import platform
import random
import subprocess
import sys
import threading

import sweep_remote_dsp as sweep
import tune_remote_dsp as tune

ROOT = sweep.ROOT


def read_optional(path):
    try:
        return Path(path).read_text().strip()
    except OSError as error:
        return {"unavailable": str(error)}


def inventory(binary, output):
    scripts = [Path(__file__), ROOT / "scripts/sweep_remote_dsp.py",
               ROOT / "scripts/tune_remote_dsp.py"]
    identity = {str(p.relative_to(ROOT)): sweep.digest(p) for p in scripts}
    path = output / "campaign.json"
    if path.exists():
        previous = json.loads(path.read_text())
        if previous["scripts_sha256"] != identity or previous["binary_sha256"] != sweep.digest(binary):
            raise RuntimeError("campaign executable/scripts changed; use a new output directory")
        return
    paths = ["/proc/self/limits", "/proc/self/status", "/proc/self/cgroup",
             "/proc/self/mountinfo", "/proc/meminfo", "/etc/os-release",
             "/sys/kernel/realtime", "/sys/devices/system/cpu/isolated",
             "/sys/devices/system/cpu/nohz_full", "/sys/devices/system/cpu/amd_pstate/status",
             "/sys/firmware/acpi/platform_profile", "/proc/sys/kernel/sched_rt_runtime_us",
             "/proc/sys/kernel/sched_rt_period_us", "/proc/sys/net/core/rmem_max",
             "/proc/sys/net/core/wmem_max"]
    for pattern in ["/sys/devices/system/cpu/cpufreq/policy*/scaling_governor",
                    "/sys/devices/system/cpu/cpufreq/policy*/energy_performance_preference",
                    "/sys/class/power_supply/*/online", "/sys/class/power_supply/*/status",
                    "/sys/class/thermal/thermal_zone*/temp"]:
        paths.extend(str(p) for p in Path("/").glob(pattern.lstrip("/")))
    cgroup = read_optional("/proc/self/cgroup")
    if isinstance(cgroup, str):
        for line in cgroup.splitlines():
            if line.startswith("0::"):
                current = Path("/sys/fs/cgroup") / line[3:].lstrip("/")
                while current.is_relative_to("/sys/fs/cgroup"):
                    paths.extend(str(current / name) for name in
                                 ["cpu.max", "cpu.stat", "cpuset.cpus.effective", "memory.max"])
                    current = current.parent
    data = {"started_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "command": sys.argv, "source_revision": sweep.capture(["git", "rev-parse", "HEAD"]),
            "source_changes": sweep.capture(["git", "status", "--porcelain"]),
            "binary_sha256": sweep.digest(binary), "scripts_sha256": identity,
            "platform": platform.platform(), "lscpu": sweep.capture(["lscpu"]),
            "cpu_topology": sweep.capture(["lscpu", "-e=CPU,CORE,MAXMHZ"]),
            "host_files": {p: read_optional(p) for p in sorted(set(paths))},
            "notes": "No system-wide changes. Campaigns run sequentially; no concurrent builds/tests. "
                     "Affinity is not CPU isolation. Root cgroup counters include unrelated host tasks."}
    path.write_text(json.dumps(data, indent=2) + "\n")


def scheduling_snapshot():
    processes = []
    for proc in Path("/proc").glob("[0-9]*"):
        try:
            if (proc / "comm").read_text().strip() != "shoop_dsp_host":
                continue
            threads = []
            for task in sorted((proc / "task").iterdir()):
                tid = int(task.name)
                threads.append({"tid": tid, "name": (task / "comm").read_text().strip(),
                                "policy": os.sched_getscheduler(tid),
                                "priority": os.sched_getparam(tid).sched_priority,
                                "nice": os.getpriority(os.PRIO_PROCESS, tid),
                                "affinity": sorted(os.sched_getaffinity(tid))})
            processes.append({"pid": int(proc.name),
                              "command": (proc / "cmdline").read_bytes().decode().split("\0")[:-1],
                              "threads": threads})
        except (OSError, ProcessLookupError):
            continue
    return {"observed_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "processes": processes}


def observed_case(binary, output, manifest, config, mode):
    ident = f"followup-p{config[0]}-k{config[1]}-n{config[2]}-{mode}-r0"
    observation = output / "scheduling" / f"{ident}.json"
    if (output / f"{ident}.json").exists():
        return tune.run_case(binary, output, manifest, "followup", config, mode, 0, 60)
    stop = threading.Event()

    def observe():
        if not stop.wait(1):
            observation.parent.mkdir(parents=True, exist_ok=True)
            observation.write_text(json.dumps(scheduling_snapshot(), indent=2) + "\n")

    observer = threading.Thread(target=observe, daemon=True)
    observer.start()
    try:
        return tune.run_case(binary, output, manifest, "followup", config, mode, 0, 60)
    finally:
        stop.set()
        observer.join()


def extensions(args, binary, output):
    references = []
    tuning_data = tune.rows(output / "tuning")
    extra = output / "matched-tuning"
    extra.mkdir(exist_ok=True)
    manifest = tune.create_manifest(args, binary, extra)
    key = extra / "credential.key"
    if not key.exists():
        subprocess.run([str(binary), "generate-credential", str(key)], check=True)
    pairs = [(config, mode) for config in tune.CONFIGS for mode in
             ["yield-pinned", "yield-pinned-fifo1", "yield-pinned-rr1"] if mode in tune.modes(manifest)]
    with gzip.open(ROOT / "docs/remote-dsp/tuning-evidence/results.jsonl.gz", "rt") as handle:
        cloud_tuning = [json.loads(line) for line in handle]
    pairs.extend((tuple(row["requested"][k] for k in ["P", "k", "N"]), row["mode"])
                 for row in cloud_tuning if row["suite"] == "followup" and row["mode"] != "yield-free")
    pairs = sorted(set(pairs))
    random.Random(20261105).shuffle(pairs)
    for config, mode in pairs:
        existing = next((r for r in tuning_data if r["suite"] == "followup" and r["mode"] == mode
                         and tuple(r["requested"][k] for k in ["P", "k", "N"]) == config), None)
        if existing:
            references.append({"config": config, "mode": mode, "reused": f"tuning/{existing['case']}.json"})
            continue
        row = observed_case(binary, extra, manifest, config, mode)
        print(json.dumps({k: row.get(k) for k in ["case", "status", "late", "missing", "correctness_errors", "error"]}), flush=True)
        if row.get("status") != "measured" or row.get("correctness_errors", 0) or row.get("client_exit_code", 0) or row.get("server_exit_code", 0):
            tune.report(extra)
            raise RuntimeError(f"matched tuning failed: {row['case']}")
    (extra / "reused.json").write_text(json.dumps(references, indent=2) + "\n")
    tune.report(extra)
    data = sweep.results(output / "sweep")
    extra = output / "matched-sweep"
    extra.mkdir(exist_ok=True)
    sweep.manifest(binary, extra, args)
    if not (extra / "credential.key").exists():
        subprocess.run([str(binary), "generate-credential", str(extra / "credential.key")], check=True)
    with gzip.open(ROOT / "docs/remote-dsp/evidence/results.jsonl.gz", "rt") as handle:
        cloud_sweep = [json.loads(line) for line in handle]
    references = []
    for row in cloud_sweep:
        if row["suite"] != "followup":
            continue
        config = tuple(row["requested"][k] for k in ["P", "k", "N"])
        existing = next((r for r in data if r["suite"] == "followup" and
                         tuple(r["requested"][k] for k in ["P", "k", "N"]) == config), None)
        if existing:
            references.append({"config": config, "reused": f"sweep/{existing['case']}.json"})
            continue
        measured = sweep.run_case(binary, extra, "followup", *config, seconds=60, warmup=0.5,
                                  rationale="fixed cloud followup configuration for matched comparison")
        print(f"matched sweep {measured['case']}: {measured.get('deadline_miss_fraction')}", flush=True)
        if measured["status"] != "measured" or measured.get("correctness_errors", 0) or measured.get("client_exit_code", 0) or measured.get("server_exit_code", 0):
            sweep.write_report(extra)
            raise RuntimeError(f"matched sweep failed: {measured['case']}")
    sweep.write_report(extra)
    (output / "matched-sweep-reused.json").write_text(json.dumps(references, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "artifacts/remote-dsp-local")
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/shoop_dsp_host")
    parser.add_argument("--client-cpu", type=int, default=2)
    parser.add_argument("--server-cpu", type=int, default=4)
    parser.add_argument("--phase", choices=["all", "sweep", "tuning", "extensions"], default="all")
    args = parser.parse_args()
    output, binary = args.output.resolve(), args.binary.resolve()
    output.mkdir(parents=True, exist_ok=True)
    inventory(binary, output)
    for phase in ["sweep", "tuning"]:
        if args.phase not in ["all", phase]:
            continue
        command = [sys.executable, str(ROOT / "scripts/sweep_remote_dsp.py")]
        if phase == "tuning":
            command = [sys.executable, str(ROOT / "scripts/tune_remote_dsp.py"),
                       "--client-cpu", str(args.client_cpu), "--server-cpu", str(args.server_cpu)]
        command += ["--binary", str(binary), "--profile", "all", "--output", str(output / phase)]
        subprocess.run(command, check=True, cwd=ROOT)
    if args.phase in ["all", "extensions"]:
        extensions(args, binary, output)
    (output / f"{args.phase}-finished.json").write_text(json.dumps({
        "finished_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "phase": args.phase, "binary_sha256": sweep.digest(binary)}, indent=2) + "\n")


if __name__ == "__main__":
    main()
