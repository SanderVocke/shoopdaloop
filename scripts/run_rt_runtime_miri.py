#!/usr/bin/env python3
"""Pinned Miri checks for runtime ownership, borrowed buffers and loop rendering."""
from __future__ import annotations

import argparse
import hashlib
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
    ("midi_state", [], 7),
    ("midi", [], 6),
    ("midi_restore_state", [], 2),
    ("midi_index", [], 8),
    ("typed_schedule", [], 7),
    ("typed_execution", [], 13),
    ("planner_admission", [], 3),
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
    ("lib", ["buffer::tests::bulk_preparation_and_window_merges_match_scalar_packets_through_refusal_reset_and_reuse", "--", "--exact"], 1),
]


STAGE8_MODELS = (
    ("rt_runtime", "adversarial_delivery", "seeded_typed_payloads_receipts_and_recycled_tokens_match_a_full_length_independent_model"),
    ("rt_runtime", "adversarial_capture", "seeded_capture_indexes_keep_immutable_prefixes_across_partitions_reuse_and_exhausted_maintenance"),
    ("shoop_loops", "adversarial_timeline", "seeded_programs_preserve_scalar_timing_through_full_receipts_journal_loss_and_rejected_owners"),
    ("shoop_loops", "adversarial_timeline", "seeded_sync_components_deliver_each_occurrence_once_with_saturated_results_and_optional_journals"),
)


# New engine backend: the protocol codec and carriers, the control endpoint's
# materialization of runtime commands and the engine's pool handover.
ENGINE_CASES = (
    ("shoop_rt_protocol", "protocol", [], 5),
    ("shoop_rt_engine", "lib", [], 2),
    ("shoop_rt_engine", "workflow", [], 1),
    ("shoop_rt_engine", "control", [], 5),
    ("shoop_rt_engine", "structure", ["reused_slots_start_fresh_and_clear_empties_a_loop", "--", "--exact"], 1),
    ("shoop_rt_engine", "structure", ["edits_keep_other_paths_continuous_and_ramp_removed_paths", "--", "--exact"], 1),
)


