#!/usr/bin/env python3
"""Check standalone SP instruction vectors, state bounds, FIFO and reset."""

import argparse
import os
from pathlib import Path
import subprocess
import sys
import xml.etree.ElementTree as ET

from cocotb.runner import get_runner

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
CONFIGURATIONS = {"default": (256, 256, 256), "small": (7, 11, 5), "minimum": (1, 1, 1)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", choices=["all", *CONFIGURATIONS], default="all")
    parser.add_argument("--depths", type=int, nargs=3, metavar=("PROGRAM", "STATE", "JOBS"),
                        help="override presets with explicit physical memory depths")
    parser.add_argument("--random-cases", type=int, default=200)
    parser.add_argument("--seed", type=int, default=20261110)
    parser.add_argument("--case", type=Path, help="replay one serialized case or trace")
    parser.add_argument("--waves", action="store_true")
    parser.add_argument("--output", type=Path, default=ROOT / "work/fpga-state-sim")
    args = parser.parse_args()
    if args.random_cases < 0:
        parser.error("--random-cases must be nonnegative")
    if args.depths and any(not 1 <= depth <= 65536 for depth in args.depths):
        parser.error("--depths values must be in 1..65536")
    if args.case and args.config == "all" and not args.depths:
        parser.error("--case requires one --config or explicit --depths")
    sys.path[:0] = [str(HERE / "tests"), str(ROOT / "scripts")]
    if sys.prefix != sys.base_prefix:
        os.environ.setdefault("VIRTUAL_ENV", sys.prefix)
    configurations = ({"custom": tuple(args.depths)} if args.depths else
                      CONFIGURATIONS if args.config == "all" else
                      {args.config: CONFIGURATIONS[args.config]})
    for name, depths in configurations.items():
        output = args.output.resolve() / name
        output.mkdir(parents=True, exist_ok=True)
        runner = get_runner("verilator")
        runner.build(
            verilog_sources=[HERE / "rtl" / source for source in
                             ("sp_ram.sv", "sp_job_fifo.sv", "sp_engine.sv", "state_processor.sv")],
            includes=[HERE / "rtl"], hdl_toplevel="state_processor",
            parameters=dict(zip(("PROGRAM_DEPTH", "STATE_DEPTH", "JOB_DEPTH"), depths)),
            build_args=["--Wall", "--timescale", "1ns/1ps"], build_dir=output / "build", always=True,
            timescale=("1ns", "1ps"), waves=args.waves,
            log_file=output / "build.log",
        )
        environment = {
            "SP_DEPTHS": ",".join(map(str, depths)),
            "SP_RANDOM_CASES": str(args.random_cases), "SP_SEED": str(args.seed),
            "SP_ARTIFACT_DIR": str(output),
            "SP_CASE_FILE": str(args.case.resolve()) if args.case else "",
        }
        results = runner.test(
            hdl_toplevel="state_processor", test_module="test_state_processor",
            testcase="directed_against_rust" if args.case else None,
            seed=args.seed,
            test_dir=output, extra_env=environment, waves=args.waves,
            log_file=output / "simulation.log", results_xml="results.xml",
        )
        if not results.is_file():
            raise SystemExit(f"{name}: no simulation results; see {output / 'simulation.log'}")
        report = ET.parse(results).getroot()
        tests = list(report.iter("testcase"))
        if (len(tests) != (1 if args.case else 4) or list(report.iter("failure"))
                or list(report.iter("error")) or list(report.iter("skipped"))):
            raise SystemExit(f"{name}: simulation failed; see {output / 'simulation.log'}")
        print(f"{name}: {len(tests)} behavioral tests passed; results in {output}", flush=True)


if __name__ == "__main__":
    main()
