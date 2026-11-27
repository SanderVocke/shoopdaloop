#!/usr/bin/env python3
"""Check candidate MMIO composition without promoting the deployed ABI."""
import re
import unittest

from generate_semantic_mmio import AXI, HERE, generate
from generate_top import generate as host_tops
from generate_tops import header


class SemanticMmioTests(unittest.TestCase):
    def test_generator_and_normal_drift_gate_include_the_candidate(self):
        source = generate()
        self.assertEqual((HERE / "rtl/fifo_mmio_pipeline_axi.sv").read_text(), source)
        self.assertEqual(host_tops()["fifo_mmio_pipeline_axi.sv"], source)

    def test_transport_owns_packet_and_config_controls_but_bulk_media_remains_private(self):
        _, ports = header(generate())
        public = {name for _, name in ports}
        _, mmio = header((HERE / "rtl/fifo_mmio.sv").read_text())
        for declaration, name in mmio:
            if name.startswith("s_axi_") or name == "irq":
                self.assertIn(name, public)
            elif declaration.startswith("output"):
                self.assertNotIn(name, public)
        for name in ["clk", "reset", "epoch", "runtime_reset", "runtime_epoch",
                     "store_cmd_valid", "audio_xruns", "audio_fault", "watchdog_fault", "first_fault_sequence"]:
            self.assertIn(name, public)
        _, core = header((AXI / "rtl/fifo_recovery_pipeline_axi.sv").read_text())
        private = {n for p, n in core if p.startswith("input") and n.startswith(("cfg_", "audio_cfg_", "store_cfg_"))}
        self.assertEqual(len(private), 16)
        self.assertFalse(private & public)

    def test_critical_reset_epoch_retention_and_geometry_connections_are_explicit(self):
        source = generate()
        self.assertIn(".reset(reset)", source)
        self.assertIn(".runtime_reset(runtime_reset)", source)
        self.assertIn(".epoch(runtime_epoch)", source)
        self.assertIn(".fault_sequence(first_fault_sequence)", source)
        self.assertNotIn(".fault_sequence(diagnostic_sequence)", source)
        self.assertNotIn(".reset(runtime_reset)", source)
        self.assertIn(".LOOPS(LOOP_DEPTH)", source)
        self.assertIn(".SYSTEM_REGISTERS(1)", source)
        self.assertNotIn("input wire recovery_request", source)
        self.assertIn(".quantum_done(done && !faulted && error_code==0)", source)
        self.assertIn("diagnostic_cycles+32'd1", source)
        self.assertIn(".EPOCH_RESET_COUPLED(1)", source)
        self.assertIn(".SERVICE_REGISTERS(1)", source)
        self.assertIn("fifo_config_service service", source)
        self.assertIn(".live(service_live)", source)
        self.assertIn("recovery_request || faulted || diagnostic_valid", source)
        self.assertNotIn("input wire [31:0] pipeline_fault_cause", source)
        self.assertIn(".pipeline_fault(pipeline_fault || audio_fault || watchdog_fault || abort_request)", source)
        for name, cause in [("pipeline_fault", 64), ("audio_fault", 2), ("watchdog_fault", 32), ("abort_request", 16)]:
            self.assertIn(f"({name} ? 32'd{cause} : 0)", source)
        core, _ = header((AXI / "rtl/fifo_recovery_pipeline_axi.sv").read_text())
        parameters = re.findall(r"parameter\s+(?:integer|\[[^\]]+\])\s+(\w+)\s*=", core[2])
        for parameter in parameters:
            self.assertIn(f".{parameter}({parameter})", source)


if __name__ == "__main__":
    unittest.main()
