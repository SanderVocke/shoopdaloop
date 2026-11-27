#!/usr/bin/env python3
"""Check the candidate indexed ABI independently of the deployed mailbox ABI."""
import copy
import json
from pathlib import Path
import unittest
from unittest.mock import patch

from generate_registers import HERE, ROOT, semantic_definitions


class SemanticRegistersTests(unittest.TestCase):
    def test_generated_stream_registers_are_single_words(self):
        spec = json.loads((HERE / "semantic_registers.json").read_text())
        self.assertEqual(spec["constants"]["VERSION"], 16)
        self.assertEqual(spec["constants"]["LOOP_LIMIT"], 128)
        self.assertEqual(spec["registers"]["COMMAND_DATA"], "0x20c")
        self.assertEqual(spec["registers"]["COMMAND_PUBLISH"], "0x210")
        self.assertEqual(spec["registers"]["RESULT_DATA"], "0x28c")
        self.assertEqual(spec["registers"]["SNAPSHOT_DATA"], "0x30c")
        self.assertEqual(spec["constants"]["SERVICE_MEDIA_COMMAND"], 17)
        for name, address in {"MEDIA_STATUS": "0x400", "MEDIA_TAG": "0x404", "MEDIA_WRITE": "0x408",
                              "MEDIA_READ": "0x40c", "MEDIA_RECEIPT": "0x410"}.items():
            self.assertEqual(spec["registers"][name], address)
        for path, lines in semantic_definitions():
            self.assertEqual(path.read_text(), "\n".join(lines) + "\n")

    def test_candidate_low_subset_retains_legacy_addresses_without_retired_operations(self):
        spec = json.loads((HERE / "semantic_registers.json").read_text())
        legacy = json.loads((HERE / "registers.json").read_text())
        for name, address in spec["registers"].items():
            if int(address, 16) < 0x150:
                self.assertEqual(address, legacy["registers"][name])
        self.assertEqual(spec["constants"]["DISCOVERY_ID"], legacy["id"])
        self.assertNotIn("operations", spec)
        self.assertEqual(spec["registers"]["ABORT"], "0x110")

    def test_legacy_schema_and_generated_version_remain_fifteen(self):
        self.assertEqual(json.loads((HERE / "registers.json").read_text())["version"], 15)
        self.assertIn("HOST_VERSION = 32'd15", (HERE / "rtl/host_registers.sv").read_text())
        self.assertIn("HOST_VERSION: u32 = 15", (ROOT / "src/rust/rt_fpga_rtl/src/host/registers.rs").read_text())

    def test_invalid_addresses_constants_and_receipts_are_rejected(self):
        spec = json.loads((HERE / "semantic_registers.json").read_text())
        for key, name, value in [
            ("registers", "COMMAND_DATA", "0x210"),
            ("registers", "COMMAND_DATA", "0x20d"),
            ("registers", "COMMAND_DATA", "0x14c"),
            ("registers", "COMMAND_DATA", "0x1000"),
            ("registers", "COMMAND_DATA", "0x04c"),
            ("registers", "COMMAND_DATA", "0x0b4"),
            ("registers", "COMMAND_DATA", "0x4b8"),
            ("registers", "TRACE_DATA", "0xfd8"),
            ("registers", "MEDIA_WRITE", "0x40c"),
            ("registers", "MEDIA_STATUS", "0x30c"),
            ("registers", "MEDIA_READ", "0x1000"),
            ("constants", "TRACE_WORDS", 0),
            ("constants", "TRACE_WORDS", 12),
            ("constants", "PAYLOAD_WORDS", 0),
            ("constants", "PAYLOAD_WORDS", 13),
            ("constants", "VERSION", -1),
            ("constants", "COMMAND_CYCLES", 1 << 32),
            ("receipts", "FULL", 0),
            ("receipts", "EXHAUSTED", 16),
        ]:
            invalid = copy.deepcopy(spec)
            invalid[key][name] = value
            with self.subTest(key=key, name=name, value=value):
                with patch.object(Path, "read_text", return_value=json.dumps(invalid)):
                    with self.assertRaises(ValueError):
                        semantic_definitions()


if __name__ == "__main__":
    unittest.main()
