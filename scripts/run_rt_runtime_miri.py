#!/usr/bin/env python3
"""Pinned, isolated Miri checks for rt_runtime ownership and borrowed buffers."""
from __future__ import annotations

import json
import os
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]
TOOLCHAIN = "nightly-2026-05-04"
REPORTS = ROOT / "target/rt-runtime-miri"
CASES = [
    ("contracts", [], 12),
    ("planning_graph", [], 8),
    ("planner_execution", [], 8),
    ("resource_render", [], 4),
    ("resource_index", [], 5),
    ("typed_schedule", [], 7),
    ("typed_execution", [], 11),
    ("wire_properties", [], 3),
    ("resource_properties", [], 1),
    ("mixed_stress", [], 1),
    ("capture", [], 7),
    ("host_processor", [], 2),
    ("lifecycle", ["--", "--skip", "deterministic_admission_model", "--skip", "cancellation_flag_model"], 9),
    ("resource_execution", ["resource_payloads_survive_saturation_refusal_cancellation_recovery_and_disconnect"], 1),
]


def main():
    REPORTS.mkdir(parents=True, exist_ok=True)
    (REPORTS / "result.json").unlink(missing_ok=True)
    env = os.environ.copy()
    for key in ("RUSTC", "RUSTDOC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "MIRIFLAGS"):
        env.pop(key, None)
    version = subprocess.check_output(["rustup", "run", TOOLCHAIN, "cargo", "miri", "--version"], cwd=ROOT, env=env, text=True).strip()
    cases = [(target, arguments, minimum, 0) for target, arguments, minimum in CASES]
    cases.extend(("lifecycle", ["two_thread_stress_keeps_completion_and_state_publication_coherent"], 1, seed) for seed in (1, 2))
    results = []
    for index, (target, arguments, minimum, seed) in enumerate(cases):
        case_env = {**env, "MIRIFLAGS": f"-Zmiri-seed={seed}"}
        command = ["rustup", "run", TOOLCHAIN, "cargo", "miri", "test", "--locked", "-p", "rt_runtime", "--test", target, *arguments]
        log = REPORTS / f"{index:02d}-{target}-seed{seed}.log"
        try:
            result = subprocess.run(command, cwd=ROOT, env=case_env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, timeout=900)
        except subprocess.TimeoutExpired as error:
            output = error.stdout or ""
            log.write_text(output.decode(errors="replace") if isinstance(output, bytes) else output)
            raise RuntimeError(f"Miri case timed out: {target} seed {seed}; partial output: {log}") from error
        log.write_text(result.stdout)
        matches = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed", result.stdout)
        passed = sum(int(count) for count, _ in matches)
        if result.returncode or passed < minimum or any(int(failed) for _, failed in matches):
            raise RuntimeError(f"Miri case failed or tests missing: {target} seed {seed}\n{result.stdout[-12000:]}")
        record = {"target": target, "arguments": arguments, "seed": seed, "passed": passed, "log": log.name}
        results.append(record)
        print(json.dumps(record), flush=True)
    report = {"passed": True, "toolchain": TOOLCHAIN, "miri": version, "cases": results,
              "limitations": ["Not a whole-workspace or whole-suite Miri claim.",
                              "Loom's generator/FFI model runs natively, not under Miri.",
                              "Long seeded loops and two-thread stress use explicitly smaller cfg(miri) counts; full counts run natively and applicable cases in Node/Chromium.",
                              "Browser/Web Audio/JavaScript and external C++ tracing are not interpreted; real-worklet and native tracing gates cover them separately."]}
    (REPORTS / "result.json").write_text(json.dumps(report, indent=2))
    print(f"Miri: {sum(case['passed'] for case in results)} passing executions; {REPORTS.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
