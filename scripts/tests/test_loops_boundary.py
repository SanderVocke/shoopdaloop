import importlib.util
import pathlib
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "check_loops_boundary.py"
SPEC = importlib.util.spec_from_file_location("loop_boundary", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class LoopBoundaryTests(unittest.TestCase):
    def test_allows_only_the_loop_root_and_generic_dependencies(self):
        self.assertEqual(MODULE.violations(
            "shoop_loops v0.3.0\nrt_runtime v0.3.0\nrtrb v0.4.0\n"
        ), [])

    def test_rejects_other_domain_and_host_dependencies(self):
        names = ["shoop_engine", "shoop_tracing", "shoop_backend", "shoop_app",
                 "shoop_wasm_test_support", "web-sys", "wasm-bindgen", "cpal"]
        self.assertEqual(MODULE.violations("\n".join(f"{name} v1.0.0" for name in names)), sorted(names))

    def test_runtime_policy_still_rejects_the_loop_crate(self):
        self.assertEqual(MODULE.RUNTIME.violations("shoop_loops v0.3.0\n"), ["shoop_loops"])


if __name__ == "__main__":
    unittest.main()
