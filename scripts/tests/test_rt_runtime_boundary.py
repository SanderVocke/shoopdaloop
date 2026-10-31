import importlib.util
import pathlib
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "check_rt_runtime_boundary.py"
SPEC = importlib.util.spec_from_file_location("runtime_boundary", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class RuntimeBoundaryTests(unittest.TestCase):
    def test_accepts_generic_dependencies(self):
        self.assertEqual(MODULE.violations("rt_runtime v0.3.0\nrtrb v0.4.0\nthiserror v2.0.0\n"), [])

    def test_rejects_domain_host_ui_and_browser_dependencies(self):
        tree = "\n".join(f"{name} v1.0.0 (*)" for name in (
            "shoop_engine", "shoop_tracing", "cpal", "eframe", "web-sys", "wasm-bindgen", "js-sys",
        ))
        self.assertEqual(MODULE.violations(tree), [
            "cpal", "eframe", "js-sys", "shoop_engine", "shoop_tracing", "wasm-bindgen", "web-sys",
        ])


if __name__ == "__main__":
    unittest.main()
