#!/usr/bin/env python3
"""Run and report separate-process localhost DSP experiments without timing gates."""
from __future__ import annotations
import argparse
import csv
import datetime
import gzip
import hashlib
import itertools
import json
import os
from pathlib import Path
import platform
import queue
import random
import subprocess
import threading

ROOT = Path(__file__).resolve().parents[1]


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def capture(args: list[str]) -> str:
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def manifest(binary: Path, output: Path, args: argparse.Namespace) -> None:
    path = output / "manifest.json"
    if path.exists():
        old = json.loads(path.read_text())
        if old["binary_sha256"] != digest(binary) or old["runner_sha256"] != digest(Path(__file__)):
            raise RuntimeError("binary or runner changed; use another output directory")
        return
    environment = {}
    for name in ("cpu.max", "memory.max"):
        file = Path("/sys/fs/cgroup") / name
        if file.exists():
            environment[name] = file.read_text().strip()
    cpu = Path("/proc/cpuinfo")
    if cpu.exists():
        environment["cpu_model"] = next((line.split(":", 1)[1].strip() for line in cpu.read_text().splitlines() if line.startswith("model name")), "unknown")
    data = {
        "started_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "source_revision": capture(["git", "rev-parse", "HEAD"]),
        "source_changes": capture(["git", "status", "--porcelain"]),
        "binary": str(binary), "binary_sha256": digest(binary),
        "runner_sha256": digest(Path(__file__)), "runner_arguments": vars(args) | {"binary": str(binary), "output": str(output)},
        "os": platform.platform(), "machine": platform.machine(), "python": platform.python_version(),
        "rustc": capture(["rustc", "--version"]), "cargo": capture(["cargo", "--version"]),
        "cpus_reported": os.cpu_count(), "affinity": sorted(os.sched_getaffinity(0)) if hasattr(os, "sched_getaffinity") else None,
        "scheduling": "inherited ordinary scheduling; no affinity, RT priority or system tuning applied",
        "build_profile": "release", "rustflags": os.environ.get("RUSTFLAGS", ""),
        "network": "localhost UDP audio; authenticated TCP control; separate native processes", "environment": environment,
    }
    path.write_text(json.dumps(data, indent=2) + "\n")


def case_id(suite: str, p: int, k: int, n: int, delay: int, channels: int, repeat: int) -> str:
    return f"{suite}-p{p}-k{k}-n{n}-a{delay}-c{channels}-r{repeat}"


def run_case(binary: Path, output: Path, suite: str, p: int, k: int, n: int,
             delay: int = 0, channels: int = 2, repeat: int = 0,
             seconds: float = 2.0, warmup: float = 0.5, rationale: str = "") -> dict:
    ident = case_id(suite, p, k, n, delay, channels, repeat)
    result_path = output / f"{ident}.json"
    if result_path.exists():
        return json.loads(result_path.read_text())
    credential = output / "credential.key"
    common = ["--credential-file", str(credential), "--packet-frames", str(p), "--multiplier", str(k),
              "--allowance", str(n), "--delay", str(delay), "--channels", str(channels),
              "--seconds", str(seconds), "--warmup", str(warmup), "--seed", "20261102", "--output", str(result_path)]
    preflight = [str(binary), "client", "--endpoint", "127.0.0.1:1", "--validate-only", *common]
    subprocess.run(preflight, check=True, capture_output=True, text=True, timeout=15)
    report = json.loads(result_path.read_text())
    command = preflight
    if report["status"] == "admitted":
        result_path.unlink()
        server_command = [str(binary), "serve", "--once", "--processor", "delay-copy", "--credential-file", str(credential)]
        with (output / f"{ident}.server.log").open("w") as stderr:
            server = subprocess.Popen(server_command, stdout=subprocess.PIPE, stderr=stderr, text=True)
            try:
                ready_line: queue.Queue[str] = queue.Queue()
                threading.Thread(target=lambda: ready_line.put(server.stdout.readline()), daemon=True).start()
                ready = json.loads(ready_line.get(timeout=15))
                command = [str(binary), "client", "--endpoint", ready["control_endpoint"], *common]
                completed = subprocess.run(command, capture_output=True, text=True, timeout=seconds + warmup + 20)
                (output / f"{ident}.client.log").write_text(completed.stdout + completed.stderr)
                if result_path.exists():
                    report = json.loads(result_path.read_text())
                else:
                    report = {"status": "failed", "error": "requester did not write measurements"}
                report["client_exit_code"] = completed.returncode
                server.wait(timeout=10)
                report["server_exit_code"] = server.returncode
            except Exception as error:
                report = {"status": "failed", "error": f"{type(error).__name__}: {error}"}
            finally:
                if server.poll() is None:
                    server.kill()
                    server.wait()
    report.update({"case": ident, "suite": suite, "repetition": repeat, "command": command,
                   "rationale": rationale, "requested": {"P": p, "k": k, "N": n, "A": delay, "channels": channels,
                                                          "seconds": seconds, "warmup": warmup}})
    result_path.write_text(json.dumps(report, indent=2) + "\n")
    return report


