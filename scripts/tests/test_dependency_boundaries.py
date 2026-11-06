import subprocess
import unittest
from unittest.mock import patch
import contextlib
import io

from scripts import check_dependency_boundaries as boundary


class RuntimeBoundaryTests(unittest.TestCase):
    def test_accepts_generic_dependencies(self):
        self.assertEqual(boundary.violations("rt_runtime v0.3.0\nrtrb v0.4.0\nthiserror v2.0.0\n", 'rt_runtime'), [])

    def test_rejects_domain_host_ui_and_browser_dependencies(self):
        tree = "\n".join(f"{name} v1.0.0 (*)" for name in (
            "shoop_engine", "shoop_tracing", "cpal", "eframe", "web-sys", "wasm-bindgen", "js-sys",
        ))
        self.assertEqual(boundary.violations(tree, 'rt_runtime'), [
            "cpal", "eframe", "js-sys", "shoop_engine", "shoop_tracing", "wasm-bindgen", "web-sys",
        ])


class LoopBoundaryTests(unittest.TestCase):
    def test_allows_only_the_loop_root_and_generic_dependencies(self):
        self.assertEqual(boundary.violations("shoop_loops v0.3.0\nshoop_loop_plan v0.3.0\nrt_runtime v0.3.0\nrtrb v0.4.0\n", 'shoop_loops'), [])

    def test_rejects_other_domain_and_host_dependencies(self):
        names = ["shoop_engine", "shoop_tracing", "shoop_backend", "shoop_app",
                 "shoop_wasm_test_support", "web-sys", "wasm-bindgen", "cpal"]
        self.assertEqual(boundary.violations("\n".join(f"{name} v1.0.0" for name in names), 'shoop_loops'), sorted(names))

    def test_runtime_policy_still_rejects_the_loop_crate(self):
        self.assertEqual(boundary.violations("shoop_loops v0.3.0\n", "rt_runtime"), ["shoop_loops"])

    def test_the_plan_compiler_cannot_pull_runtime_or_domain_owners_into_adapters(self):
        self.assertEqual(boundary.violations("shoop_loop_plan v0.3.0\n", "shoop_loop_plan"), [])
        names = ["rt_runtime", "shoop_loops", "shoop_backend", "shoop_app", "cpal"]
        self.assertEqual(boundary.violations("\n".join(f"{n} v1.0.0" for n in names), "shoop_loop_plan"), sorted(names))


class EngineBoundaryTests(unittest.TestCase):
    def test_allows_the_engine_stack(self):
        self.assertEqual(boundary.violations("shoop_rt_engine v0.3.0\nshoop_rt_protocol v0.3.0\nshoop_loops v0.3.0\n"
            "rt_runtime v0.3.0\nrtrb v0.4.0\n", 'shoop_rt_engine'), [])

    def test_covers_the_native_host_and_allows_its_device_libraries(self):
        self.assertIn("shoop_rt_host_native", boundary.SCOPES["engine"])
        self.assertEqual(boundary.violations("shoop_rt_host_native v0.3.0\nshoop_rt_engine v0.3.0\ncpal v0.17.1\n"
            "midir v0.11.0\nrtrb v0.4.0\n", 'shoop_rt_host_native'), [])

    def test_covers_the_worklet_host(self):
        self.assertIn("shoop_rt_host_worklet", boundary.SCOPES["engine"])
        self.assertEqual(boundary.violations("shoop_rt_host_worklet v0.3.0\nshoop_rt_engine v0.3.0\nrt_runtime v0.3.0\n", 'shoop_rt_host_worklet'), [])

    def test_rejects_application_and_legacy_crates(self):
        names = sorted(boundary.ENGINE)
        self.assertEqual(boundary.violations("\n".join(f"{name} v1.0.0" for name in names), 'shoop_rt_engine'), names)

    def test_portable_connections_cannot_depend_on_the_engine_or_a_carrier(self):
        tree = "shoop_rt_transport v0.3.0\nshoop_rt_engine v0.3.0\nshoop_rt_transport_socket v0.3.0\n"
        self.assertEqual(boundary.violations(tree, "shoop_rt_transport"),
                         ["shoop_rt_engine", "shoop_rt_transport_socket"])

    def test_schema_cannot_depend_on_connection_machinery(self):
        self.assertEqual(boundary.violations("shoop_rt_transport v0.3.0\n", "shoop_rt_protocol"),
                         ["shoop_rt_transport"])

    def test_shared_records_and_carriers_reject_engine_semantics(self):
        for package in ("shoop_transport", "shoop_transport_socket", "shoop_transport_shared_memory"):
            self.assertIn(package, boundary.SCOPES["engine"])
            for dependency in ("shoop_rt_protocol", "shoop_rt_transport", "shoop_rt_engine"):
                self.assertEqual(boundary.violations(dependency + " v0.3.0\n", package), [dependency])

    def test_shared_records_cannot_depend_on_concrete_carriers(self):
        self.assertEqual(boundary.violations("shoop_transport_socket v0.3.0\n", "shoop_transport"),
                         ["shoop_transport_socket"])
        self.assertEqual(boundary.violations("shoop_transport v0.3.0\n", "shoop_rt_protocol"), [])

    def test_app_contracts_and_client_stay_portable_without_a_runtime_or_carrier(self):
        for package in ("shoop_app_protocol", "shoop_app_client"):
            self.assertIn(package, boundary.SCOPES["engine"])
            allowed = "shoop_app_api v0.3.0\nshoop_app_protocol v0.3.0\nshoop_transport v0.3.0\n"
            self.assertEqual(boundary.violations(allowed, package), [])
            for dependency in ("shoop_app", "shoop_backend", "shoop_scripting", "shoop_rt_protocol", "shoop_transport_socket", "cpal"):
                self.assertEqual(boundary.violations(dependency + " v0.3.0\n", package), [dependency])

    def test_app_connection_uses_only_portable_contracts(self):
        self.assertIn("shoop_app_connection", boundary.SCOPES["engine"])
        self.assertEqual(boundary.violations("shoop_app v0.3.0\nshoop_transport v0.3.0\n", "shoop_app_connection"), [])
        for dependency in ("shoop_app_client", "shoop_host", "shoop_scripting", "shoop_transport_socket", "shoop_rt_engine", "cpal"):
            self.assertEqual(boundary.violations(dependency + " v0.3.0\n", "shoop_app_connection"), [dependency])

    def test_neutral_backend_contract_has_no_engine_or_application_runtime(self):
        self.assertIn("shoop_backend", boundary.SCOPES["engine"])
        self.assertEqual(boundary.violations("shoop_app_api v0.3.0\nshoop_latency v0.3.0\n", "shoop_backend"), [])
        for dependency in ("shoop_engine", "shoop_app", "shoop_scripting", "shoop_rt_host_native"):
            self.assertEqual(boundary.violations(dependency + " v0.3.0\n", "shoop_backend"), [dependency])

    def test_adapter_uses_only_portable_engine_connections(self):
        tree = "shoop_rt_backend v0.3.0\nshoop_backend v0.3.0\nshoop_rt_protocol v0.3.0\nshoop_rt_transport v0.3.0\n"
        self.assertEqual(boundary.violations(tree, "shoop_rt_backend"), [])
        for dependency in ("shoop_engine", "shoop_app", "shoop_scripting", "shoop_rt_engine", "shoop_rt_host_process", "shoop_rt_transport_socket", "shoop_rt_transport_shared_memory"):
            self.assertEqual(boundary.violations(tree + dependency + " v0.3.0\n", "shoop_rt_backend"), [dependency])


