#!/usr/bin/env python3
"""Controlled Linux scheduling/polling experiments; preserves every outcome."""
from __future__ import annotations

import argparse
import csv
import datetime
import gzip
import json
import os
from pathlib import Path
import platform
import queue
import random
import resource
import subprocess
import sys
import threading
import time

from sweep_remote_dsp import ROOT, capture, digest, healthy

CONFIGS = [(8, 1, 4), (8, 1, 8), (8, 1, 16), (16, 4, 4),
           (16, 4, 8), (16, 4, 16), (128, 8, 8), (128, 8, 16)]
MODES = {
    "yield-free": ("yield", "yield", False, 200),
    "yield-pinned": ("yield", "yield", True, 200),
    "spin-free": ("spin", "spin", False, 200),
    "spin-pinned": ("spin", "spin", True, 200),
    "client-spin-pinned": ("spin", "yield", True, 200),
    "server-spin-pinned": ("yield", "spin", True, 200),
    "spin-pinned-gap1000": ("spin", "spin", True, 1000),
}


def snapshot() -> dict:
    base = Path("/sys/fs/cgroup")
    counters = {}
    if (base / "cpu.stat").exists():
        counters = {k: int(v) for k, v in (line.split() for line in (base / "cpu.stat").read_text().splitlines())}
    children = resource.getrusage(resource.RUSAGE_CHILDREN)
    return {"wall": time.monotonic(), "user": children.ru_utime,
            "system": children.ru_stime, "cgroup": counters}


def priority_probes() -> list[dict]:
    script = "import os,json;print(json.dumps({'scheduler':os.sched_getscheduler(0),'nice':os.getpriority(os.PRIO_PROCESS,0)}))"
    requests = [("fifo1", ["chrt", "--fifo", "1"], os.SCHED_FIFO, 0),
                ("rr1", ["chrt", "--rr", "1"], os.SCHED_RR, 0),
                ("nice-minus10", ["nice", "-n", "-10"], os.SCHED_OTHER, -10)]
    probes = []
    for name, prefix, policy, nice in requests:
        command = prefix + [sys.executable, "-c", script]
        result = subprocess.run(command, capture_output=True, text=True, timeout=10)
        try:
            actual = json.loads(result.stdout)
        except ValueError:
            actual = None
        available = result.returncode == 0 and actual == {"scheduler": policy, "nice": nice}
        probes.append({"name": name, "command": command, "exit_code": result.returncode,
                       "stdout": result.stdout, "stderr": result.stderr,
                       "actual": actual, "available": available, "prefix": prefix})
    return probes


def create_manifest(args, binary: Path, output: Path) -> dict:
    path = output / "manifest.json"
    if path.exists():
        data = json.loads(path.read_text())
        if data["binary_sha256"] != digest(binary) or data["runner_sha256"] != digest(Path(__file__)):
            raise RuntimeError("binary/runner changed; use a fresh output directory")
        if data["client_cpu"] != args.client_cpu or data["server_cpu"] != args.server_cpu:
            raise RuntimeError("CPU selection changed; use a fresh output directory")
        return data
    allowed = sorted(os.sched_getaffinity(0))
    if args.client_cpu == args.server_cpu or not {args.client_cpu, args.server_cpu} <= set(allowed):
        raise RuntimeError("select two distinct CPUs in the inherited affinity mask")
    probes = priority_probes()
    data = {"started_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "source_revision": capture(["git", "rev-parse", "HEAD"]),
            "source_changes": capture(["git", "status", "--porcelain"]),
            "binary": str(binary), "binary_sha256": digest(binary),
            "runner_sha256": digest(Path(__file__)), "command": sys.argv,
            "os": platform.platform(), "python": platform.python_version(),
            "rustc": capture(["rustc", "--version"]), "cargo": capture(["cargo", "--version"]),
            "build_profile": "release", "rustflags": os.environ.get("RUSTFLAGS", ""),
            "allowed_cpus": allowed, "client_cpu": args.client_cpu, "server_cpu": args.server_cpu,
            "inherited_scheduler": os.sched_getscheduler(0), "inherited_nice": os.getpriority(os.PRIO_PROCESS, 0),
            "priority_probes": probes, "modes": MODES, "configs": CONFIGS,
            "seed": 20261103, "screen_repetitions": 3, "screen_seconds": 2,
            "followup_seconds": 60, "warmup_seconds": 0.5,
            "limitations": "shared Linux host; affinity is not CPU isolation; cgroup counters include other tasks; no physical NIC/FPGA; no builds/tests during measurements"}
    for cpu in [args.client_cpu, args.server_cpu]:
        topology = Path(f"/sys/devices/system/cpu/cpu{cpu}/topology")
        data[f"cpu{cpu}_topology"] = {p.name: p.read_text().strip() for p in
                                       (topology / "physical_package_id", topology / "core_id", topology / "thread_siblings_list") if p.exists()}
    for name in ["cpu.max", "cpuset.cpus.effective", "memory.max"]:
        p = Path("/sys/fs/cgroup") / name
        data[name] = p.read_text().strip() if p.exists() else None
    data["cpu_model"] = next((line.split(":", 1)[1].strip() for line in Path("/proc/cpuinfo").read_text().splitlines() if line.startswith("model name")), "unknown")
    path.write_text(json.dumps(data, indent=2) + "\n")
    return data


