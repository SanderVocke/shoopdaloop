#!/usr/bin/env python3
"""Run the independent primitive loop host in native and real Web Audio contexts."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import urllib.request

import run_rt_runtime_hosts as harness

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "tests/loop_engine_host"
ASSETS = ROOT / "target/loop-engine-host"


def compare(result, native):
    if result.get("format") != "LLH1":
        raise RuntimeError("unknown loop host report format")
    if result.get("cross_origin_isolated") is not False:
        raise RuntimeError("missing non-isolated browser evidence")
    if [report["mode"] for report in result["reports"]] != ["offline", "live"]:
        raise RuntimeError("both actual contexts are required")
    for report in result["reports"]:
        expected_audio = [[sample[channel] for sample in native["samples"]] + [0] * (256 - len(native["samples"])) for channel in range(2)]
        if report["audio"] != expected_audio:
            raise RuntimeError(f"{report['mode']} Web Audio differs from native scalar oracle")
        data = bytes(report["midiBytes"])
        midi, cursor = [], 0
        while cursor < len(data):
            if cursor + 12 > len(data):
                raise RuntimeError("truncated MIDI journal header")
            at, length = struct.unpack_from("<QI", data, cursor)
            cursor += 12
            if not length or cursor + length > len(data):
                raise RuntimeError("truncated MIDI journal packet")
            midi.append([at, list(data[cursor:cursor + length])])
            cursor += length
        if midi != native["midi"]:
            raise RuntimeError("complete MIDI content/timing differs from native")
        transitions = []
        for response in report["responses"]:
            packet = bytes(response)
            if len(packet) != 96 or packet[:6] != b"LLR1\x01\x00":
                raise RuntimeError("invalid response journal")
            if packet[6] == 2:
                if struct.unpack_from("<I", packet, 28)[0] != 4 or packet[7] != 1 or struct.unpack_from("<I", packet, 72)[0] != 0:
                    raise RuntimeError("non-applied browser transition")
                requested, effective, delivered, terminal = struct.unpack_from("<QQQQ", packet, 40)
                if requested != effective:
                    raise RuntimeError("unexpected late scheduling")
                transitions.append([requested, delivered, terminal])
        if transitions != native["transitions"] or report["summary"]["recovered"] != native["recovered"]:
            raise RuntimeError("transition timing or ownership recovery differs")


def verify_failure(result, exit_code, kind):
    diagnostic = "intentional loop " + ("processor" if kind == "processor" else "Rust heap") + " failure canary detected"
    witness = result.get("workletFailure") or {}
    heap = witness.get("heap", [])
    expected_code, expected_origin = (10, 1) if kind == "processor" else (0xffff0001, 2)
    if (exit_code == 0 or result.get("passed") is not False or diagnostic not in result.get("failure", "")
            or witness.get("code") != expected_code or witness.get("processor") != expected_origin
            or len(heap) != 3 or any(type(count) is not int for count in heap)
            or (heap != [0, 0, 0] if kind == "processor" else any(count <= 0 for count in heap))):
        raise RuntimeError(f"{kind} did not fail with its specific Rust-origin witnesses")
    return {"kind": kind, "exit_code": exit_code, "diagnostic": diagnostic, "witness": witness}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--no-build", action="store_true", help="Reuse existing native/Wasm artifacts")
    parser.add_argument("--profile", choices=("debug", "release"), default="release")
    parser.add_argument("--failure-canary", choices=("processor", "rust-heap"), help="Must fail through the corresponding real worklet path")
    parser.add_argument("--verify-failures", action="store_true")
    args = parser.parse_args()
    reports = ASSETS / "reports" / args.profile
    if args.failure_canary:
        reports = reports / ("failure-" + args.failure_canary)
    reports.mkdir(parents=True, exist_ok=True)
    assets = ASSETS / args.profile
    assets.mkdir(parents=True, exist_ok=True)
    harness.ASSETS, harness.REPORTS = assets, reports
    for name in ("result.json", "browser.json", "browser-console.json"):
        (reports / name).unlink(missing_ok=True)
    manifest = str(FIXTURE / "Cargo.toml")
    profile = ["--release"] if args.profile == "release" else []
    if not args.no_build:
        harness.command(["cargo", "build", "--locked", "--manifest-path", manifest, *profile], reports / "native-build.log")
        harness.command(["wasm-pack", "build", str(FIXTURE), "--target", "web", "--release" if args.profile == "release" else "--dev",
                         "--out-dir", str(assets / "pkg"), "--out-name", "fixture", "--no-opt", "--", "--locked"], reports / "wasm-build.log")
    for source in (FIXTURE / "web").iterdir():
        if source.suffix in (".js", ".mjs"):
            shutil.copy2(source, assets / source.name)
    (assets / "package.json").write_text('{"type":"module"}\n')
    shutil.copy2(ROOT / "tests/rt_runtime_host/web/text_decoder.js", assets / "text_decoder.js")
    (assets / "index.html").write_text("<!doctype html><meta charset=utf-8><title>Independent loop host</title>\n")
    node = None
    if not args.failure_canary:
        node = json.loads(harness.command(["node", str(assets / "node_checks.mjs")], reports / "node-checks.json"))
        if not node.get("passed") or len(node.get("checks", [])) != 7:
            raise RuntimeError("incomplete Node binding checks")
    with harness.server() as url:
        with urllib.request.urlopen(url + "/index.html") as response:
            if response.headers.get("Cross-Origin-Opener-Policy") or response.headers.get("Cross-Origin-Embedder-Policy"):
                raise RuntimeError("unexpected isolation headers")
        result = harness.browser(url, args.failure_canary)
    executable = FIXTURE / "target" / args.profile / ("replay.exe" if os.name == "nt" else "replay")
    native = json.loads(harness.command([str(executable), "--json"], reports / "native.json"))
    compare(result, native)
    if result.get("bindingChecks") != {key: node[key] for key in ("passed", "checks")}:
        raise RuntimeError("Chromium binding checks differ from Node")
    result["node"] = node
    for mode in ("--direct", "--wire", "--two-thread"):
        harness.command([str(executable), mode], reports / (mode[2:] + ".log"))
    result["profile"] = args.profile
    result["wasm_sha256"] = hashlib.sha256((assets / "pkg/fixture_bg.wasm").read_bytes()).hexdigest()
    result["source_head"] = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    result["source_dirty"] = bool(subprocess.check_output(["git", "status", "--porcelain", "--", str(FIXTURE), str(Path(__file__).resolve())], cwd=ROOT, text=True).strip())
    if args.verify_failures:
        result["failure_canaries"] = []
        for kind in ("processor", "rust-heap"):
            failed = subprocess.run([sys.executable, str(Path(__file__).resolve()), "--no-build", "--profile", args.profile, "--failure-canary", kind],
                                    cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=360)
            (reports / f"{kind}-failure-gate.log").write_text(failed.stdout)
            failure_report = reports / ("failure-" + kind) / "browser.json"
            if not failure_report.exists():
                raise RuntimeError(f"{kind} produced no browser failure evidence:\n{failed.stdout[-12000:]}")
            result["failure_canaries"].append(verify_failure(json.loads(failure_report.read_text()), failed.returncode, kind))
    (reports / "result.json").write_text(json.dumps(result, indent=2))
    print(json.dumps({"passed": True, "profile": args.profile, "contexts": [r["mode"] for r in result["reports"]], "reports": str(reports)}, indent=2))


if __name__ == "__main__":
    main()
