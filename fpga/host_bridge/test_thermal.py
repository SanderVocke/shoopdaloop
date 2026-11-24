#!/usr/bin/env python3
"""Check the autonomous thermal policy and fail-closed bitstream hook."""
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
TCLSH = os.environ.get("TCLSH") or shutil.which("tclsh") or shutil.which("tclsh8.6")
STUB = """
set count [lindex $argv 1]
set shutdown [lindex $argv 2]
array set registers {INIT_40 16'h0000 INIT_41 16'h0000 INIT_42 16'h0800 INIT_53 16'hB0D3 INIT_57 16'hABC4}
foreach override [lrange $argv 3 end] {
    lassign [split $override =] name value
    set registers($name) $value
}
proc get_cells {args} {global count; return [lrepeat $count monitor]}
proc current_design {} {return design}
proc get_property {name object} {
    global shutdown registers
    if {$name eq "BITSTREAM.CONFIG.OVERTEMPPOWERDOWN"} {return $shutdown}
    return $registers($name)
}
source [lindex $argv 0]
"""


class Thermal(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not TCLSH:
            raise RuntimeError("Install Tcl or set TCLSH to its interpreter")

    def check_hook(self, count=1, shutdown="ENABLE", **registers):
        with tempfile.TemporaryDirectory() as directory:
            stub = Path(directory) / "test.tcl"
            stub.write_text(STUB)
            result = subprocess.run([TCLSH, str(stub), str(HERE / "zturn_v2_thermal_check.tcl"),
                                     str(count), shutdown,
                                     *[f"{name}={value}" for name, value in registers.items()]],
                                    cwd=directory, capture_output=True, text=True, timeout=10)
            report = Path(directory) / "thermal.rpt"
            return result, report.read_text() if report.exists() else None

    def test_enabled_policy(self):
        result, report = self.check_hook()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("shutdown: ENABLE", report)
        self.assertIn("does not power off the PS", report)

    def test_register_formats(self):
        for value in ("B0D3", "0xB0D3", "16'hb0d3"):
            with self.subTest(value=value):
                result, _ = self.check_hook(INIT_53=value)
                self.assertEqual(result.returncode, 0, result.stderr)

    def test_missing_or_duplicate_monitor(self):
        for count in (0, 2):
            with self.subTest(count=count):
                result, report = self.check_hook(count=count)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("exactly one", result.stderr)
                self.assertIsNone(report)

    def test_disabled_shutdown(self):
        result, report = self.check_hook(shutdown="DISABLE")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("shutdown is disabled", result.stderr)
        self.assertIsNone(report)

    def test_unsafe_registers(self):
        cases = {"INIT_40": ("16'h8000",), "INIT_41": ("16'h0001", "16'h1000"),
                 "INIT_42": ("16'h0830",),
                 "INIT_53": ("16'hB0D0", "16'hCA33", "invalid"), "INIT_57": ("16'hCA3E",)}
        for name, values in cases.items():
            for value in values:
                with self.subTest(name=name, value=value):
                    result, report = self.check_hook(**{name: value})
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn("Unsafe XADC", result.stderr)
                    self.assertIsNone(report)

    def test_rtl_thresholds(self):
        rtl = (HERE / "rtl/zynq_thermal_guard.v").read_text()
        values = {name: int(value, 16) for name, value in
                  re.findall(r"\.(INIT_[0-9A-F]+)\(16'h([0-9A-F]+)\)", rtl)}
        self.assertEqual(values["INIT_53"] & 15, 3)
        self.assertEqual(values["INIT_41"], 0)
        self.assertEqual(values["INIT_42"], 0x0800)
        trip = (values["INIT_53"] >> 4) * 503.975 / 4096 - 273.15
        lower = values["INIT_57"] * 503.975 / 65536 - 273.15
        self.assertAlmostEqual(trip, 75, delta=0.13)
        self.assertAlmostEqual(lower, 65, delta=0.02)
        self.assertIn('DONT_TOUCH = "TRUE"', rtl)
        self.assertIn(".DWE(1'b0)", rtl)
        self.assertIn(".RESET(1'b0)", rtl)

    def test_board_build_integration(self):
        build = (HERE / "zturn_v2.tcl").read_text()
        self.assertIn("[get_bd_pins thermal/clk]", build)
        self.assertIn("STEPS.WRITE_BITSTREAM.TCL.PRE $thermal_check", build)
        self.assertIn("source $thermal_check", build)
        self.assertIn("OVERTEMPPOWERDOWN ENABLE", (HERE / "zturn_v2_thermal.xdc").read_text())


if __name__ == "__main__":
    unittest.main()
