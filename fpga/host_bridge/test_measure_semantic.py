"""Offline measurement snapshot and accounting regressions."""
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest

from measure_semantic import SCRIPT, digest
from measure_semantic_pipeline import BOUNDARIES, ROOT, freeze

ARCHIVE = ROOT / "docs/fpga/command-fifo-implementation/reports/pipeline-synthesis-9b8cd12c8"


class MeasurementTests(unittest.TestCase):
    def test_consumer_tcl_is_unchanged(self):
        old = ROOT / "docs/fpga/command-fifo-implementation/reports/consumer-route-2cd453c8/run.tcl"
        self.assertEqual(old.read_text().replace("if {1}", "if {0}"), SCRIPT.replace("ROUTE_ENABLED", "0"))

    def test_profile_snapshot_preserves_measured_probe_except_neutral_fault_binding(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory)
            manifest, script, profile = freeze(source, digest, SCRIPT)
            probe = source / "semantic_profile.sv"
            binding = "        .pipeline_fault_cause(32'd0),\n"
            self.assertEqual(probe.read_text().count(binding), 1)
            self.assertEqual(probe.read_text().replace(binding, ""), (ARCHIVE / "semantic_profile.sv").read_text())
            self.assertEqual(profile["fixed_fault_inputs"], {"pipeline_fault_cause": "32'd0"})
            self.assertFalse(profile["mmio_epoch_reset_coupled"])
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

    def test_indexed_mmio_profile_preserves_geometry_and_uses_disjoint_nested_boundaries(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory)
            manifest, script, profile = freeze(source, digest, SCRIPT, mmio=True)
            probe = (source / "semantic_profile.sv").read_text()
            self.assertTrue(profile["indexed_mmio"])
            self.assertTrue(profile["standard_optimization"])
            self.assertTrue(profile["mmio_epoch_reset_coupled"])
            previous = ROOT / "docs/fpga/command-fifo-implementation/reports/coupled-epoch-519455096/semantic_profile.sv"
            private = {"cfg_valid", "cfg_state", "cfg_loop", "cfg_write", "cfg_address", "cfg_write_data",
                       "audio_cfg_valid", "audio_cfg_space", "audio_cfg_write", "audio_cfg_address", "audio_cfg_write_data",
                       "store_cfg_valid", "store_cfg_space", "store_cfg_write", "store_cfg_address", "store_cfg_write_data",
                       "store_cmd_valid", "store_cmd_opcode", "store_cmd_view", "store_cmd_start",
                       "store_cmd_length", "store_cmd_count", "store_response_ready", "trace_ready"}
            pattern = re.compile(r"\b(?:" + "|".join(sorted(private)) + r")\b")
            lines = previous.read_text().splitlines(keepends=True)
            self.assertEqual(sum(bool(pattern.search(line)) for line in lines), 48)
            self.assertEqual(probe, "".join(line for line in lines if not pattern.search(line)))
            self.assertIsNone(pattern.search(probe))
            self.assertTrue(profile["low_range_service_included"])
            self.assertFalse(profile["configuration_service_only"])
            self.assertTrue(profile["clear_view_service_included"])
            self.assertTrue(profile["optional_trace_mmio_included"])
            self.assertFalse(profile["complete_preparation_service_included"])
            self.assertTrue(profile["abort_mmio_included"])
            self.assertTrue(profile["qualified_fault_sidebands_included"])
            self.assertEqual(profile["fixed_fault_inputs"], {})
            self.assertIn("input wire audio_fault", probe)
            self.assertIn("input wire watchdog_fault", probe)
            self.assertNotIn("input wire [31:0] pipeline_fault_cause", probe)
            self.assertIn("fifo_mmio_pipeline_axi #(", probe)
            self.assertIn("input wire [11:0] s_axi_awaddr", probe)
            self.assertNotIn("input wire host_valid", probe)
            for name, value in {"LOOP_DEPTH": 128, "STATE_DEPTH": 512, "BUFFERS": 512,
                                "VIEWS": 512, "PROGRAM_DEPTH": 1024, "AXI_DATA_WIDTH": 64,
                                "PHYSICAL_BASE": 0x30000000, "PHYSICAL_BYTES_LIMIT": 128 * 1024 * 1024}.items():
                self.assertEqual(profile["parameters"][name], value)
            expected = {name: path.replace("dut/", "dut/pipeline/", 1) for name, path in BOUNDARIES.items()}
            self.assertEqual(profile["excluded_execution_boundaries"], expected)
            for path in [*expected.values(), "dut/mmio", "dut/pipeline/recovery", "dut/pipeline/retained_fault"]:
                self.assertIn(f"get_cells -quiet {path}", script)
            self.assertLess(script.index("/source/fpga/host_bridge/rtl/semantic_registers.sv"),
                            script.index("/source/fpga/host_bridge/rtl/fifo_mmio.sv"))
            self.assertIn("component-optimized-utilization.rpt", script)
            self.assertIn("component-optimized-timing.rpt", script)
            for item in manifest:
                path = source / ("semantic_profile.sv" if item["path"] == "generated/semantic_profile.sv" else item["path"])
                self.assertEqual(digest(path), item["sha256"])

    def test_mmio_flag_requires_pipeline_before_creating_output(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "invalid"
            result = subprocess.run([sys.executable, str(ROOT / "fpga/host_bridge/measure_semantic.py"),
                                     str(output), "--mmio"], capture_output=True, text=True)
            self.assertEqual(result.returncode, 2)
            self.assertIn("--mmio requires --pipeline", result.stderr)
            self.assertFalse(output.exists())

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