HOST_PACKAGE = "loops_host_fixture"
HOST_MANIFEST = "tests/loops_host/Cargo.toml"
HOST_CASES = (
    "bridge_tests::callback_fault_is_permanent_and_pending_wire_ownership_is_reclaimed_off_render",
    "bridge_tests::client_rejects_duplicate_foreign_and_reordered_receipts_without_releasing_credit",
    "bridge_tests::malformed_foreign_duplicate_requests_and_disconnect_keep_owners_bounded",
    "bridge_tests::repeated_slot_reuse_rejects_old_cancellation_and_receipts",
    "bridge_tests::response_codec_preserves_error_origin_and_rejects_noncanonical_records",
    "bridge_tests::stalled_normal_returns_preserve_music_and_independent_recovery",
    "bridge_tests::transport_cancellation_stale_targets_domain_refusal_and_lateness_are_distinct",
    "bridge_tests::wire_layout_high_bits_and_canonical_rejections_are_heap_free",
    "bridge_tests::wire_path_matches_complete_native_media_across_partitions",
    "browser::native_dense::tests::acknowledged_restart_keeps_media_and_owning_results_while_transport_is_unpolled",
    "browser::native_dense::tests::dense_wire_and_capacity_one_actors_preserve_music_with_all_results_stalled",
    "browser::native_export::tests::cancelled_reads_eof_stalls_and_128_export_reuses_return_real_owners",
    "browser::native_export::tests::immutable_exports_hold_both_revisions_across_native_direct_and_bounded_threads",
    "browser::native_graph::tests::acknowledged_graph_worklet_script_has_complete_partition_invariant_native_reference",
    "browser::native_graph::tests::bounded_two_thread_graph_transport_matches_complete_native_reference",
    "browser::native_media::tests::replacement_countdown_prefix_exports_and_bounded_actors_match_complete_oracles",
    "browser::native_staging::tests::cancelled_owning_chunks_and_128_program_reuses_preserve_real_buffers_and_generations",
    "browser::native_staging::tests::prepared_resource_uploads_immutable_reads_and_nested_programs_match_bounded_actors",
    "browser::tests::actual_processor_and_heap_failures_have_distinct_witnesses",
    "browser::tests::diagnostic_journal_exhaustion_does_not_change_music",
    "browser::tests::graph_binding_buffers_cover_route_cut_and_reliable_owning_release",
    "browser::tests::graph_binding_fault_disposal_abandons_credits_without_fabricating_returns",
    "browser::tests::prepared_binding_buffers_match_native_media_and_recover_all_owners",
    "browser::tests::same_position_seek_is_observed_without_inventing_a_transition_or_pulse",
    "client::tests::last_sequence_is_usable_once_without_wrapping",
    "export_wire::tests::client_identity_credit_and_exhaustion_are_transactional_and_never_wrap",
    "export_wire::tests::full_width_export_records_reject_truncation_padding_and_invalid_projection",
    "graph_bridge::tests::local_transport_refusal_returns_the_candidate_and_next_bank_generation_without_rendering",
    "graph_bridge::tests::unexpected_owning_completion_is_returned_intact_without_recycling_or_fabricating_a_reply",
    "graph_transfer::tests::busy_stale_foreign_and_malformed_records_do_not_advance_or_destroy_owners",
    "graph_transfer::tests::cancelled_stale_and_invalid_runtime_candidates_return_the_exact_prepared_owner",
    "graph_transfer::tests::canonical_records_preserve_full_width_fields_and_reject_all_padding_and_truncations",
    "graph_transfer::tests::generation_and_sequence_exhaustion_never_wrap_and_recycled_capacities_are_truthful",
    "graph_transfer::tests::ingress_refusal_stalled_returns_and_disconnect_keep_all_graph_owners_explicit",
    "graph_transfer::tests::receiver_descriptors_stage_activate_and_recycle_real_facade_graph_owners",
    "graph_transfer::tests::receiving_is_transactional_ordered_and_independent_of_other_banks",
    "graph_wire::tests::actual_bridge_graph_staging_activation_and_retirement_match_all_media_and_receipts",
    "graph_wire::tests::graph_candidate_returns_and_domain_failure_origins_survive_128_wire_cycles",
    "graph_wire::tests::graph_client_credits_validate_identity_phase_and_last_sequence_without_wrapping",
    "graph_wire::tests::graph_command_codec_preserves_full_width_fields_and_canonical_padding",
    "graph_wire::tests::malformed_graph_requests_preserve_receiver_state_and_queued_ready_owners_drop_off_render",
    "graph_wire::tests::stalled_graph_credits_preserve_music_and_independent_primitive_recovery",
    "journal::tests::all_event_variants_preserve_full_width_fields_and_canonical_padding",
    "journal::tests::decision_projection_preserves_large_values_and_absent_optional_fields",
    "journal::tests::loss_is_sticky_saturating_and_empty_callback_cuts_do_not_consume_records",
    "native::tests::bounded_two_thread_and_wire_drivers_match_the_independent_native_oracle",
    "planner_canary::tests::planner_errors_and_heap_faults_have_exact_lifecycle_origins_and_are_not_successful_cleanup",
    "planner_canary::tests::resolver_planner_control_is_partition_invariant_with_complete_boundary_tables",
    "scenario::dense::tests::dense_capture_cascades_and_clocked_composites_keep_music_with_unread_results",
    "scenario::media_case::tests::replacement_countdown_and_capture_exhaustion_keep_exact_prefix_and_live_audio",
    "scenario::tests::allocator_instrumentation_detects_all_heap_operations",
    "scenario::tests::guard_rejects_allocation",
    "scenario::tests::guard_rejects_deallocation",
    "scenario::tests::guard_rejects_reallocation",
    "scenario::tests::native_partitions_match_full_media_and_owning_recovery",
    "scenario::tests::semantic_storage_and_resolver_journal_loss_preserve_media_receipts_and_recovery",
    "staging_wire::tests::canonical_staging_records_preserve_full_width_and_reject_all_truncations_and_padding",
    "staging_wire::tests::countdown_results_keep_delivery_distinct_from_terminal_frame_and_preserve_credit_on_corruption",
    "staging_wire::tests::staging_client_rejections_preserve_credit_and_nonwrapping_identity_watermarks",
    "graph_transfer::model::seeded_descriptor_bank_model_preserves_complete_tables_and_exact_owners_after_every_action",
)
HOST_CONCURRENCY = (
    "native::tests::bounded_two_thread_and_wire_drivers_match_the_independent_native_oracle",
    "browser::native_graph::tests::bounded_two_thread_graph_transport_matches_complete_native_reference",
)


