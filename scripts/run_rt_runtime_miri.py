#!/usr/bin/env python3
"""Pinned Miri checks for runtime ownership, borrowed buffers and loop rendering."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import signal
import subprocess

ROOT = Path(__file__).resolve().parents[1]
TOOLCHAIN = "nightly-2026-05-04"
REPORTS = ROOT / "target/rt-runtime-miri"
CASES = [
    ("contracts", [], 12),
    ("planning_graph", [], 8),
    ("planner_execution", [], 9),
    ("initial_graph", [], 3),
    ("resource_render", [], 4),
    ("resource_index", [], 5),
    ("midi_state", [], 6),
    ("midi_restore_state", [], 2),
    ("midi_index", [], 8),
    ("typed_schedule", [], 7),
    ("typed_execution", [], 12),
    ("wire_properties", [], 3),
    ("resource_properties", [], 1),
    ("mixed_stress", [], 1),
    ("capture", [], 7),
    ("capture_groups", [], 5),
    ("capture_group_intervals", [], 5),
    ("capture_prepared_interval", [], 3),
    ("capture_group_planner", [], 1),
    ("capture_boundary", [], 1),
    ("capture_group_model", [], 1),
    ("capture_group_disconnect", [], 1),
    ("host_processor", [], 2),
    ("lifecycle", ["--", "--skip", "deterministic_admission_model", "--skip", "cancellation_flag_model"], 9),
    ("resource_execution", ["resource_payloads_survive_saturation_refusal_cancellation_recovery_and_disconnect"], 1),
]


def run_case(command, env):
    with subprocess.Popen(command, cwd=ROOT, env=env, stdout=subprocess.PIPE,
                          stderr=subprocess.STDOUT, text=True,
                          start_new_session=os.name == "posix") as child:
        try:
            output, _ = child.communicate(timeout=900)
        except BaseException as error:
            if os.name == "posix":
                try:
                    os.killpg(child.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            else:
                child.kill()
            output, _ = child.communicate()
            if isinstance(error, subprocess.TimeoutExpired):
                error.output = output
            raise
        return subprocess.CompletedProcess(command, child.returncode, output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package", choices=("rt_runtime", "shoop_loop_engine"), help="Run one package's selected cases into a separate report directory")
    args = parser.parse_args()
    reports = REPORTS / args.package if args.package else REPORTS
    reports.mkdir(parents=True, exist_ok=True)
    (reports / "result.json").unlink(missing_ok=True)
    env = os.environ.copy()
    for key in ("RUSTC", "RUSTDOC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "MIRIFLAGS"):
        env.pop(key, None)
    version = subprocess.check_output(["rustup", "run", TOOLCHAIN, "cargo", "miri", "--version"], cwd=ROOT, env=env, text=True).strip()
    cases = [("rt_runtime", target, arguments, minimum, 0) for target, arguments, minimum in CASES]
    cases.extend(("rt_runtime", "lifecycle", ["two_thread_stress_keeps_completion_and_state_publication_coherent"], 1, seed) for seed in (1, 2))
    cases.append(("shoop_loop_engine", "audio", [], 6, 0))
    cases.append(("shoop_loop_engine", "audio_ports", [], 2, 0))
    cases.append(("shoop_loop_engine", "lib", [], 2, 0))
    cases.append(("shoop_loop_engine", "topology_staging", [], 7, 0))
    cases.append(("shoop_loop_engine", "runtime_topology", [], 6, 0))
    cases.append(("shoop_loop_engine", "media", [], 4, 0))
    cases.append(("shoop_loop_engine", "media_decisions", [], 4, 0))
    cases.append(("shoop_loop_engine", "capture_decisions", [], 4, 0))
    cases.append(("shoop_loop_engine", "midi_restore", [], 4, 0))
    cases.append(("shoop_loop_engine", "midi", [], 6, 0))
    for name in (
        "coordinated_stereo_midi_record_play_seek_export_and_repeated_capture_match_independent_oracles",
        "scenarios::prepared_replacement_retirement_gain_capture_revisions_and_late_delivery_are_coherent",
        "scenarios::stopped_input_initial_state_and_half_open_prefix_export_survive_immediate_play",
        "scenarios::capture_preflight_and_optional_export_refusals_do_not_publish_partial_takes",
        "scenarios::running_capture_exhaustion_preserves_independently_predicted_common_prefix_and_live_audio",
        "scenarios::recovery_and_cancellation_do_not_require_normal_domain_or_runtime_result_credits",
        "scenarios::stopped_extraction_recovers_active_and_partially_staged_faulted_capture_owners",
        "scenarios::dense_one_sample_media_keeps_all_128_boundaries_with_unread_external_results",
        "scenarios::facade_role_validation_invalid_operations_and_imported_revision_exhaustion_are_checked",
        "scenarios::integrated_restart_smoothing_and_gain_follow_decisions_not_host_cuts",
    ):
        cases.append(("shoop_loop_engine", "runtime", [name, "--", "--exact"], 1, 0))
    cases.append(("shoop_loop_engine", "topology_views", [], 3, 0))
    cases.append(("shoop_loop_engine", "interloop", [], 1, 0))
    if args.package:
        cases = [case for case in cases if case[0] == args.package]
    results = []
    for index, (package, target, arguments, minimum, seed) in enumerate(cases):
        case_env = {**env, "MIRIFLAGS": f"-Zmiri-seed={seed}"}
        selection = ["--lib"] if target == "lib" else ["--test", target]
        command = ["rustup", "run", TOOLCHAIN, "cargo", "miri", "test", "--locked", "-p", package, *selection, *arguments]
        log = reports / f"{index:02d}-{target}-seed{seed}.log"
        try:
            result = run_case(command, case_env)
        except subprocess.TimeoutExpired as error:
            output = error.stdout or ""
            log.write_text(output.decode(errors="replace") if isinstance(output, bytes) else output)
            raise RuntimeError(f"Miri case timed out: {target} seed {seed}; partial output: {log}") from error
        log.write_text(result.stdout)
        matches = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed", result.stdout)
        passed = sum(int(count) for count, _ in matches)
        if result.returncode or passed < minimum or any(int(failed) for _, failed in matches):
            raise RuntimeError(f"Miri case failed or tests missing: {target} seed {seed}\n{result.stdout[-12000:]}")
        record = {"package": package, "target": target, "arguments": arguments, "seed": seed, "passed": passed, "log": log.name}
        results.append(record)
        print(json.dumps(record), flush=True)
    report = {"passed": True, "package_filter": args.package, "toolchain": TOOLCHAIN, "miri": version, "cases": results,
              "limitations": ["Not a whole-workspace or whole-suite Miri claim.",
                              "Loom's generator/FFI model runs natively, not under Miri.",
                              "Long seeded loops and two-thread stress use explicitly smaller cfg(miri) counts; full counts run natively and applicable cases in Node/Chromium.",
                              "Browser/Web Audio/JavaScript and external C++ tracing are not interpreted; real-worklet and native tracing gates cover them separately."]}
    (reports / "result.json").write_text(json.dumps(report, indent=2))
    print(f"Miri: {sum(case['passed'] for case in results)} passing executions; {reports.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
