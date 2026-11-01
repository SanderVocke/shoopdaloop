import importlib.util
import pathlib
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "check_rt_engine_boundary.py"
SPEC = importlib.util.spec_from_file_location("engine_boundary", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class EngineBoundaryTests(unittest.TestCase):
    def test_allows_the_engine_stack(self):
        self.assertEqual(MODULE.violations(
            "shoop_rt_engine v0.3.0\nshoop_rt_protocol v0.3.0\nshoop_loops v0.3.0\n"
            "rt_runtime v0.3.0\nrtrb v0.4.0\n"
        ), [])

    def test_covers_the_native_host_and_allows_its_device_libraries(self):
        self.assertIn("shoop_rt_host_native", MODULE.PACKAGES)
        self.assertEqual(MODULE.violations(
            "shoop_rt_host_native v0.3.0\nshoop_rt_engine v0.3.0\ncpal v0.17.1\n"
            "midir v0.11.0\nrtrb v0.4.0\n"
        ), [])

    def test_covers_the_worklet_host(self):
        self.assertIn("shoop_rt_host_worklet", MODULE.PACKAGES)
        self.assertEqual(MODULE.violations(
            "shoop_rt_host_worklet v0.3.0\nshoop_rt_engine v0.3.0\nrt_runtime v0.3.0\n"
        ), [])

    def test_rejects_application_and_legacy_crates(self):
        names = sorted(MODULE.FORBIDDEN)
        self.assertEqual(MODULE.violations("\n".join(f"{name} v1.0.0" for name in names)), names)

    def test_portable_connections_cannot_depend_on_the_engine_or_a_carrier(self):
        tree = "shoop_rt_transport v0.3.0\nshoop_rt_engine v0.3.0\nshoop_rt_transport_socket v0.3.0\n"
        self.assertEqual(MODULE.violations(tree, "shoop_rt_transport"),
                         ["shoop_rt_engine", "shoop_rt_transport_socket"])

    def test_schema_cannot_depend_on_connection_machinery(self):
        self.assertEqual(MODULE.violations("shoop_rt_transport v0.3.0\n", "shoop_rt_protocol"),
                         ["shoop_rt_transport"])

    def test_shared_records_and_carriers_reject_engine_semantics(self):
        for package in ("shoop_transport", "shoop_transport_socket", "shoop_transport_shared_memory"):
            self.assertIn(package, MODULE.PACKAGES)
            for dependency in ("shoop_rt_protocol", "shoop_rt_transport", "shoop_rt_engine"):
                self.assertEqual(MODULE.violations(dependency + " v0.3.0\n", package), [dependency])

    def test_shared_records_cannot_depend_on_concrete_carriers(self):
        self.assertEqual(MODULE.violations("shoop_transport_socket v0.3.0\n", "shoop_transport"),
                         ["shoop_transport_socket"])
        self.assertEqual(MODULE.violations("shoop_transport v0.3.0\n", "shoop_rt_protocol"), [])

    def test_app_contracts_and_client_stay_portable_without_a_runtime_or_carrier(self):
        for package in ("shoop_app_protocol", "shoop_app_client"):
            self.assertIn(package, MODULE.PACKAGES)
            allowed = "shoop_app_api v0.3.0\nshoop_app_protocol v0.3.0\nshoop_transport v0.3.0\n"
            self.assertEqual(MODULE.violations(allowed, package), [])
            for dependency in ("shoop_app", "shoop_backend", "shoop_scripting", "shoop_rt_protocol", "shoop_transport_socket", "cpal"):
                self.assertEqual(MODULE.violations(dependency + " v0.3.0\n", package), [dependency])

    def test_adapter_uses_only_portable_engine_connections(self):
        tree = "shoop_rt_backend v0.3.0\nshoop_backend v0.3.0\nshoop_rt_protocol v0.3.0\nshoop_rt_transport v0.3.0\n"
        self.assertEqual(MODULE.violations(tree, "shoop_rt_backend"), [])
        for dependency in ("shoop_rt_engine", "shoop_rt_host_process", "shoop_rt_transport_socket", "shoop_rt_transport_shared_memory"):
            self.assertEqual(MODULE.violations(tree + dependency + " v0.3.0\n", "shoop_rt_backend"), [dependency])


if __name__ == "__main__":
    unittest.main()