def manifest_paths(cases):
    return {package: HOST_MANIFEST if package == HOST_PACKAGE else "Cargo.toml"
            for package, *_ in cases}


def shard_spec(value):
    try:
        index, total = map(int, value.split("/"))
        if not 1 <= index <= total:
            raise ValueError
    except ValueError as error:
        raise argparse.ArgumentTypeError("shard must be INDEX/TOTAL with 1 <= INDEX <= TOTAL") from error
    return index, total


def sharded_cases(cases, shard):
    if shard is None:
        return cases
    index, total = shard
    if total > len(cases):
        raise ValueError("shard count would create empty batches")
    return [case for offset, case in enumerate(cases) if offset % total == index - 1]


def inventory(package=None, shard=None, measureme=False):
    cases = selected_cases(package)
    full = {"toolchain": TOOLCHAIN, "cases": cases, "manifests": manifest_paths(cases)}
    fingerprint = hashlib.sha256(json.dumps(full, sort_keys=True).encode()).hexdigest()
    selected = sharded_cases(cases, shard)
    if measureme and len(selected) != 1:
        raise ValueError("measureme requires exactly one selected command for justified failure diagnosis")
    return {"toolchain": TOOLCHAIN, "cases": selected, "manifests": manifest_paths(selected),
            "full_inventory_sha256": fingerprint, "measureme": measureme,
            "shard": {"index": shard[0], "total": shard[1]} if shard else None}


def source_commit():
    recorded = os.environ.get("MIRI_SOURCE_COMMIT") or os.environ.get("GITHUB_SHA")
    return recorded or subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()


