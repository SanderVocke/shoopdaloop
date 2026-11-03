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

PHASES = ["build", "records", "first-stage", "network-boot", "warm-boot", "ui", "roundtrip", "watchdogs",
          "disk-faults", "metadata-damage", "network-faults", "refusals", "cleanup-refusal", "no-server"]


def worker(args):
    directory = args.directory.resolve()
    if args.phase == "build":
        build(directory, args.fetch, args.port_base)
        return
    q = Qualification(directory / "artifacts", directory / "private", directory / "evidence" / args.phase,
                      args.port_base, args.updater_port)
    if args.phase == "records":
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
    parser.add_argument("--phase", choices=[*PHASES, "original"], help=argparse.SUPPRESS)
    parser.add_argument("--port-base", type=int, default=16400)
    parser.add_argument("--updater-port", type=int, default=16554)
    args = parser.parse_args()
    if args.phase:
        worker(args)
        return
    directory = args.directory.resolve()
    directory.mkdir(parents=True, exist_ok=False)
    evidence = directory / "evidence"; evidence.mkdir()
    phases = [*PHASES, *(["original"] if args.with_original else [])]
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
        (evidence / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print("All Zynq network-deployment gates passed", flush=True)


if __name__ == "__main__":
    main()
