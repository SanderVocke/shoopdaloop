#!/usr/bin/env python3
"""Reproduce graph/link correctness and optional paced/Carla demonstrations."""
from __future__ import annotations

import argparse
import hashlib
import json
import platform
import queue
from pathlib import Path
import subprocess
import sys
import threading
import time


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--carla", action="store_true")
    parser.add_argument("--paced", action="store_true")
    args = parser.parse_args()
    binary = args.binary.resolve()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if (output / "manifest.json").exists():
        parser.error("use a fresh output directory to preserve the previous run")
    key = output / "credential.key"
    if not key.exists():
        subprocess.run([str(binary), "generate-credential", str(key)], check=True, timeout=10)
    root = Path(__file__).resolve().parents[1]
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
    dirty = bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=root, text=True).strip())
    records = []

    def run(name: str, options: list[str], processor: str | None = None) -> dict:
        server = None
        server_log = None
        command = [str(binary), "run", *options]
        destination = output / f"{name}.json"
        started = time.monotonic()
        try:
            if processor:
                server_log = (output / f"{name}-server.log").open("w")
                server = subprocess.Popen([str(binary), "serve", "--once", "--processor", processor,
                                           "--credential-file", str(key)], stdout=subprocess.PIPE,
                                          stderr=server_log, text=True)
                assert server.stdout is not None
                lines = queue.Queue()
                threading.Thread(target=lambda: lines.put(server.stdout.readline()), daemon=True).start()
                ready = json.loads(lines.get(timeout=10))
                command += ["--endpoint", ready["control_endpoint"], "--credential-file", str(key)]
            command += ["--output", str(destination)]
            result = subprocess.run(command, capture_output=True, text=True, timeout=45)
            (output / f"{name}-client.log").write_text(result.stdout + result.stderr)
            report = json.loads(destination.read_text()) if destination.exists() else None
            record = {"case": name, "command": command, "exit_code": result.returncode,
                      "elapsed_seconds": time.monotonic() - started, "report": report}
            records.append(record)
            return record
        except (OSError, ValueError, KeyError, queue.Empty, subprocess.TimeoutExpired) as error:
            record = {"case": name, "command": command, "exit_code": 124,
                      "elapsed_seconds": time.monotonic() - started,
                      "error": str(error), "report": None}
            records.append(record)
            return record
        finally:
            if server:
                try:
                    server.wait(timeout=8)
                except subprocess.TimeoutExpired:
                    server.kill()
                    server.wait()
            if server_log:
                server_log.close()

    run("reference", ["--backend", "reference", "--quantum", "3", "--dsp-frames", "5",
                      "--callback-frames", "7", "--frames", "1000", "--delay", "17"])
    for callback in (7, 96):
        run(f"udp-{callback}", ["--callback-frames", str(callback), "--frames", "4096",
                               "--delay", "67"], "delay-copy")
    if args.carla:
        project = output / "gain.xml"
        project.write_text((root / "src/rust/shoop_engine/test_data/carla_legacy_rack_loaded_project.xml")
                           .read_text().replace("<Value>1</Value>", "<Value>0.5</Value>", 1))
        saved = output / "saved-state.txt"
        for name, state in (("carla", project), ("carla-restored", saved)):
            options = ["--processor", "carla", "--state-file", str(state), "--expected-gain", "0.5",
                       "--warmup-frames", "4096", "--frames", "8192"]
            if name == "carla":
                options += ["--state-output", str(saved)]
            run(name, options, "carla")
    if args.paced:
        run("udp-paced", ["--mode", "paced", "--callback-frames", "64", "--frames", "16384"], "delay-copy")
    manifest = {"source_revision": revision, "source_dirty": dirty, "platform": platform.platform(),
                "python": sys.version.split()[0], "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                "cases": records, "paced_is_not_a_latency_guarantee": True}
    (output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    failures = [r["case"] for r in records if r["exit_code"] != 0 or not r["report"]
                or r["report"]["correctness_errors"] != 0 or r["report"]["verified_samples"] == 0]
    print(json.dumps({"runs": len(records), "failures": failures, "manifest": str(output / "manifest.json")}))
    if failures:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
