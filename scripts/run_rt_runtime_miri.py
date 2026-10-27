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
import time

ROOT = Path(__file__).resolve().parents[1]
TOOLCHAIN = "nightly-2026-05-04"
REPORTS = ROOT / "target/rt-runtime-miri"
CASES = [
    ("contracts", [], 12),
    ("planning_graph", [], 8),
    ("planner_execution", [], 9),
    ("initial_graph", [], 3),
    ("graph_activation", [], 7),
    ("lib", ["graph::activation::tests::stage_scope_generation_and_epoch_exhaustion_preserve_unpublished_owners", "--", "--exact"], 1),
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
    ("lifecycle", ["--", "--skip", "deterministic_admission_model", "--skip", "cancellation_flag_model"], 10),
    ("resource_execution", ["resource_payloads_survive_saturation_refusal_cancellation_recovery_and_disconnect"], 1),
]


def run_case(command, env, log=None):
    if log is not None:
        with log.open("w") as output:
            return _run_case(command, env, output, log)
    return _run_case(command, env, subprocess.PIPE, None)


def _run_case(command, env, stdout, log):
    with subprocess.Popen(command, cwd=ROOT, env=env, stdout=stdout,
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
                error.output = log.read_text() if log is not None else output
            raise
        return subprocess.CompletedProcess(command, child.returncode,
                                           log.read_text() if log is not None else output)


def selected_cases(package=None):
    cases = [("rt_runtime", target, arguments, minimum, 0) for target, arguments, minimum in CASES]
    cases.extend(("rt_runtime", "lifecycle", ["two_thread_stress_keeps_completion_and_state_publication_coherent"], 1, seed) for seed in (1, 2))
    cases.append(("shoop_loop_engine", "audio", [], 6, 0))
    cases.append(("shoop_loop_engine", "audio_ports", [], 2, 0))
    cases.append(("shoop_loop_engine", "lib", [], 4, 0))
    cases.append(("shoop_loop_engine", "program_boundaries", [], 3, 0))
    cases.append(("shoop_loop_engine", "program_changes", [], 10, 0))
    cases.append(("shoop_loop_engine", "topology_staging", [], 7, 0))
    for name in (
        "configuration_scenarios::generation_graph_media_capture_and_old_exports_share_one_cut_and_independent_reference",
        "configuration_scenarios::inactive_prepared_consumers_support_kind_change_removal_and_readd_without_retargeting_old_requests",
        "configuration_scenarios::input::input_projection_is_explicitly_preserved_or_reset_once_before_the_first_new_capture",
        "configuration_scenarios::rejections::batch_capacity_is_preflighted_without_reusing_retirement_credits_or_publishing_a_prefix",
        "configuration_scenarios::rejections::configuration_refusals_and_final_resolver_failure_leave_all_three_owners_ready",
        "configuration_scenarios::sources::source_validation_shares_callback_work_and_cancel_recovers_owned_inputs_under_saturation",
        "configuration_scenarios::teardown::disconnect_and_fault_cleanup_recover_source_receivers_partial_batches_and_published_capture_claims",
        "full_domain_slots_unread_graph_results_and_retirement_do_not_gate_clock_or_recovery",
        "full_normal_domain_slots_and_unread_program_results_allow_clock_progress_and_recovery",
        "graph_and_stopped_topology_publish_together_with_independent_media_and_journals",
        "graph_pair_preflight_rejects_role_scope_epoch_and_cancelled_owners_without_partial_publication",
        "intervening_live_graph_change_refuses_pending_program_without_publishing_its_zero_actions",
        "live_route_cut_keeps_recording_prefix_and_midi_continuity_without_restarting_the_take",
        "paired_graph_topology_owners_survive_disconnect_and_post_publication_render_fault",
        "pending_program_cancellation_disconnect_and_fault_extraction_return_every_bank_owner",
        "program_capture_preflight_uses_real_banks_and_preserves_old_media_on_family_refusal",
        "program_delivery_validation_and_queued_cancellation_preserve_ready_bank_generations",
        "program_graph_scenarios::capture::paired_program_capture_preflight_publishes_both_or_neither_and_records_real_fx_media",
        "program_graph_scenarios::lifecycle::paired_cancellation_released_generations_and_intervening_graphs_never_publish_the_program",
        "program_graph_scenarios::lifecycle::paired_retirement_unread_results_recovery_disconnect_and_render_fault_preserve_ownership",
        "program_graph_scenarios::running_program_and_routes_publish_at_one_zero_cut_with_full_independent_reference",
        "running_program_bank_activation_matches_independent_media_states_and_ordered_journals",
        "staged_fixed_graph_activation_preserves_media_and_matches_independent_timeline",
        "staging_work_shares_the_callback_budget_and_recovery_ignores_normal_receipt_stalls",
        "stale_scope_generation_role_and_malformed_candidates_preserve_the_working_timeline",
        "stopped_extraction_recovers_receiving_validating_ready_failed_and_retired_banks",
        "timing_activation_during_capture_keeps_the_take_prefix_and_recorded_fx_media_coherent",
        "topology_revalidation_completes_countdown_receipts_and_cancelled_delivery_keeps_the_candidate",
    ):
        cases.append(("shoop_loop_engine", "runtime_topology", [name, "--", "--exact"], 1, 0))
    cases.append(("shoop_loop_engine", "media", [], 7, 0))
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
    for name in (
        "cascades_nested_clocked_program_and_capture_outcomes_match_independent_media_and_journals",
        "nested_lifecycle::nested_seek_stop_restart_and_direct_priority_preserve_media_cuts_and_pending_outcomes",
    ):
        cases.append(("shoop_loop_engine", "interloop", [name, "--", "--exact"], 1, 0))
    if package:
        cases = [case for case in cases if case[0] == package]
    return cases


def atomic_json(path, value):
    temporary = path.with_suffix(".tmp")
    temporary.write_text(json.dumps(value, indent=2) + "\n")
    temporary.replace(path)


def interrupted(signum, _frame):
    raise InterruptedError(f"Miri supervisor signal {signum}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package", choices=("rt_runtime", "shoop_loop_engine"), help="Run one package's selected cases")
    parser.add_argument("--report-dir", type=Path, help="Isolated output directory; overrides the default package directory")
    parser.add_argument("--list", action="store_true", help="Print the exact selected inventory without running Miri")
    args = parser.parse_args()
    cases = selected_cases(args.package)
    if args.list:
        print(json.dumps({"toolchain": TOOLCHAIN, "cases": cases}))
        return
    if os.name == "posix":
        signal.signal(signal.SIGTERM, interrupted)
    reports = args.report_dir or (REPORTS / args.package if args.package else REPORTS)
    reports.mkdir(parents=True, exist_ok=True)
    (reports / "result.json").unlink(missing_ok=True)
    progress = {"status": "running", "inventory": cases, "completed": [], "current": None}
    atomic_json(reports / "progress.json", progress)
    env = os.environ.copy()
    for key in ("RUSTC", "RUSTDOC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "MIRIFLAGS"):
        env.pop(key, None)
    version = subprocess.check_output(["rustup", "run", TOOLCHAIN, "cargo", "miri", "--version"], cwd=ROOT, env=env, text=True).strip()
    results = []
    for index, (package, target, arguments, minimum, seed) in enumerate(cases):
        case_env = {**env, "MIRIFLAGS": f"-Zmiri-seed={seed}"}
        selection = ["--lib"] if target == "lib" else ["--test", target]
        command = ["rustup", "run", TOOLCHAIN, "cargo", "miri", "test", "--locked", "-p", package, *selection, *arguments]
        log = reports / f"{index:02d}-{target}-seed{seed}.log"
        progress["current"] = {"command": command, "log": log.name}
        atomic_json(reports / "progress.json", progress)
        case_started = time.monotonic()
        try:
            result = run_case(command, case_env, log)
        except subprocess.TimeoutExpired as error:
            output = error.stdout or ""
            log.write_text(output.decode(errors="replace") if isinstance(output, bytes) else output)
            raise RuntimeError(f"Miri case timed out: {target} seed {seed}; partial output: {log}") from error
        matches = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed", result.stdout)
        passed = sum(int(count) for count, _ in matches)
        if result.returncode or passed < minimum or any(int(failed) for _, failed in matches):
            raise RuntimeError(f"Miri case failed or tests missing: {target} seed {seed}\n{result.stdout[-12000:]}")
        record = {"package": package, "target": target, "arguments": arguments, "seed": seed, "passed": passed, "log": log.name, "seconds": time.monotonic() - case_started}
        results.append(record)
        progress["completed"] = results
        progress["current"] = None
        atomic_json(reports / "progress.json", progress)
        print(json.dumps(record), flush=True)
    report = {"passed": True, "package_filter": args.package, "toolchain": TOOLCHAIN, "miri": version, "cases": results,
              "limitations": ["Not a whole-workspace or whole-suite Miri claim.",
                              "Loom's generator/FFI model runs natively, not under Miri.",
                              "Long seeded loops and two-thread stress use explicitly smaller cfg(miri) counts; full counts run natively and applicable cases in Node/Chromium.",
                              "Browser/Web Audio/JavaScript and external C++ tracing are not interpreted; real-worklet and native tracing gates cover them separately."]}
    atomic_json(reports / "result.json", report)
    progress["status"] = "passed"
    atomic_json(reports / "progress.json", progress)
    print(f"Miri: {sum(case['passed'] for case in results)} passing executions; {reports}")


if __name__ == "__main__":
    main()
