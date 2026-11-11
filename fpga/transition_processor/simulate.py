#!/usr/bin/env python3
"""Compare transition-processor RTL, Rust hardware model and native resolver."""
import argparse
import os
from pathlib import Path
import subprocess
import sys
import xml.etree.ElementTree as ET
from cocotb.runner import get_runner

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
CONFIGS = {"default": (64, 64, 192), "small": (7, 11, 25), "minimum": (1, 1, 3), "wide": (130, 130, 390)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", choices=["all", *CONFIGS], default="all")
    parser.add_argument("--random-cases", type=int, default=100)
    parser.add_argument("--case", type=Path)
    parser.add_argument("--waves", action="store_true")
    parser.add_argument("--output", type=Path, default=ROOT / "work/fpga-transition-sim")
    args = parser.parse_args()
    if args.random_cases < 0 or (args.case and args.config == "all"):
        parser.error("use nonnegative --random-cases and one --config for --case")
    executable = Path(os.environ.get("SHOOP_FPGA_REFERENCE", ROOT / "target/debug/fpga-reference")).resolve()
    if "SHOOP_FPGA_REFERENCE" not in os.environ:
        subprocess.run(["cargo", "build", "--locked", "-p", "rt_fpga_testbench"], cwd=ROOT, check=True)
    sys.path[:0] = [str(HERE / "tests"), str(ROOT / "scripts")]
    if sys.prefix != sys.base_prefix:
        os.environ.setdefault("VIRTUAL_ENV", sys.prefix)
    configurations = CONFIGS if args.config == "all" else {args.config: CONFIGS[args.config]}
    for name, depths in configurations.items():
        output = args.output.resolve() / name
        output.mkdir(parents=True, exist_ok=True)
        runner = get_runner("verilator")
        runner.build(verilog_sources=[HERE / "rtl/tp_ram.sv", HERE / "rtl/tp_priority.sv", HERE / "rtl/transition_processor.sv",
                                     HERE / "tests/transition_processor_tb.sv"],
                     hdl_toplevel="transition_processor_tb",
                     parameters=dict(zip(("LOOP_DEPTH", "REQUEST_DEPTH", "EVENT_DEPTH"), depths)),
                     build_args=["--Wall", "--timing", "--timescale", "1ns/1ps"],
                     build_dir=output / "build", always=True, waves=args.waves, log_file=output / "build.log")
        results = runner.test(hdl_toplevel="transition_processor_tb", test_module="test_transition_processor",
                              testcase="directed_against_rust" if args.case else None, seed=20261110,
                              test_dir=output, waves=args.waves, log_file=output / "simulation.log",
                              extra_env={"TP_DEPTHS": ",".join(map(str, depths)), "TP_RANDOM_CASES": str(args.random_cases),
                                         "TP_OUTPUT": str(output), "TP_REFERENCE": str(executable),
                                         "TP_CASE": str(args.case.resolve()) if args.case else ""})
        if not results.is_file():
            raise SystemExit(f"No simulation results; see {output / 'simulation.log'}")
        report = ET.parse(results).getroot()
        if (len(list(report.iter("testcase"))) != (1 if args.case else 5)
                or any(list(report.iter(tag)) for tag in ("failure", "error", "skipped"))):
            raise SystemExit(f"Behavioral simulation failed; see {output / 'simulation.log'}")
        print(f"{name}: behavioral simulation passed; results in {output}", flush=True)


if __name__ == "__main__":
    main()