def results(output: Path) -> list[dict]:
    return [json.loads(p.read_text()) for p in sorted(output.glob("*.json")) if p.name != "manifest.json"]


def healthy(row: dict) -> bool:
    stats = row.get("server", {})
    return (row.get("status") == "measured" and row["late"] + row["missing"] == 0
            and row.get("correctness_errors", 0) == 0 and row.get("verified_samples", 0) > 0
            and row.get("unverified_samples", 0) == 0 and row.get("send_errors_or_skipped", 0) == 0
            and stats.get("concealed_packets", 0) == 0 and not stats.get("retired", False))


def write_report(output: Path) -> None:
    data = results(output)
    with gzip.open(output / "results.jsonl.gz", "wt", encoding="utf-8") as handle:
        for row in data:
            handle.write(json.dumps(row, separators=(",", ":")) + "\n")
    fields = ["case", "suite", "P", "k", "N", "A", "channels", "status", "on_time", "late", "missing", "correctness_errors", "verified_samples", "deadline_miss_fraction", "rtt_p99_ns", "rtt_max_ns", "batch_p99_ns", "sender_p99_ns", "retired"]
    with (output / "summary.csv").open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=fields)
        writer.writeheader()
        for row in data:
            record = {key: row.get(key) for key in fields}
            record.update(row["requested"])
            record.update({"rtt_p99_ns": row.get("rtt", {}).get("p99_ns"), "rtt_max_ns": row.get("rtt", {}).get("max_ns"),
                           "batch_p99_ns": row.get("batch_turnaround", {}).get("p99_ns"), "sender_p99_ns": row.get("sender_lateness", {}).get("p99_ns"),
                           "retired": row.get("server", {}).get("retired")})
            writer.writerow({key: record.get(key) for key in fields})
    baseline = [r for r in data if r["suite"] == "baseline"]
    measured = [r for r in data if r["status"] == "measured"]
    failures = [r for r in data if r["status"] == "failed" or r.get("correctness_errors", 0) or r.get("client_exit_code", 0) or r.get("server_exit_code", 0)]
    lines = ["# Localhost network DSP measurements", "", "Actual separate-process UDP measurements; no hardware NIC or FPGA was used.", "",
             f"Recorded {len(data)} runs/refusals: {len(measured)} measured, {sum(r['status'] == 'rejected' for r in data)} rejected, {len(failures)} correctness/infrastructure failures.",
             f"Verified {sum(r.get('verified_samples', 0) for r in data):,} received audio samples; {sum(r.get('correctness_errors', 0) for r in data):,} incorrect samples; {sum(r.get('unverified_samples', 0) for r in data):,} explicitly unverified samples.", "",
             "## Lowest allowance with no observed misses in all baseline repetitions", "",
             "Values are packet periods. Every repetition must have zero output deadline misses, zero input concealment/source skips, verified samples and a healthy stream. An em dash means no tested allowance met those conditions.", "",
             "| P | k=1 | k=2 | k=4 | k=8 |", "| --- | --- | --- | --- | --- |"]
    for p in [8, 16, 32, 64, 128]:
        cells = []
        for k in [1, 2, 4, 8]:
            valid = []
            for n in [1, 2, 4, 8, 16]:
                runs = [r for r in baseline if r["requested"]["P"] == p and r["requested"]["k"] == k and r["requested"]["N"] == n and r["status"] == "measured"]
                if runs and all(healthy(r) for r in runs):
                    valid.append(n)
            cells.append(str(min(valid)) if valid else "—")
        lines.append(f"| {p} | " + " | ".join(cells) + " |")
    lines += ["", "## Follow-up runs", "", "| P | k | N | seconds | miss fraction | reason |", "| --- | --- | --- | --- | --- | --- |"]
    for row in data:
        if row["suite"] == "followup":
            req = row["requested"]
            lines.append(f"| {req['P']} | {req['k']} | {req['N']} | {req['seconds']} | {row.get('deadline_miss_fraction', 'failed')} | {row['rationale']} |")
    lines += ["", "## Interpretation and reproduction", "",
              "Packet RTT includes aggregation for early packets in each batch. Batch turnaround starts at actual final-input submission and requires a complete, unconcealed batch. Server DSP durations use only its local clock.",
              "Sender lateness is measured against nominal readiness. Consumption deadlines do not move when a process wakes late. Missing and late responses remain in the miss-rate denominator; valid late audio is still checked for sample correctness.",
              "Tail percentiles describe finite observed samples, not worst-case guarantees. Inspect histogram counts before interpreting rare tails. Shared-host process scheduling and loopback omit physical-network behavior.", "",
              "The archive contains every per-run report, commands, configuration, socket buffers, counters, missing masks, timing histograms (1 microsecond bins), exact observed percentile summaries and sample counts. The manifest records source/binary/runner hashes and host conditions.", "",
              "```sh", "cargo build --release -p shoop_dsp_host --no-default-features", "python3 scripts/sweep_remote_dsp.py --profile all --output work/remote-dsp-sweep", "python3 scripts/sweep_remote_dsp.py --report-only --output work/remote-dsp-sweep", "```", "",
              "See `manifest.json`, `summary.csv` and `results.jsonl.gz` alongside this report. No RT priority, affinity or kernel/network tuning was applied."]
    (output / "report.md").write_text("\n".join(lines) + "\n")


