"""Validate firmware policy input before a cross-build can consume it."""
import json
from pathlib import Path
import sys
import tempfile
import unittest

PLATFORM = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PLATFORM))
import engine_policy


class Policy(unittest.TestCase):
    def test_default_and_enabled_configuration_are_independent(self):
        default = engine_policy.load()
        enabled = engine_policy.load(PLATFORM / "input-history.json")
        self.assertEqual(default["input_history"]["seconds"], 0)
        self.assertEqual(enabled["input_history"]["seconds"], 30)
        self.assertEqual((enabled["tracks"], enabled["loops"]), (3, 20))
        environment = engine_policy.environment(enabled)
        self.assertEqual(environment["SHOOP_INPUT_HISTORY_SECONDS"], "30")
        self.assertEqual(environment["SHOOP_ENGINE_TRACKS"], "3")
        self.assertEqual(default, engine_policy.load())

    def test_partial_policy_keeps_explicit_defaults(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "policy.json"
            path.write_text(json.dumps({"input_history": {"seconds": 30}}))
            policy = engine_policy.load(path)
            self.assertEqual(policy["tracks"], 8)
            self.assertEqual(policy["input_history"]["midi_records"], 65536)

    def test_invalid_settings_fail_before_build(self):
        for policy in [[], {"unknown": 1}, {"tracks": 0}, {"loops": True},
                       {"input_history": []}, {"input_history": {"unknown": 1}},
                       {"input_history": {"seconds": -1}},
                       {"input_history": {"seconds": True}},
                       {"input_history": {"seconds": 30, "writers": 0}},
                       {"input_history": {"midi_pages": 1 << 32}}]:
            with self.subTest(policy=policy), tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / "policy.json"
                path.write_text(json.dumps(policy))
                with self.assertRaises(ValueError):
                    engine_policy.load(path)


if __name__ == "__main__":
    unittest.main()
