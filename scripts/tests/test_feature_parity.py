import importlib.util
from pathlib import Path
import unittest

PATH = Path(__file__).resolve().parents[1] / "check_feature_parity.py"
SPEC = importlib.util.spec_from_file_location("feature_parity", PATH)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class FeatureParityTests(unittest.TestCase):
    def test_new_and_retired_capabilities_require_ledger_reconciliation(self):
        api = "pub enum BackendCapability {\n NewFeature,\n}\n"
        ledger = "| `OldFeature` | AVAILABLE |"
        self.assertEqual(MODULE.violations(api, ledger), [
            "untracked capability: NewFeature", "stale capability: OldFeature"])

    def test_flags_must_point_to_a_real_restoration_item(self):
        api = "pub enum BackendCapability {\n SessionCapture,\n}\n"
        ledger = "| `SessionCapture` | P01 |"
        self.assertTrue(MODULE.violations(api, ledger))
        self.assertEqual(MODULE.violations(api, ledger + "\n| P01 OPEN | Save sessions |"), [])

    def test_duplicate_dispositions_are_rejected(self):
        api = "pub enum BackendCapability {\n MidiInjection,\n}\n"
        ledger = "| `MidiInjection` | AVAILABLE |\n" * 2
        self.assertIn("duplicate capability dispositions", MODULE.violations(api, ledger))