def followups(data: list[dict]) -> list[tuple[int, int, int, str]]:
    selected = []
    for p, k in [(8, 1), (16, 4), (128, 8)]:
        groups = {}
        for row in data:
            if row["suite"] == "baseline" and row["status"] == "measured" and row["requested"]["P"] == p and row["requested"]["k"] == k:
                groups.setdefault(row["requested"]["N"], []).append(row)
        if not groups:
            continue
        zero = [n for n, rows in groups.items() if all(healthy(r) for r in rows)]
        boundary = min(zero) if zero else min(groups, key=lambda n: sum(r["deadline_miss_fraction"] for r in groups[n]) / len(groups[n]))
        selected.append((p, k, boundary, "lowest observed miss-free baseline allowance" if zero else "lowest observed baseline miss fraction; none miss-free"))
        neighbors = [n for n in sorted(groups) if n < boundary]
        other = max(neighbors) if neighbors else next((n for n in sorted(groups) if n > boundary), None)
        if other is not None:
            selected.append((p, k, other, "adjacent tested allowance to the observed boundary"))
    return selected[:6]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/shoop_dsp_host")
    parser.add_argument("--output", type=Path, default=ROOT / "work/remote-dsp-sweep")
    parser.add_argument("--profile", choices=["all", "baseline", "delays", "followup", "smoke"], default="all")
    parser.add_argument("--report-only", action="store_true")
    args = parser.parse_args()
    binary = args.binary.resolve()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if args.report_only:
        write_report(output)
        return 0
    manifest(binary, output, args)
    credential = output / "credential.key"
    if not credential.exists():
        subprocess.run([str(binary), "generate-credential", str(credential)], check=True)
    cases = []
    if args.profile in ("all", "baseline"):
        for p, k, n in itertools.product([8, 16, 32, 64, 128], [1, 2, 4, 8], [1, 2, 4, 8, 16]):
            for repeat in range(3 if n >= k else 1):
                cases.append(("baseline", p, k, n, 0, 2, repeat, 2.0, 0.5, "planned baseline matrix"))
        random.Random(20261102).shuffle(cases)
    if args.profile in ("all", "delays"):
        for p, k in itertools.product([8, 16, 32, 64, 128], [1, 2, 4, 8]):
            for delay in (1, p * k + 3):
                cases.append(("delay", p, k, 64, delay, 2, 0, 0.5, 0.1, "non-aligned sample delay with larger allowance"))
        cases += [("mtu", 16, 4, 64, 0, 16, 0, 0.5, 0.1, "16-channel MTU acceptance"), ("mtu", 32, 4, 64, 0, 16, 0, 0.5, 0.1, "16-channel MTU refusal")]
    if args.profile == "smoke":
        for p, k, n, delay in [(8, 1, 1, 0), (16, 4, 64, 67), (128, 8, 16, 1), (16, 4, 2, 0)]:
            cases.append(("smoke", p, k, n, delay, 2, 0, 0.05, 0.0, "CI sample correctness without wall-time gates"))
    for index, case in enumerate(cases):
        row = run_case(binary, output, *case)
        if index % 10 == 0 or row.get("correctness_errors", 0) or row["status"] == "failed":
            print(f"{index+1}/{len(cases)} {row['case']}: {row['status']} misses={row.get('deadline_miss_fraction')} errors={row.get('correctness_errors')}", flush=True)
    if args.profile in ("all", "followup"):
        for p, k, n, reason in followups(results(output)):
            row = run_case(binary, output, "followup", p, k, n, seconds=60.0, warmup=0.5, rationale=reason)
            print(f"followup {row['case']}: misses={row.get('deadline_miss_fraction')}", flush=True)
    write_report(output)
    failed = [r for r in results(output) if r["status"] == "failed" or r.get("correctness_errors", 0) or r.get("client_exit_code", 0) or r.get("server_exit_code", 0)]
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
