#!/usr/bin/env python3
"""Execute all network-deployment gates with private cards and public evidence."""
import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

from build_artifacts import build, PLATFORM
from emulator import BootQemu, ROOT
from qualification import Qualification

PHASES = ["build", "records", "first-stage", "network-boot", "warm-boot", "ui", "roundtrip", "defaults", "watchdogs",
          "disk-faults", "metadata-damage", "network-faults", "refusals", "cleanup-refusal", "no-server"]


def audit_public_output(directory):
    """Quarantine a secret-bearing file before the always-run artifact upload."""
    credential = directory / "private/credential"
    if not credential.exists():
        return []
    secret = credential.read_bytes()
    patterns = [b"-----BEGIN PRIVATE KEY-----", b"-----BEGIN RSA PRIVATE KEY-----"]
    if len(secret) == 32:
        patterns.append(secret)
    rejected = []
    for root in [directory / "artifacts", directory / "evidence"]:
        for path in root.rglob("*"):
            if not path.is_file():
                continue
            found, tail = False, b""
            with path.open("rb") as stream:
                while chunk := stream.read(1024 * 1024):
                    data = tail + chunk
                    if any(pattern in data for pattern in patterns):
                        found = True; break
                    tail = data[-64:]
            if found:
                relative = str(path.relative_to(directory))
                quarantine = directory / "private/quarantine" / path.relative_to(directory)
                quarantine.parent.mkdir(parents=True, exist_ok=True)
                path.rename(quarantine)
                rejected.append(relative)
    return rejected


def worker(args):
    directory = args.directory.resolve()
    if args.native_coverage:
        from native_coverage import configure_runtime
        configure_runtime(directory)
    if args.phase == "build":
        build(directory, args.fetch, args.port_base, args.native_coverage)
        return
    q = Qualification(directory / "artifacts", directory / "private", directory / "evidence" / args.phase,
                      args.port_base, args.updater_port)
    if args.phase == "records":
        import tempfile
        with tempfile.TemporaryDirectory() as temporary:
            test = Path(temporary)
            for folder in ["private", "artifacts", "evidence"]:
                (test / folder).mkdir()
            # Fixed public marker exercises chunk-boundary detection.
            marker = bytes([165]) * 32
            (test / "private/credential").write_bytes(marker)
            (test / "artifacts/clean").write_bytes(b"public artifact")
            (test / "artifacts/leak").write_bytes(bytes(1024 * 1024 - 16) + marker)
            (test / "evidence/leak").write_bytes(b"-----BEGIN PRIVATE KEY-----")
            assert sorted(audit_public_output(test)) == ["artifacts/leak", "evidence/leak"]
            assert (test / "artifacts/clean").exists() and not (test / "artifacts/leak").exists()
            assert (test / "private/quarantine/artifacts/leak").exists()
        subprocess.run([sys.executable, PLATFORM / "deployment/tests/records.py"], check=True)
        executable = directory / "private/boot-state-tests"
        subprocess.run(["cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-fsanitize=address,undefined",
                        "-I", PLATFORM / "deployment", PLATFORM / "deployment/tests/boot_state.c",
                        PLATFORM / "deployment/boot_state.c", "-o", executable], check=True)
        subprocess.run([executable], check=True, env={**os.environ, "ASAN_OPTIONS": "detect_leaks=0"})
    elif args.phase == "first-stage":
        import re
        image = q.factory("first-stage")
        with BootQemu(directory / "artifacts/loader/u-boot-spl", image, q.evidence / "probe",
                      port_base=args.port_base, updater_port=args.updater_port, first_stage=True) as vm:
            time.sleep(3)
            vm.qmp("stop")
            registers = vm.qmp("human-monitor-command", {"command-line": "info registers"})
            pc = int(re.search(r"R15=([0-9a-fA-F]+)", registers)[1], 16)
            memory, cpus = vm.qmp("query-memory-size-summary"), vm.qmp("query-cpus-fast")
            assert memory["base-memory"] == 1 << 30 and len(cpus) == 1
            assert 0x400 <= pc < 0x430, "Reference SPL no longer matches the documented PS-init polling limitation"
            result = {"entry": "reference Zynq SPL ELF", "bootrom_executed": False, "uboot_handoff": False,
                      "pc": pc, "limitation": "reference PS-init SLCR 0xf8000b74 polling", "memory": memory, "cpus": cpus}
            (q.evidence / "coverage.json").write_text(json.dumps(result, indent=2) + "\n")
    elif args.phase in {"network-boot", "warm-boot", "ui"}:
        import probe
        network = args.phase == "network-boot"
        image = q.factory(args.phase, mode=2 if network else 0)
        tftp = None
        if network:
            import shutil
            tftp = directory / "artifacts/tftp"; tftp.mkdir()
            shutil.copyfile(directory / "artifacts/firmware-a.itb", tftp / "shoop.itb")
        probe.run(q.loader, image, q.profile, q.evidence / "clients", "firmware-a", ui=args.phase == "ui",
                  warm=args.phase == "warm-boot", tftp=tftp, port_base=args.port_base, updater_port=args.updater_port)
    elif args.phase == "native-coverage":
        from native_coverage import report
        report(directory)
    elif args.phase == "original":
        subprocess.run([sys.executable, PLATFORM / "suite.py", *(["--fetch"] if args.fetch else [])], cwd=ROOT, check=True)
    else:
        getattr(q, args.phase.replace("-", "_"))()


