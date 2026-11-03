#!/usr/bin/env python3
"""Matched UDP/TCP DSP measurements, locally or with an explicitly selected SSH server."""
from __future__ import annotations

import argparse
import csv
import datetime
import difflib
import gzip
import hashlib
import ipaddress
import json
import os
from pathlib import Path, PurePosixPath
import queue
import random
import resource
import shlex
import shutil
import signal
import subprocess
import sys
import threading
import time

from sweep_remote_dsp import ROOT, digest, healthy
from compare_remote_dsp import check_distribution, timing_ms

CONFIGS = [(8, 1, 4), (8, 1, 8), (16, 4, 4), (16, 4, 8)]
TRANSPORTS = {"udp-gap": ("udp", "gap"), "udp-complete": ("udp", "complete"), "tcp-complete": ("tcp", "complete")}
PROBE = r'''
import hashlib,json,os,platform,resource,subprocess,sys
from pathlib import Path
binary=Path(sys.argv[1]).resolve(); key=Path(sys.argv[2]).resolve()
def command(args):
    try:
        p=subprocess.run(args,capture_output=True,text=True,timeout=15)
        return {"exit_code":p.returncode,"stdout":p.stdout.strip(),"stderr":p.stderr.strip()}
    except (OSError,subprocess.SubprocessError) as e: return {"error":str(e)}
def read(path):
    try: return Path(path).read_text().strip()
    except OSError: return None
probe=command(["chrt","--fifo","1",sys.executable,"-c","import os;print(os.sched_getscheduler(0));print(os.sched_getparam(0).sched_priority)"])
files=["/proc/cpuinfo","/proc/meminfo","/proc/self/cgroup","/proc/self/limits","/proc/interrupts","/proc/net/dev","/proc/sys/kernel/sched_rt_runtime_us","/proc/sys/kernel/sched_rt_period_us","/proc/net/snmp","/proc/net/netstat","/sys/firmware/acpi/platform_profile"]
files += [str(p) for pattern in ["/sys/devices/system/cpu/cpufreq/policy*/scaling_governor","/sys/devices/system/cpu/cpu*/topology/thread_siblings_list","/sys/class/power_supply/*/online","/sys/class/thermal/thermal_zone*/temp"] for p in Path('/').glob(pattern.lstrip('/'))]
print(json.dumps({"binary":str(binary),"binary_sha256":hashlib.sha256(binary.read_bytes()).hexdigest(),
    "credential_sha256":hashlib.sha256(key.read_bytes()).hexdigest(),"hostname":platform.node(),"platform":platform.platform(),
    "allowed_cpus":sorted(os.sched_getaffinity(0)),"nice":os.getpriority(os.PRIO_PROCESS,0),"fifo_probe":probe,
    "fifo_available":probe.get('exit_code')==0 and probe.get('stdout')=='1\n1',
    "git_revision":command(['git','rev-parse','HEAD']),"git_changes":command(['git','status','--porcelain']),
    "rustc":command(['rustc','--version']),"interfaces":command(['ip','-j','address']),"routes":command(['ip','-j','route']),
    "clock_ticks_per_second":os.sysconf('SC_CLK_TCK'),"host_files":{p:read(p) for p in files}}))
'''


def utc():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def cpus(text):
    result = []
    for part in text.split(","):
        ends = list(map(int, part.split("-")))
        result.extend(range(ends[0], ends[-1] + 1))
    return result


def parse_config(value):
    try:
        p, k, n = map(int, value.split(","))
        if p <= 0 or k <= 0 or n < k:
            raise ValueError()
        return p, k, n
    except ValueError as error:
        raise argparse.ArgumentTypeError("expected positive P,k,N with N >= k") from error


def server_command(args, command):
    if not args.server_ssh:
        return command
    if args.server_ssh.startswith("-"):
        raise ValueError("SSH destination must not start with '-'")
    shell = f"cd {shlex.quote(args.server_root)} && exec {shlex.join(command)}"
    return ["ssh", "-o", "BatchMode=yes", "-p", str(args.ssh_port), "--", args.server_ssh, shell]


