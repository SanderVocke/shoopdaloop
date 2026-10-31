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


if __name__ == "__main__":
    unittest.main()