def run_phase(command, log, timeout):
    process = subprocess.Popen(command, cwd=ROOT, stdout=log, stderr=log, start_new_session=True)
    try:
        return process.wait(timeout=timeout)
    finally:
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            pass
        # An interrupted worker must not leave a VM/NBD client running.
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline:
            try:
                os.killpg(process.pid, 0)
            except ProcessLookupError:
                break
            time.sleep(.05)
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait(timeout=5)


def main():
    signal.signal(signal.SIGTERM, lambda number, frame: sys.exit(128 + number))
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, default=ROOT / "work/zynq/deployment-suite")
    parser.add_argument("--fetch", action="store_true")
    parser.add_argument("--with-original", action="store_true", help="Also execute the original complete firmware suite")
    parser.add_argument("--native-coverage", action="store_true", help="Collect real QEMU deployment CLI coverage with cargo-llvm-cov")
    parser.add_argument("--phase", choices=[*PHASES, "original", "native-coverage"], help=argparse.SUPPRESS)
    parser.add_argument("--port-base", type=int, default=16400)
    parser.add_argument("--updater-port", type=int, default=16554)
    args = parser.parse_args()
    if args.phase:
        worker(args)
        return
    directory = args.directory.resolve()
    directory.mkdir(parents=True, exist_ok=False)
    evidence = directory / "evidence"; evidence.mkdir()
    phases = [*PHASES, *(["original"] if args.with_original else []), *(["native-coverage"] if args.native_coverage else [])]
    result = {"passed": False, "ram_bytes": 1 << 30, "card_bytes": 512 << 20, "expected_phases": phases, "phases": []}
    try:
        for name in phases:
            print("Running deployment phase:", name, flush=True)
            phase = {"name": name, "passed": False}; result["phases"].append(phase)
            started = time.monotonic()
            path = evidence / (name + ".log")
            with path.open("w") as log:
                status = run_phase([sys.executable, Path(__file__).resolve(), "--directory", directory, "--phase", name,
                                    "--port-base", str(args.port_base), "--updater-port", str(args.updater_port),
                                    *(["--fetch"] if args.fetch else [])], log, 2400 if name in {"build", "original"} else 1800)
            phase.update(passed=status == 0, seconds=round(time.monotonic() - started, 2))
            if status:
                raise RuntimeError(name + " failed:\n" + path.read_text()[-12000:])
        result["passed"] = True
    finally:
        rejected = audit_public_output(directory)
        result["public_output_audit"] = {"passed": not rejected, "quarantined_files": rejected}
        result["passed"] &= not rejected
        (evidence / "result.json").write_text(json.dumps(result, indent=2) + "\n")
        if rejected:
            raise RuntimeError("Secret-bearing public output was quarantined; refusing to publish those files")
    print("All Zynq network-deployment gates passed", flush=True)


if __name__ == "__main__":
    main()
