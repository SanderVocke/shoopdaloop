#!/usr/bin/env python3
"""Execute all firmware gates; every VM is owned by a bounded child harness."""
import argparse
import json
import os
import signal
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]


def run_phase(command, environment, log):
    process = subprocess.Popen(command, cwd=ROOT, env=environment, stdout=log, stderr=log,
                               start_new_session=True)
    try:
        return process.wait(timeout=1500)
    finally:
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            pass
        deadline = time.monotonic() + 5
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


def interrupted(number, _frame):
    raise SystemExit(128 + number)


def main():
    signal.signal(signal.SIGTERM, interrupted)
    parser = argparse.ArgumentParser()
    parser.add_argument("--fetch", action="store_true")
    args = parser.parse_args()
    directory = ROOT / "work/zynq/suite"
    directory.mkdir(parents=True, exist_ok=True)
    phases = [("echo-build", "build.py", ["--fetch"] if args.fetch else []),
              ("echo", "qemu.py", []), ("echo-canary", "qemu.py", []),
              ("production", "integration.py", []), ("ui", "integration.py", ["--ui"]),
              ("recovery", "integration.py", ["--recovery"]),
              ("fixture", "integration.py", ["--fixture"])]
    summary = {"ram_bytes": 1 << 30, "phases": []}
    try:
        for name, script, arguments in phases:
            print(f"Running {name}", flush=True)
            environment = dict(os.environ)
            if name == "echo-canary":
                environment["SHOOP_ZYNQ_CORRUPT_ECHO"] = "1"
            start = time.monotonic()
            phase = {"name": name, "passed": False}
            summary["phases"].append(phase)
            log_path = directory / (name + ".log")
            with log_path.open("w") as log:
                result = run_phase([sys.executable, str(Path(__file__).parent / script), *arguments], environment, log)
            if name == "echo-canary":
                passed = result != 0 and "Echo content mismatch" in log_path.read_text()
            else:
                passed = result == 0
            phase.update(passed=passed, seconds=round(time.monotonic()-start, 2))
            if not passed:
                raise RuntimeError(f"{name} failed:\n{log_path.read_text()[-12000:]}")
    finally:
        (directory / "result.json").write_text(json.dumps(summary, indent=2) + "\n")
    print("All three QEMU firmware gates passed", flush=True)


if __name__ == "__main__":
    main()
