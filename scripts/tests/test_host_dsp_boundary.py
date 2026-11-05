import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location("host_dsp_boundary", Path(__file__).resolve().parents[1] / "check_host_dsp_boundary.py")
POLICY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(POLICY)


class HostDspBoundaryTests(unittest.TestCase):
    def test_disabled_implementations_do_not_enter_normal_or_build_dependencies(self):
        tree = "shoop_dsp v0.3.0\nfundsp v0.23.0\noxisynth v0.1.0\nsoundfont v0.1.0\n"
        self.assertEqual(POLICY.violations(tree, ()), ["fundsp", "oxisynth", "soundfont"])
        self.assertEqual(POLICY.violations(tree, ("builtin-fx",)), ["oxisynth", "soundfont"])
        self.assertEqual(POLICY.violations(tree, ("oxisynth",)), ["fundsp"])
        self.assertEqual(POLICY.violations(tree, ("builtin-fx", "oxisynth")), [])

    def test_enabling_dsp_never_admits_legacy_application_or_devices(self):
        tree = "shoop_dsp v0.3.0\nshoop_engine v0.3.0\nshoop_app v0.3.0\ncpal v0.18.0\n"
        self.assertEqual(POLICY.violations(tree, ("builtin-fx", "oxisynth")), ["cpal", "shoop_app", "shoop_engine"])