def source_identity():
    paths = [ROOT / "scripts/benchmark_remote_dsp_transports.py", ROOT / "scripts/sweep_remote_dsp.py",
             ROOT / "scripts/compare_remote_dsp.py", ROOT / "scripts/tune_remote_dsp.py",
             ROOT / "scripts/tests/test_remote_dsp_transports.py"]
    paths += [p for name in ["shoop_dsp_host", "shoop_remote_dsp", "shoop_dsp", "shoop_transport_socket"]
              for p in (ROOT / "src/rust" / name).rglob("*") if p.suffix in [".rs", ".toml"]]
    paths += [ROOT / "Cargo.toml", ROOT / "Cargo.lock", ROOT / "flake.nix", ROOT / "flake.lock", ROOT / "rust-toolchain.toml"]
    return {str(p.relative_to(ROOT)): digest(p) for p in sorted(set(paths))}


def prefix(host, mode, cpu):
    mask = str(cpu) if mode == "fifo1" else ",".join(map(str, host["allowed_cpus"]))
    return ["taskset", "--cpu-list", mask, "chrt", "--fifo", "1"] if mode == "fifo1" else ["taskset", "--cpu-list", mask, "chrt", "--other", "0"]


def manifest(args, output):
    path = output / "manifest.json"
    identity = source_identity()
    binary_hash = digest(args.binary)
    arguments = {key: str(value) if isinstance(value, Path) else value for key, value in vars(args).items()
                 if key not in ["report_only", "publish"]}
    arguments = json.loads(json.dumps(arguments))
    if path.exists():
        old = json.loads(path.read_text())
        if old["source_files_sha256"] != identity or old["client"]["binary_sha256"] != binary_hash or old["arguments"] != arguments:
            raise RuntimeError("source/binary/arguments changed; use a fresh output directory")
        client = json.loads(subprocess.check_output([sys.executable, "-c", PROBE, str(args.binary), str(args.credential_file)], text=True, cwd=ROOT))
        server = json.loads(subprocess.check_output(server_command(args, [args.server_python, "-c", PROBE, args.server_binary, args.server_credential]), text=True, cwd=ROOT))
        if client.pop("credential_sha256") != server.pop("credential_sha256"):
            raise RuntimeError("client/server credential files differ")
        for role, current in [("client", client), ("server", server)]:
            if any(current[key] != old[role][key] for key in ["binary_sha256", "hostname", "platform", "allowed_cpus", "nice", "fifo_available"]):
                raise RuntimeError(f"{role} binary/host/permissions changed; use a fresh output directory")
        return old
    client = json.loads(subprocess.check_output([sys.executable, "-c", PROBE, str(args.binary), str(args.credential_file)], text=True, cwd=ROOT))
    server = json.loads(subprocess.check_output(server_command(args, [args.server_python, "-c", PROBE, args.server_binary, args.server_credential]), text=True, cwd=ROOT))
    if client.pop("credential_sha256") != server.pop("credential_sha256"):
        raise RuntimeError("client/server credential files differ")
    client_cpu = args.client_cpu if args.client_cpu is not None else (2 if 2 in client["allowed_cpus"] else client["allowed_cpus"][0])
    server_cpu = args.server_cpu if args.server_cpu is not None else next((c for c in [4, *server["allowed_cpus"]] if c in server["allowed_cpus"] and (args.server_ssh or c != client_cpu)), None)
    if client_cpu not in client["allowed_cpus"] or server_cpu not in server["allowed_cpus"] or (not args.server_ssh and client_cpu == server_cpu):
        raise RuntimeError("select allowed CPUs, distinct for a local process pair")
    modes = ["other"]
    if client["fifo_available"] and server["fifo_available"]:
        modes.append("fifo1")
    configurations = args.config or CONFIGS
    if args.profile == "smoke" and not args.config:
        configurations = [(16, 4, 8)]
    cases = []
    for suite, seconds, repeats in [("screen", 0.05 if args.profile == "smoke" else args.screen_seconds, 1 if args.profile == "smoke" else args.repetitions),
                                     ("followup", args.followup_seconds, 1)]:
        if suite == "followup" and args.profile != "all":
            continue
        stage = [(suite, list(config), variant, mode, repeat, seconds) for config in configurations
                 for variant in TRANSPORTS for mode in modes for repeat in range(repeats)]
        random.Random(args.seed + (suite == "followup")).shuffle(stage)
        cases.extend(stage)
    if args.profile == "all" and args.stall_us:
        stall_config = next((c for c in configurations if tuple(c) == (16, 4, 8)),
                            next((c for c in configurations if c[1] > 1), configurations[0]))
        stage = [("stall", list(stall_config), variant, mode, 0, 1.0) for variant in TRANSPORTS for mode in modes]
        random.Random(args.seed + 2).shuffle(stage)
        cases.extend(stage)
    data = {"started_utc": utc(), "arguments": arguments, "client": client, "server": server,
            "client_cpu": client_cpu, "server_cpu": server_cpu, "source_files_sha256": identity,
            "modes": modes, "cases": cases, "remote": bool(args.server_ssh),
            "limitations": "No automatic network/firewall/power changes. No audio device. Localhost measurements are not physical-LAN measurements. No builds/tests during capture. Host counters include unrelated work."}
    path.write_text(json.dumps(data, indent=2) + "\n")
    tracked = list(identity)
    patch = subprocess.check_output(["git", "diff", "HEAD", "--", *tracked], cwd=ROOT, text=True)
    untracked = subprocess.check_output(["git", "ls-files", "--others", "--exclude-standard", "--", *tracked], cwd=ROOT, text=True).splitlines()
    for name in untracked:
        patch += "".join(difflib.unified_diff([], (ROOT / name).read_text().splitlines(keepends=True), fromfile="/dev/null", tofile="b/" + name))
    (output / "implementation.patch").write_text(patch)
    return data


