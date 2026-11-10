#!/usr/bin/env python3
"""Compare a BRAM audio pipeline with the Rust state and job processors."""
import argparse
import os
from pathlib import Path
import subprocess
import sys
import xml.etree.ElementTree as ET
from cocotb.runner import get_runner

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
CONFIGS = {"default": (32, 16, 32, 32, 16, 2, 2), "small": (16, 8, 16, 3, 8, 2, 2)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", choices=["all", *CONFIGS], default="all")
    parser.add_argument("--random-cases", type=int, default=150)
    parser.add_argument("--seed", type=int, default=20261110)
    parser.add_argument("--case", type=Path)
    parser.add_argument("--waves", action="store_true")
    parser.add_argument("--output", type=Path, default=ROOT / "work/fpga-job-sim")
    args = parser.parse_args()
    if args.random_cases < 0 or (args.case and args.config == "all"):
        parser.error("use nonnegative --random-cases and one --config for --case")
    subprocess.run(["cargo", "build", "--locked", "-p", "rt_fpga_testbench"], cwd=ROOT, check=True)
    sys.path[:0] = [str(HERE / "tests"), str(ROOT / "fpga/state_processor/tests"), str(ROOT / "scripts")]
    if sys.prefix != sys.base_prefix:
        os.environ.setdefault("VIRTUAL_ENV", sys.prefix)
    configs = CONFIGS if args.config == "all" else {args.config: CONFIGS[args.config]}
    sources = [ROOT / "fpga/state_processor/rtl" / (name + ".sv")
               for name in ("sp_ram", "sp_job_fifo", "sp_engine", "state_processor")]
    sources += [HERE / "rtl" / (name + ".sv")
                for name in ("jp_sample_ram", "jp_divider", "job_processor", "audio_pipeline")]
    for name, capacities in configs.items():
        output = args.output.resolve() / name
        output.mkdir(parents=True, exist_ok=True)
        runner = get_runner("verilator")
        runner.build(verilog_sources=sources, includes=[ROOT / "fpga/state_processor/rtl"],
                     hdl_toplevel="audio_pipeline", parameters=dict(zip(
                         ("PROGRAM_DEPTH", "STATE_DEPTH", "JOB_DEPTH", "MAX_FRAMES", "BUFFERS", "INPUT_PORTS", "OUTPUT_PORTS"), capacities)),
                     build_args=["--Wall", "--timescale", "1ns/1ps"], build_dir=output / "build",
                     always=True, waves=args.waves, log_file=output / "build.log")
        result = runner.test(hdl_toplevel="audio_pipeline", test_module="test_audio_pipeline",
                             testcase="directed_audio" if args.case else None, seed=args.seed,
                             test_dir=output, waves=args.waves, log_file=output / "simulation.log",
                             extra_env={"AUDIO_CAPACITIES": ",".join(map(str, capacities)),
                                        "AUDIO_SEED": str(args.seed), "AUDIO_RANDOM_CASES": str(args.random_cases),
                                        "AUDIO_REFERENCE": str(ROOT / "target/debug/fpga-reference"),
                                        "AUDIO_OUTPUT": str(output), "AUDIO_CASE": str(args.case.resolve()) if args.case else ""})
        if not result.is_file():
            raise SystemExit(f"No results; see {output / 'simulation.log'}")
        report = ET.parse(result).getroot()
        if (len(list(report.iter("testcase"))) != (1 if args.case else 3)
                or any(list(report.iter(tag)) for tag in ("failure", "error", "skipped"))):
            raise SystemExit(f"Behavioral simulation failed; see {output / 'simulation.log'}")
        print(f"{name}: audio pipeline passed; results in {output}", flush=True)


if __name__ == "__main__":
    main()
