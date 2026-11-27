"""Offline measurement snapshot and accounting regressions."""
import json
from pathlib import Path
import re
import tempfile
import unittest

from measure_semantic import SCRIPT, digest
from measure_semantic_pipeline import BOUNDARIES, ROOT, freeze

ARCHIVE = ROOT / "docs/fpga/command-fifo-implementation/reports/pipeline-synthesis-9b8cd12c8"


class MeasurementTests(unittest.TestCase):
    def test_consumer_tcl_is_unchanged(self):
        old = ROOT / "docs/fpga/command-fifo-implementation/reports/consumer-route-2cd453c8/run.tcl"
        self.assertEqual(old.read_text().replace("if {1}", "if {0}"), SCRIPT.replace("ROUTE_ENABLED", "0"))

    def test_profile_snapshot_resolves_widths_and_preserves_measured_probe(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory)
            manifest, script, profile = freeze(source, digest, SCRIPT)
            probe = source / "semantic_profile.sv"
            self.assertEqual(probe.read_bytes(), (ARCHIVE / "semantic_profile.sv").read_bytes())
            for width in re.findall(r"\[([^]]+)\]", probe.read_text()):
                self.assertIsNone(re.search(r"[A-Za-z_]", width))
            self.assertEqual(profile["fixed_audio_inputs"],
                             dict(run_quantum=32, run_chunk_frames=32, run_inputs=1, run_outputs=2))
            self.assertEqual(profile["parameters"]["PHYSICAL_BASE"], 0x30000000)
            self.assertEqual(profile["parameters"]["PHYSICAL_BYTES_LIMIT"], 128 * 1024 * 1024)
            self.assertFalse(any("native_audio" in name for name in profile["compiled_sources"]))
            for item in manifest:
                path = probe if item["path"] == "generated/semantic_profile.sv" else source / item["path"]
                self.assertEqual(digest(path), item["sha256"])
            for boundary in BOUNDARIES.values():
                self.assertIn(f"get_cells -quiet {boundary}", script)

    def test_packed_accounting_and_failed_gate_are_explicit(self):
        result = json.loads((ARCHIVE / "results.json").read_text())
        for key, value in result["component"].items():
            excluded = sum(row[key] for row in result["excluded_existing_execution_backend"].values())
            self.assertEqual(value - excluded, result["frontend_and_glue_before_mmio"][key])
        actual = {k: v.removeprefix("semantic_profile/") for k, v in result["excluded_paths"].items()}
        self.assertEqual(actual, BOUNDARIES)
        self.assertEqual(result["runner_status"], 1)
        self.assertTrue(result["synthesis_completed"])
        self.assertFalse(result["route_performed"])
        self.assertGreater(result["component"]["lut"], result["ceilings"]["whole_lut"])
        self.assertGreater(result["component"]["ff"], result["ceilings"]["whole_ff"])
        self.assertLessEqual(result["kernel"]["lut"], result["ceilings"]["kernel_lut"])
        self.assertLessEqual(result["kernel"]["ff"], result["ceilings"]["kernel_ff"])


if __name__ == "__main__":
    unittest.main()