def case_name(case):
    suite, (p, k, n), variant, mode, repeat, _ = case
    return f"{suite}-p{p}-k{k}-n{n}-{variant}-{mode}-r{repeat}"


def stop_child(child):
    if child is None:
        return
    if child.poll() is None:
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    child.wait()


def run_case(args, output, machine, case):
    ident = case_name(case)
    path = output / f"{ident}.json"
    if path.exists():
        return json.loads(path.read_text())
    suite, (p, k, n), variant, mode, repeat, seconds = case
    transport, policy = TRANSPORTS[variant]
    temporary = output / f"{ident}.json.tmp"
    client_args = ["--credential-file", str(args.credential_file), "--transport", transport, "--input-policy", policy,
                   "--catch-up", "--polling", "yield", "--packet-frames", str(p), "--multiplier", str(k), "--allowance", str(n),
                   "--reorder-packets", "64", "--gap-timeout-us", "200", "--sample-rate", str(args.sample_rate),
                   "--channels", str(args.channels), "--delay", str(args.delay), "--mtu", str(args.mtu),
                   "--seconds", str(seconds), "--warmup", str(args.warmup), "--seed", str(args.seed),
                   "--source-stall-us", str(args.stall_us if suite == "stall" else 0), "--output", str(temporary)]
    server_args = [args.server_binary, "serve", "--once", "--credential-file", args.server_credential, "--polling", "yield",
                   "--bind", args.server_bind, "--data-port", str(args.data_port)]
    if args.server_ssh or not args.server_bind.startswith("127.") and not args.server_bind.startswith("[::1]"):
        server_args.append("--lan")
    server = server_command(args, ["timeout", "--signal=TERM", f"{seconds + args.warmup + 45}s", *prefix(machine["server"], mode, machine["server_cpu"]), *server_args])
    row = {"case": ident, "suite": suite, "variant": variant, "mode": mode, "repetition": repeat, "started_utc": utc(),
           "requested": {"P":p,"k":k,"N":n,"A":args.delay,"channels":args.channels,"seconds":seconds,"warmup":args.warmup},
           "server_command": server}
    preflight = [str(args.binary), "client", "--validate-only", "--endpoint", "127.0.0.1:1", *client_args]
    subprocess.run(preflight, check=True, capture_output=True, text=True, timeout=10, cwd=ROOT)
    validated = json.loads(temporary.read_text())
    temporary.unlink()
    if validated["status"] != "admitted":
        row.update(validated)
        row["client_command"] = preflight
        path.write_text(json.dumps(row, indent=2) + "\n")
        return row
    before = resource.getrusage(resource.RUSAGE_CHILDREN)
    wall = time.monotonic()
    with (output / f"{ident}.server.log").open("w") as log:
        service = subprocess.Popen(server, stdout=subprocess.PIPE, stderr=log, text=True, cwd=ROOT, process_group=0)
        client = None
        try:
            ready = queue.Queue()
            threading.Thread(target=lambda: ready.put(service.stdout.readline()), daemon=True).start()
            endpoint = json.loads(ready.get(timeout=20))["control_endpoint"]
            command = prefix(machine["client"], mode, machine["client_cpu"]) + [str(args.binary), "client", "--endpoint", endpoint, *client_args]
            row["client_command"] = command
            client = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, cwd=ROOT, process_group=0)
            stdout, stderr = client.communicate(timeout=seconds + args.warmup + 30)
            (output / f"{ident}.client.log").write_text(stdout + stderr)
            row["client_exit_code"] = client.returncode
            service.wait(timeout=12)
            row["server_exit_code"] = service.returncode
            row.update(json.loads(temporary.read_text()))
            temporary.unlink()
        except Exception as error:
            row.update(status="failed", error=f"{type(error).__name__}: {error}")
        finally:
            for child in [client, service]:
                stop_child(child)
    elapsed = time.monotonic() - wall
    after = resource.getrusage(resource.RUSAGE_CHILDREN)
    row["resources"] = {"wall_seconds": elapsed, "controller_children_cpu_seconds": after.ru_utime + after.ru_stime - before.ru_utime - before.ru_stime,
                        "cpu_scope": "requester plus SSH, excludes remote server" if args.server_ssh else "both complete process lifetimes"}
    row["finished_utc"] = utc()
    path.write_text(json.dumps(row, indent=2) + "\n")
    return row