def verify_batches(root):
    full = inventory()
    source = source_commit()
    reports = sorted(root.rglob("result.json"))
    if not reports:
        raise RuntimeError("Missing Miri batch reports")
    found = {}
    total = None
    version = None
    artifacts = {}
    for path in reports:
        report = json.loads(path.read_text())
        shard = report.get("shard")
        if (not isinstance(shard, dict) or type(shard.get("index")) is not int
                or type(shard.get("total")) is not int):
            raise RuntimeError("Missing or invalid Miri batch identity")
        index, count = shard["index"], shard["total"]
        if not 1 <= index <= count <= len(full["cases"]) or index in found:
            raise RuntimeError("Duplicate or invalid Miri batch")
        if total is not None and total != count:
            raise RuntimeError("Inconsistent Miri batch count")
        total = count
        selected = inventory(shard=(index, count))
        if (report.get("passed") is not True or report.get("toolchain") != TOOLCHAIN
                or report.get("package_filter") is not None or report.get("source_commit") != source
                or report.get("full_inventory_sha256") != full["full_inventory_sha256"]
                or report.get("manifests") != selected["manifests"]
                or report.get("measureme") is not False):
            raise RuntimeError("Miri batch source, inventory or manifest scope mismatch")
        miri = report.get("miri")
        if not isinstance(miri, str) or not miri or (version is not None and version != miri):
            raise RuntimeError("Missing or inconsistent Miri version")
        version = miri
        cases = report.get("cases", [])
        if len(cases) != len(selected["cases"]):
            raise RuntimeError("Incomplete Miri batch inventory")
        seen = set()
        artifacts[str(path.relative_to(root))] = hashlib.sha256(path.read_bytes()).hexdigest()
        for case, expected in zip(cases, selected["cases"]):
            package, target, arguments, minimum, seed = expected
            if (type(case.get("passed")) is not int or type(case.get("seed")) is not int
                    or (case.get("package"), case.get("target"), case.get("arguments"),
                        case.get("passed"), case.get("seed")) != (package, target, arguments, minimum, seed)
                    or case.get("miri_flags") != case_flags(seed, path.parent)):
                raise RuntimeError("Miri batch case/filter/count/seed mismatch")
            name = case.get("log")
            if not isinstance(name, str) or Path(name).name != name or name in seen:
                raise RuntimeError("Invalid or duplicate Miri batch log")
            seen.add(name)
            log = path.parent / name
            summaries = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed", log.read_text())
            if (not summaries or sum(int(n) for n, _ in summaries) != minimum
                    or any(int(n) for _, n in summaries)):
                raise RuntimeError("Missing or mismatched Miri batch log summaries")
            artifacts[str(log.relative_to(root))] = hashlib.sha256(log.read_bytes()).hexdigest()
        found[index] = {"report": str(path.relative_to(root)), "run_attempt": report.get("run_attempt"),
                        "commands": len(cases), "executions": sum(case["passed"] for case in cases)}
    if set(found) != set(range(1, total + 1)):
        raise RuntimeError("Incomplete Miri batch set")
    return {"passed": True, "source_commit": source, "toolchain": TOOLCHAIN, "miri": version,
            "full_inventory_sha256": full["full_inventory_sha256"], "batches": found,
            "commands": sum(batch["commands"] for batch in found.values()),
            "executions": sum(batch["executions"] for batch in found.values()),
            "artifact_sha256": artifacts,
            "limitations": ["Separate CI batches, not one uninterrupted interpreter invocation.",
                            "Proves this recorded source and selected inventory, not whole-workspace coverage."]}


def case_flags(seed, reports, measureme=False):
    flags = f"-Zmiri-seed={seed}"
    if measureme:
        path = reports.resolve() / "measureme"
        if any(character.isspace() for character in str(path)):
            raise ValueError("measureme report path must not contain whitespace")
        flags += f" -Zmiri-measureme={path}"
    return flags