class HostDspBoundaryTests(unittest.TestCase):
    def test_disabled_implementations_do_not_enter_normal_or_build_dependencies(self):
        tree = "shoop_dsp v0.3.0\nfundsp v0.23.0\noxisynth v0.1.0\nsoundfont v0.1.0\n"
        self.assertEqual(boundary.violations(tree, "shoop_dsp", ()), ["fundsp", "oxisynth", "soundfont"])
        self.assertEqual(boundary.violations(tree, "shoop_dsp", ("builtin-fx",)), ["oxisynth", "soundfont"])
        self.assertEqual(boundary.violations(tree, "shoop_dsp", ("oxisynth",)), ["fundsp"])
        self.assertEqual(boundary.violations(tree, "shoop_dsp", ("builtin-fx", "oxisynth")), [])

    def test_enabling_dsp_never_admits_legacy_application_or_devices(self):
        tree = "shoop_dsp v0.3.0\nshoop_engine v0.3.0\nshoop_app v0.3.0\ncpal v0.18.0\n"
        self.assertEqual(boundary.violations(tree, "shoop_dsp", ("builtin-fx", "oxisynth")), ["cpal", "shoop_app", "shoop_engine"])



class BoundaryCommandTests(unittest.TestCase):
    def test_audits_every_target_and_feature_variant_with_production_edges(self):
        def tree(command, **kwargs):
            owner = command[command.index("-p") + 1]
            return subprocess.CompletedProcess(command, 0, owner + " v1.0.0\n", "")

        with patch.object(boundary.subprocess, "run", side_effect=tree) as run, contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(boundary.main([]), 0)
        commands = [call.args[0] for call in run.call_args_list]
        expected = {(owner, target, features, policy) for packages in boundary.SCOPES.values()
                    for package in packages for owner, features, policy in boundary.checks(package)
                    for target in boundary.TARGETS}
        actual = set()
        for command in commands:
            owner = command[command.index("-p") + 1]
            target = command[command.index("--target") + 1]
            features = (tuple(command[command.index("--features") + 1].split(",")) if "--features" in command
                        else None if "--all-features" in command else ())
            policy = "shoop_app_client_runtime" if owner == "shoop_app_client" and features else owner
            actual.add((owner, target, features, policy))
            self.assertIn("--locked", command)
            self.assertEqual(command[command.index("-e") + 1], "normal,build")
            self.assertEqual("--no-default-features" in command,
                             features is not None and policy != "shoop_app_client_runtime")
        self.assertEqual(actual, expected)
        self.assertEqual(len(commands), len(actual))

    def test_missing_root_cargo_failure_and_forbidden_dependency_fail(self):
        for code, output in [(0, "unrelated v1.0.0\n"), (7, ""),
                             (0, "rt_runtime v1.0.0\nshoop_engine v1.0.0\n")]:
            with self.subTest(code=code, output=output), patch.object(
                boundary.subprocess, "run", return_value=subprocess.CompletedProcess([], code, output, "")
            ), contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(boundary.main(["--package", "rt_runtime", "--target", "test"]), code or 1)