def read_rows(output):
    paths = sorted(output.glob("screen-*.json")) + sorted(output.glob("followup-*.json")) + sorted(output.glob("stall-*.json"))
    if paths:
        return [json.loads(path.read_text()) for path in paths]
    with gzip.open(output / "results.jsonl.gz", "rt") as handle:
        return [json.loads(line) for line in handle if line.strip()]


def audit_rows(rows, machine):
    expected_cases = {case_name(case): case for case in machine["cases"]}
    expected = set(expected_cases)
    assert len({r["case"] for r in rows}) == len(rows)
    assert {r["case"] for r in rows} <= expected
    failed = []
    for row in rows:
        if row.get("status") != "measured" or row.get("client_exit_code", 0) or row.get("server_exit_code", 0):
            failed.append(row["case"])
            continue
        declared = expected_cases[row["case"]]
        requested = row["requested"]
        assert (row["suite"], row["variant"], row["mode"], row["repetition"]) == (declared[0], declared[2], declared[3], declared[4])
        assert [requested[key] for key in ["P", "k", "N"]] == declared[1]
        assert requested["seconds"] == declared[5]
        assert [row["config"][key] for key in ["packet_frames", "multiplier", "allowance"]] == declared[1]
        assert row["config"]["sample_rate"] == machine["arguments"]["sample_rate"]
        assert row["measured_packets"] == row["on_time"] + row["late"] + row["missing"]
        assert row["verified_samples"] + row["unverified_samples"] == (row["on_time"] + row["late"]) * row["config"]["packet_frames"] * row["config"]["channels"]
        for name in ["rtt", "sender_lateness", "batch_turnaround", "dsp_processing"]:
            check_distribution(row[name])
        assert row["sender_lateness"]["count"] == row["sent"]
        assert row["rtt"]["count"] <= min(row["sent"], row["on_time"] + row["late"])
        assert row["batch_turnaround"]["count"] <= row["dsp_processing"]["count"] <= row["measured_packets"] // row["config"]["multiplier"]
        if not row["server"].get("diagnostics_truncated", False):
            assert sum(mask.bit_count() for _, mask in row.get("concealment", [])) == row["server"]["concealed_packets"]
        assert abs(row["deadline_miss_fraction"] - (row["late"] + row["missing"]) / row["measured_packets"]) < 1e-12
        transport, policy = TRANSPORTS[row["variant"]]
        assert (row["transport"], row["input_policy"], row["catch_up"]) == (transport, policy, True)
        assert row["source_stall_us"] == (machine["arguments"]["stall_us"] if row["suite"] == "stall" else 0)
        assert row["source_stalled"] == (row["source_stall_us"] > 0)
        if policy == "complete":
            assert row["server"]["concealed_packets"] == 0
        if transport == "tcp":
            assert row["client_audio"]["nodelay"] and row["server_audio"]["nodelay"]
        for role, field in [("client", "client_runtime"), ("server", "worker_runtime")]:
            actual = row[field]
            assert actual["scheduler"] == (1 if row["mode"] == "fifo1" else 0)
            assert actual["realtime_priority"] == (1 if row["mode"] == "fifo1" else 0)
            assert actual["nice"] == machine[role]["nice"]
            assert cpus(actual["affinity"]) == ([machine[f"{role}_cpu"]] if row["mode"] == "fifo1" else machine[role]["allowed_cpus"])
        if row["correctness_errors"] or row["unverified_samples"]:
            failed.append(row["case"])
    return {"records":len(rows),"expected_records":len(expected),"complete":len(rows)==len(expected),"infrastructure_or_verification_failures":failed,
            "measured_packets":sum(r.get("measured_packets",0) for r in rows),"verified_samples":sum(r.get("verified_samples",0) for r in rows),
            "incorrect_samples":sum(r.get("correctness_errors",0) for r in rows),"unverified_samples":sum(r.get("unverified_samples",0) for r in rows),
            "late":sum(r.get("late",0) for r in rows),"missing":sum(r.get("missing",0) for r in rows),
            "concealed":sum(r.get("server",{}).get("concealed_packets",0) for r in rows),
            "retired":sum(bool(r.get("server",{}).get("retired")) for r in rows),
            "checks":["unique expected cases","packet/sample counts","histogram counts/means/quantile bins","actual per-thread policy/priority/affinity","complete-input semantics","TCP_NODELAY"]}