def modes(manifest: dict) -> dict:
    result = dict(MODES)
    for probe in manifest["priority_probes"]:
        if probe["available"]:
            result[f"spin-pinned-{probe['name']}"] = ("spin", "spin", True, 200)
    return result


def command_for(binary, role, mode, manifest, extra):
    client_poll, server_poll, pinned, _ = modes(manifest)[mode]
    cpus = [manifest[f"{role}_cpu"]] if pinned else manifest["allowed_cpus"]
    prefix = ["taskset", "--cpu-list", ",".join(map(str, cpus))]
    for probe in manifest["priority_probes"]:
        if mode.endswith("-" + probe["name"]) and probe["available"]:
            prefix += probe["prefix"]
    command = prefix + [str(binary), "client" if role == "client" else "serve",
                        "--polling", client_poll if role == "client" else server_poll, *extra]
    return command, cpus


def run_case(binary, output, manifest, suite, config, mode, repetition, seconds):
    p, k, n = config
    ident = f"{suite}-p{p}-k{k}-n{n}-{mode}-r{repetition}"
    path = output / f"{ident}.json"
    if path.exists():
        return json.loads(path.read_text())
    gap = modes(manifest)[mode][3]
    key = output / "credential.key"
    result_path = output / f"{ident}.requester.json.tmp"
    server_command, server_cpus = command_for(binary, "server", mode, manifest,
        ["--once", "--processor", "delay-copy", "--credential-file", str(key)])
    before = snapshot()
    row = {"case": ident, "suite": suite, "mode": mode, "repetition": repetition,
           "started_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
           "requested": {"P": p, "k": k, "N": n, "A": 0, "channels": 2, "seconds": seconds, "warmup": 0.5, "gap_timeout_us": gap},
           "server_command": server_command}
    with (output / f"{ident}.server.log").open("w") as error_log:
        server = subprocess.Popen(server_command, stdout=subprocess.PIPE, stderr=error_log, text=True)
        client = None
        try:
            ready_queue = queue.Queue()
            threading.Thread(target=lambda: ready_queue.put(server.stdout.readline()), daemon=True).start()
            ready = json.loads(ready_queue.get(timeout=15))
            row["server_affinity"] = sorted(os.sched_getaffinity(server.pid))
            if row["server_affinity"] != server_cpus:
                raise RuntimeError("server affinity differs from requested affinity")
            client_command, client_cpus = command_for(binary, "client", mode, manifest,
                ["--endpoint", ready["control_endpoint"], "--credential-file", str(key),
                 "--packet-frames", str(p), "--multiplier", str(k), "--allowance", str(n),
                 "--gap-timeout-us", str(gap), "--reorder-packets", "64", "--seed", "20261102",
                 "--seconds", str(seconds), "--warmup", "0.5", "--output", str(result_path)])
            row["client_command"] = client_command
            client = subprocess.Popen(client_command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            deadline = time.monotonic() + 5
            while sorted(os.sched_getaffinity(client.pid)) != client_cpus:
                if time.monotonic() >= deadline or client.poll() is not None:
                    raise RuntimeError("client affinity was not applied")
                time.sleep(0.001)
            row["client_affinity"] = sorted(os.sched_getaffinity(client.pid))
            stdout, stderr = client.communicate(timeout=seconds + 25)
            (output / f"{ident}.client.log").write_text(stdout + stderr)
            row["client_exit_code"] = client.returncode
            server.wait(timeout=10)
            row["server_exit_code"] = server.returncode
            row.update(json.loads(result_path.read_text()))
            result_path.unlink()
        except Exception as error:
            row.update(status="failed", error=f"{type(error).__name__}: {error}")
        finally:
            for child in [client, server]:
                if child is not None:
                    if child.poll() is None:
                        child.kill()
                    child.wait()
    after = snapshot()
    wall = after["wall"] - before["wall"]
    user, system = after["user"] - before["user"], after["system"] - before["system"]
    row["resources"] = {"pair_wall_seconds": wall, "pair_user_seconds": user,
                        "pair_system_seconds": system, "pair_cpu_cores": (user + system) / wall,
                        "cgroup_delta": {key: value - before["cgroup"].get(key, 0) for key, value in after["cgroup"].items()}}
    row["finished_utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    path.write_text(json.dumps(row, indent=2) + "\n")
    return row


def rows(output):
    return [json.loads(p.read_text()) for p in sorted(output.glob("screen-*.json")) + sorted(output.glob("followup-*.json"))]


def score(group):
    if any(r.get("status") != "measured" or r.get("correctness_errors", 0) or r.get("unverified_samples", 0) for r in group):
        return (float("inf"), float("inf"), float("inf"))
    count = sum(r["measured_packets"] for r in group)
    failures = sum(r["late"] + r["missing"] + r["server"]["concealed_packets"] + r["send_errors_or_skipped"] for r in group)
    return (sum(bool(r["server"]["retired"]) for r in group), failures / count,
            sum(r["resources"]["pair_cpu_cores"] for r in group) / len(group))


def select(data, manifest):
    selected = []
    for config in CONFIGS:
        groups = {mode: [r for r in data if r["suite"] == "screen" and r["mode"] == mode and
                        tuple(r["requested"][key] for key in ["P", "k", "N"]) == config] for mode in modes(manifest) if mode != "yield-free"}
        groups = {mode: group for mode, group in groups.items() if len(group) == 3}
        if not groups:
            raise RuntimeError(f"incomplete screen for {config}")
        mode = min(groups, key=lambda mode: score(groups[mode]))
        selected.append({"config": config, "mode": mode, "score": score(groups[mode]),
                         "rule": "fewest retired repetitions, then (output misses + input concealments + source errors)/measured slots, then pair CPU cost; selection is exploratory"})
    return selected


def report(output):
    data = rows(output)
    with gzip.open(output / "results.jsonl.gz", "wt", encoding="utf-8") as handle:
        for row in data:
            handle.write(json.dumps(row, separators=(",", ":")) + "\n")
    fields = ["case", "suite", "mode", "P", "k", "N", "gap_timeout_us", "seconds", "repetition", "status", "on_time", "late", "missing", "concealed", "retired", "verified_samples", "correctness_errors", "pair_cpu_cores", "throttled_usec", "client_max_gap_ns", "server_max_gap_ns"]
    with (output / "summary.csv").open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=fields)
        writer.writeheader()
        for row in data:
            record = dict(row) | row["requested"]
            record.update(concealed=row.get("server", {}).get("concealed_packets"), retired=row.get("server", {}).get("retired"),
                          pair_cpu_cores=row["resources"]["pair_cpu_cores"], throttled_usec=row["resources"]["cgroup_delta"].get("throttled_usec"),
                          client_max_gap_ns=row.get("client_service_gaps", {}).get("maximum_ns"), server_max_gap_ns=row.get("server_service_gaps", {}).get("maximum_ns"))
            writer.writerow({key: record.get(key) for key in fields})
    lines = ["# Scheduling/polling experiment", "", f"{len(data)} recorded runs; {sum(healthy(r) for r in data if r.get('status') == 'measured')} fully healthy runs.", "",
             "All outcomes are retained. Pair CPU covers launch, warm-up, measurement and reporting. Service gaps include process work and scheduling delays, not just scheduler latency. Cgroup deltas include other tasks.", "",
             "| Suite | P | k | N | Mode | Runs | Output misses / slots | Concealed | Retired | Mean pair CPU cores |", "| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |"]
    keys = sorted({(r["suite"], r["requested"]["P"], r["requested"]["k"], r["requested"]["N"], r["mode"]) for r in data})
    for suite, p, k, n, mode in keys:
        group = [r for r in data if (r["suite"], r["requested"]["P"], r["requested"]["k"], r["requested"]["N"], r["mode"]) == (suite, p, k, n, mode)]
        measured = [r for r in group if r.get("status") == "measured"]
        lines.append(f"| {suite} | {p} | {k} | {n} | {mode} | {len(group)} | {sum(r['late']+r['missing'] for r in measured)} / {sum(r['measured_packets'] for r in measured)} | {sum(r['server']['concealed_packets'] for r in measured)} | {sum(r['server']['retired'] for r in measured)} | {sum(r['resources']['pair_cpu_cores'] for r in group)/len(group):.3f} |")
    (output / "report.md").write_text("\n".join(lines) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/shoop_dsp_host")
    parser.add_argument("--output", type=Path, default=ROOT / "work/remote-dsp-tuning")
    parser.add_argument("--client-cpu", type=int, default=1)
    parser.add_argument("--server-cpu", type=int, default=2)
    parser.add_argument("--profile", choices=["smoke", "screen", "followup", "all"], default="all")
    parser.add_argument("--report-only", action="store_true")
    args = parser.parse_args()
    output, binary = args.output.resolve(), args.binary.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if args.report_only:
        report(output)
        return 0
    manifest = create_manifest(args, binary, output)
    key = output / "credential.key"
    if not key.exists():
        subprocess.run([str(binary), "generate-credential", str(key)], check=True)
    cases = []
    if args.profile in ["all", "screen"]:
        cases = [("screen", config, mode, repeat, 2) for config in CONFIGS for mode in modes(manifest) for repeat in range(3)]
    elif args.profile == "smoke":
        cases = [("screen", (16, 4, 8), mode, 0, 0.05) for mode in ["yield-free", "spin-pinned", "spin-pinned-gap1000"]]
    random.Random(20261103).shuffle(cases)
    for case in cases:
        row = run_case(binary, output, manifest, *case)
        print(json.dumps({key: row.get(key) for key in ["case", "status", "late", "missing", "correctness_errors", "error"]}), flush=True)
    if args.profile in ["all", "followup"]:
        if any(r.get("status") != "measured" or r.get("client_exit_code", 0) or r.get("server_exit_code", 0) for r in rows(output)):
            report(output)
            return 1
        selection = select(rows(output), manifest)
        (output / "selection.json").write_text(json.dumps(selection, indent=2) + "\n")
        cases = [("followup", tuple(item["config"]), mode, 0, 60) for item in selection for mode in ["yield-free", item["mode"]]]
        random.Random(20261104).shuffle(cases)
        for case in cases:
            row = run_case(binary, output, manifest, *case)
            print(json.dumps({key: row.get(key) for key in ["case", "status", "late", "missing", "correctness_errors", "error"]}), flush=True)
    report(output)
    return int(any(r.get("status") != "measured" or r.get("correctness_errors", 0) or r.get("unverified_samples", 0) or r.get("client_exit_code", 0) or r.get("server_exit_code", 0) for r in rows(output)))


if __name__ == "__main__":
    raise SystemExit(main())