def case_command(package, target, arguments):
    selection = ["--lib"] if target == "lib" else ["--test", target]
    manifest = ["--manifest-path", HOST_MANIFEST] if package == HOST_PACKAGE else []
    return ["rustup", "run", TOOLCHAIN, "cargo", "miri", "test", "--locked", *manifest,
            "-p", package, *selection, *arguments]


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
    cases.append(("shoop_loops", "audio", [], 6, 0))
    cases.append(("shoop_loops", "audio_ports", [], 2, 0))
    cases.append(("shoop_loops", "lib", [], 4, 0))
    cases.append(("shoop_loops", "program_boundaries", [], 3, 0))
    cases.append(("shoop_loops", "program_changes", [], 10, 0))
    cases.append(("shoop_loops", "topology_staging", [], 7, 0))
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
        "graph_pair_preflight_rejects_node_pair_scope_epoch_and_cancelled_owners_without_partial_publication",
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
        "stale_scope_generation_node_pair_and_malformed_candidates_preserve_the_working_timeline",
        "stopped_extraction_recovers_receiving_validating_ready_failed_and_retired_banks",
        "timing_activation_during_capture_keeps_the_take_prefix_and_recorded_fx_media_coherent",
        "topology_revalidation_completes_countdown_receipts_and_cancelled_delivery_keeps_the_candidate",
    ):
        cases.append(("shoop_loops", "runtime_topology", [name, "--", "--exact"], 1, 0))
    cases.append(("shoop_loops", "media", [], 7, 0))
    cases.append(("shoop_loops", "media_decisions", [], 4, 0))
    cases.append(("shoop_loops", "capture_decisions", [], 4, 0))
    cases.append(("shoop_loops", "midi_restore", [], 4, 0))
    cases.append(("shoop_loops", "midi", [], 6, 0))
    for name in (
        "coordinated_stereo_midi_record_play_seek_export_and_repeated_capture_match_independent_oracles",
        "scenarios::prepared_replacement_retirement_gain_capture_revisions_and_late_delivery_are_coherent",
        "scenarios::stopped_input_initial_state_and_half_open_prefix_export_survive_immediate_play",
        "scenarios::capture_preflight_and_optional_export_refusals_do_not_publish_partial_takes",
        "scenarios::running_capture_exhaustion_preserves_independently_predicted_common_prefix_and_live_audio",
        "scenarios::recovery_and_cancellation_do_not_require_normal_domain_or_runtime_result_credits",
        "scenarios::stopped_extraction_recovers_active_and_partially_staged_faulted_capture_owners",
        "scenarios::dense_one_sample_media_keeps_all_128_boundaries_with_unread_external_results",
        "scenarios::facade_node_pair_validation_invalid_operations_and_imported_revision_exhaustion_are_checked",
        "scenarios::integrated_restart_smoothing_and_gain_follow_decisions_not_host_cuts",
    ):
        cases.append(("shoop_loops", "runtime", [name, "--", "--exact"], 1, 0))
    cases.append(("shoop_loops", "topology_views", [], 3, 0))
    for name in (
        "cascades_nested_clocked_program_and_capture_outcomes_match_independent_media_and_journals",
        "nested_lifecycle::nested_seek_stop_restart_and_direct_priority_preserve_media_cuts_and_pending_outcomes",
    ):
        cases.append(("shoop_loops", "interloop", [name, "--", "--exact"], 1, 0))
    host = [(HOST_PACKAGE, "lib", [name, "--", "--exact"], 1, 0) for name in HOST_CASES]
    host.extend((HOST_PACKAGE, "lib", [name, "--", "--exact"], 1, seed)
                for name in HOST_CONCURRENCY for seed in (1, 2))
    cases = host + cases
    cases.extend((owner, target, arguments, minimum, 0)
                 for owner, target, arguments, minimum in ENGINE_CASES)
    cases.extend((owner, target, [name, "--", "--exact"], 1, 0)
                 for owner, target, name in STAGE8_MODELS)
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
    parser.add_argument("--package", choices=("rt_runtime", "shoop_loops", "shoop_rt_protocol", "shoop_rt_engine", HOST_PACKAGE), help="Run one package's selected cases")
    parser.add_argument("--report-dir", type=Path, help="Isolated output directory; overrides the default package directory")
    parser.add_argument("--list", action="store_true", help="Print the exact selected inventory without running Miri")
    parser.add_argument("--shard", type=shard_spec, help="Disjoint CI batch INDEX/TOTAL; keeps whole commands and workloads")
    parser.add_argument("--measureme", action="store_true", help="Profile one justified failure command; unchanged workload/seed/900s watchdog")
    parser.add_argument("--verify-batches", type=Path, help="Verify a complete downloaded CI batch set without interpreting tests")
    args = parser.parse_args()
    if args.verify_batches is not None:
        if args.package or args.shard or args.list or args.report_dir or args.measureme:
            parser.error("--verify-batches cannot be combined with execution/selection options")
        result = verify_batches(args.verify_batches)
        atomic_json(args.verify_batches / "verified.json", result)
        print(json.dumps(result, indent=2))
        return
    try:
        selected = inventory(args.package, args.shard, args.measureme)
    except ValueError as error:
        parser.error(str(error))
    cases = selected["cases"]
    if args.list:
        print(json.dumps(selected))
        return
    if os.name == "posix":
        signal.signal(signal.SIGTERM, interrupted)
    reports = args.report_dir or (REPORTS / args.package if args.package else REPORTS)
    reports.mkdir(parents=True, exist_ok=True)
    if args.measureme:
        try:
            case_flags(0, reports, True)
        except ValueError as error:
            parser.error(str(error))
        (reports / "measureme").mkdir(exist_ok=True)
    (reports / "result.json").unlink(missing_ok=True)
    progress = {"status": "running", "inventory": cases, "completed": [], "current": None}
    atomic_json(reports / "progress.json", progress)
    env = os.environ.copy()
    for key in ("RUSTC", "RUSTDOC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "MIRIFLAGS"):
        env.pop(key, None)
    version = subprocess.check_output(["rustup", "run", TOOLCHAIN, "cargo", "miri", "--version"], cwd=ROOT, env=env, text=True).strip()
    results = []
    for index, (package, target, arguments, minimum, seed) in enumerate(cases):
        case_env = {**env, "MIRIFLAGS": case_flags(seed, reports, args.measureme)}
        command = case_command(package, target, arguments)
        log = reports / f"{index:02d}-{target}-seed{seed}.log"
        progress["current"] = {"command": command, "log": log.name, "miri_flags": case_env["MIRIFLAGS"]}
        atomic_json(reports / "progress.json", progress)
        case_started = time.monotonic()
        try:
            result = run_case(command, case_env, log)
        except subprocess.TimeoutExpired as error:
            output = error.stdout or ""
            log.write_text(output.decode(errors="replace") if isinstance(output, bytes) else output)
            progress["status"] = "failed"
            atomic_json(reports / "progress.json", progress)
            raise RuntimeError(f"Miri case timed out: {target} seed {seed}; partial output: {log}") from error
        matches = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed", result.stdout)
        passed = sum(int(count) for count, _ in matches)
        if result.returncode or passed < minimum or any(int(failed) for _, failed in matches):
            progress["status"] = "failed"
            atomic_json(reports / "progress.json", progress)
            raise RuntimeError(f"Miri case failed or tests missing: {target} seed {seed}\n{result.stdout[-12000:]}")
        record = {"package": package, "target": target, "arguments": arguments, "seed": seed, "passed": passed, "log": log.name, "seconds": time.monotonic() - case_started, "miri_flags": case_env["MIRIFLAGS"]}
        results.append(record)
        progress["completed"] = results
        progress["current"] = None
        atomic_json(reports / "progress.json", progress)
        print(json.dumps(record), flush=True)
    report = {"passed": True, "package_filter": args.package, "toolchain": TOOLCHAIN, "miri": version, "cases": results,
              "manifests": selected["manifests"], "shard": selected["shard"], "measureme": args.measureme,
              "full_inventory_sha256": selected["full_inventory_sha256"],
              "source_commit": source_commit(), "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT"),
              "limitations": ["Not a whole-workspace or whole-suite Miri claim.",
                              "Loom's generator/FFI model runs natively, not under Miri.",
                              "Long seeded loops and two-thread stress use explicitly smaller cfg(miri) counts; full counts run natively and applicable cases in Node/Chromium.",
                              "Standalone loop-host cases retain their full native workloads and allocator/destructor guards.",
                              "Browser/Web Audio/JavaScript and external C++ tracing are not interpreted; real-worklet and native tracing gates cover them separately."]}
    atomic_json(reports / "result.json", report)
    progress["status"] = "passed"
    atomic_json(reports / "progress.json", progress)
    print(f"Miri: {sum(case['passed'] for case in results)} passing executions; {reports}")


if __name__ == "__main__":
    main()