def report(output, publish=None):
    machine = json.loads((output / "manifest.json").read_text())
    rows = read_rows(output)
    audit = audit_rows(rows, machine)
    payload = "".join(json.dumps(r,separators=(",",":"))+"\n" for r in rows).encode()
    (output / "results.jsonl.gz").write_bytes(gzip.compress(payload, mtime=0))
    audit["decompressed_sha256"] = hashlib.sha256(payload).hexdigest()
    (output / "audit.json").write_text(json.dumps(audit,indent=2)+"\n")
    fields = ["case","suite","variant","mode","P","k","N","audio_latency_ms","seconds","status","late","missing","concealed","retired","verified_samples","batch_p99_us","rtt_p99_us","sender_p99_us","transport_error"]
    with (output / "summary.csv").open("w",newline="") as handle:
        writer = csv.DictWriter(handle,fieldnames=fields,lineterminator="\n"); writer.writeheader()
        for row in rows:
            req=row["requested"]
            record=dict(row)|req
            record.update(audio_latency_ms=timing_ms(req["P"],req["k"],req["N"],req["A"],machine["arguments"]["sample_rate"])["audio_latency"],
                          concealed=row.get("server",{}).get("concealed_packets"),retired=row.get("server",{}).get("retired"))
            for field,source in [("batch_p99_us","batch_turnaround"),("rtt_p99_us","rtt"),("sender_p99_us","sender_lateness")]:
                value=row.get(source,{}).get("p99_ns"); record[field]=None if value is None else value/1000
            writer.writerow({key:record.get(key) for key in fields})
    lines=["# Matched audio-transport measurements","",
           f"{len(rows)} of {audit['expected_records']} expected records; {audit['verified_samples']:,} verified samples; {audit['incorrect_samples']} incorrect; {audit['unverified_samples']} unverified.","",
           "All cases use yielding, catch-up without intentional overdue-source skips, identical source deadlines and DSP wire frames. "
           "UDP-gap retains the 200 us gap policy; UDP-complete and TCP-complete only process complete input blocks. "
           "TCP uses a separate authenticated, length-framed connection with TCP_NODELAY at both ends. "
           "Audio latency is ((N+1)*P+A)/fs, excluding audio hardware and extra endpoint buffers. "
           "Counts are output misses / concealed input; input counts include warm-up. No clock synchronization is needed.","",
           "## Fixed long comparisons","",
           "| P/k/N | Audio ms | Scheduling | UDP-gap | UDP-complete | TCP-complete |","| --- | ---: | --- | ---: | ---: | ---: |"]
    configs=sorted({tuple(r["requested"][key] for key in ["P","k","N"]) for r in rows})
    for c in configs:
        for mode in machine["modes"]:
            values=[]
            for variant in TRANSPORTS:
                group=[r for r in rows if r["suite"]=="followup" and r["variant"]==variant and r["mode"]==mode and tuple(r["requested"][key] for key in ["P","k","N"])==c]
                if len(group)!=1 or group[0]["status"]!="measured": values.append("not measured"); continue
                r=group[0]; suffix=" (retired)" if r["server"]["retired"] else ""
                values.append(f"{r['late']+r['missing']:,} / {r['server']['concealed_packets']:,}{suffix}")
            latency=timing_ms(*c,delay=machine["arguments"]["delay"],sample_rate=machine["arguments"]["sample_rate"])["audio_latency"]
            lines.append(f"| {'/'.join(map(str,c))} | {latency:.3f} | {mode} | "+" | ".join(values)+" |")
    lines += ["","## Every condition, with all repetitions retained","",
              "| Suite | P/k/N | Variant | Scheduling | Runs | Misses / slots | Concealed | Retired | Healthy |",
              "| --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: |"]
    keys=sorted({(r["suite"],tuple(r["requested"][key] for key in ["P","k","N"]),r["variant"],r["mode"]) for r in rows})
    for suite,c,variant,mode in keys:
        group=[r for r in rows if (r["suite"],tuple(r["requested"][key] for key in ["P","k","N"]),r["variant"],r["mode"])==(suite,c,variant,mode)]
        good=[r for r in group if r["status"]=="measured"]
        lines.append(f"| {suite} | {'/'.join(map(str,c))} | {variant} | {mode} | {len(group)} | {sum(r['late']+r['missing'] for r in good):,} / {sum(r['measured_packets'] for r in good):,} | {sum(r['server']['concealed_packets'] for r in good)} | {sum(r['server']['retired'] for r in good)} | {sum(healthy(r) and not r.get('transport_error') for r in good)} |")
    lines += ["","The manifest preserves both endpoint inventories, binary/source identities, permissions and expected cases. "
              "The archive retains every outcome, histogram, native per-thread scheduling observation, socket/queue counter, command and timestamp. "
              "Child CPU cost covers both processes locally; over SSH it excludes the remote server and is explicitly labeled. "
              "Host network counters include unrelated traffic; these are not per-stream packet-loss measurements."]
    (output / "report.md").write_text("\n".join(lines)+"\n")
    if publish:
        publish.mkdir(parents=True,exist_ok=True)
        for name in ["manifest.json","implementation.patch","results.jsonl.gz","summary.csv","audit.json","report.md","finished.json"]:
            if (output/name).exists() and output/name != publish/name:
                shutil.copy2(output/name,publish/name)
    return audit


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary",type=Path,default=ROOT/"target/release/shoop_dsp_host")
    parser.add_argument("--output",type=Path,default=ROOT/"artifacts/remote-dsp-transports")
    parser.add_argument("--credential-file",type=Path)
    parser.add_argument("--server-ssh")
    parser.add_argument("--server-root")
    parser.add_argument("--server-binary")
    parser.add_argument("--server-credential")
    parser.add_argument("--server-python",default="python3")
    parser.add_argument("--ssh-port",type=int,default=22)
    parser.add_argument("--server-bind",default="127.0.0.1:0")
    parser.add_argument("--data-port",type=int,default=0)
    parser.add_argument("--client-cpu",type=int)
    parser.add_argument("--server-cpu",type=int)
    parser.add_argument("--config",type=parse_config,action="append")
    parser.add_argument("--profile",choices=["all","screen","smoke"],default="all")
    parser.add_argument("--repetitions",type=int,default=3)
    parser.add_argument("--screen-seconds",type=float,default=2)
    parser.add_argument("--followup-seconds",type=float,default=60)
    parser.add_argument("--warmup",type=float,default=0.5)
    parser.add_argument("--sample-rate",type=int,default=48000)
    parser.add_argument("--channels",type=int,default=2)
    parser.add_argument("--delay",type=int,default=0)
    parser.add_argument("--mtu",type=int,default=1500)
    parser.add_argument("--seed",type=int,default=20261104)
    parser.add_argument("--stall-us",type=int,default=2000)
    parser.add_argument("--report-only",action="store_true")
    parser.add_argument("--publish",type=Path)
    args=parser.parse_args()
    args.output=args.output.resolve(); args.output.mkdir(parents=True,exist_ok=True)
    if args.report_only:
        audit=report(args.output,args.publish)
        print(json.dumps(audit,indent=2)); return int(bool(audit["infrastructure_or_verification_failures"]))
    if not sys.platform.startswith("linux"):
        parser.error("capture requires Linux taskset/chrt; report-only does not run measurements")
    if not (0<args.screen_seconds<=120 and 0<args.followup_seconds<=120 and 0<=args.warmup<=10 and args.repetitions>0 and 0<=args.stall_us<=1000000):
        parser.error("invalid duration/repetition count")
    args.binary=args.binary.resolve()
    args.credential_file=(args.credential_file or args.output/"credential.key").resolve()
    if args.server_ssh:
        bind_ip = ipaddress.ip_address(args.server_bind.rsplit(":", 1)[0].strip("[]"))
        if not all([args.server_root,args.server_binary,args.server_credential]) or bind_ip.is_loopback or bind_ip.is_unspecified:
            parser.error("SSH mode requires server root/binary/credential and a concrete non-loopback server-bind")
        if not PurePosixPath(args.server_root).is_absolute():
            parser.error("server-root must be absolute")
        args.server_binary=str(PurePosixPath(args.server_root)/args.server_binary)
        args.server_credential=str(PurePosixPath(args.server_root)/args.server_credential)
        if not args.credential_file.exists(): parser.error("SSH mode requires an existing shared local credential")
    else:
        args.server_binary=str(Path(args.server_binary or args.binary).resolve())
        args.server_credential=str(Path(args.server_credential or args.credential_file).resolve())
        if not args.credential_file.exists():
            subprocess.run([str(args.binary),"generate-credential",str(args.credential_file)],check=True)
    machine=manifest(args,args.output)
    for index,case in enumerate(machine["cases"]):
        row=run_case(args,args.output,machine,case)
        print(json.dumps({"progress":f"{index+1}/{len(machine['cases'])}",**{key:row.get(key) for key in ["case","status","late","missing","correctness_errors","error"]}}),flush=True)
        if row.get("status") != "measured" or row.get("client_exit_code", 0) or row.get("server_exit_code", 0) or row.get("correctness_errors", 0) or row.get("unverified_samples", 0):
            report(args.output, args.publish)
            return 1
    if source_identity() != machine["source_files_sha256"]:
        raise RuntimeError("source changed during capture")
    if not (args.output/"finished.json").exists():
        client = json.loads(subprocess.check_output([sys.executable, "-c", PROBE, str(args.binary), str(args.credential_file)], text=True, cwd=ROOT))
        server = json.loads(subprocess.check_output(server_command(args, [args.server_python, "-c", PROBE, args.server_binary, args.server_credential]), text=True, cwd=ROOT))
        client.pop("credential_sha256"); server.pop("credential_sha256")
        if client["binary_sha256"] != machine["client"]["binary_sha256"] or server["binary_sha256"] != machine["server"]["binary_sha256"]:
            raise RuntimeError("binary changed during capture")
        (args.output/"finished.json").write_text(json.dumps({"finished_utc":utc(),"client":client,"server":server},indent=2)+"\n")
    audit=report(args.output,args.publish)
    print(json.dumps(audit,indent=2))
    return int(bool(audit["infrastructure_or_verification_failures"]))


if __name__=="__main__":
    raise SystemExit(main())
