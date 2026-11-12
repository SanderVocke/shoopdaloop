#!/usr/bin/env python3
"""Run arithmetic vectors without an FPGA reference execution model."""
import argparse
import os
from pathlib import Path
import subprocess
import sys
import xml.etree.ElementTree as ET

from cocotb.runner import get_runner
from simulate import CONFIGS, HERE, ROOT


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", choices=["all", *CONFIGS], default="all")
    parser.add_argument("--output", type=Path, default=ROOT / "work/production-numeric")
    args = parser.parse_args()
    subprocess.run(["cargo", "build", "--locked", "-p", "rt_audio_math", "--example", "vectors"], cwd=ROOT, check=True)
    sys.path[:0] = [str(HERE / "tests"), str(ROOT / "fpga/state_processor/tests")]
    if sys.prefix != sys.base_prefix:
        os.environ.setdefault("VIRTUAL_ENV", sys.prefix)
    sources = [ROOT / "fpga/state_processor/rtl" / f"{name}.sv"
               for name in ("sp_ram", "sp_job_fifo", "sp_engine", "state_processor")]
    sources += [HERE / "rtl" / f"{name}.sv"
                for name in ("jp_sample_ram", "jp_divider", "job_processor", "audio_pipeline")]
    configs = CONFIGS if args.config == "all" else {args.config: CONFIGS[args.config]}
    for name, capacities in configs.items():
        output = args.output.resolve() / name
        output.mkdir(parents=True, exist_ok=True)
        runner = get_runner("verilator")
        runner.build(verilog_sources=sources, includes=[ROOT / "fpga/state_processor/rtl"],
                     hdl_toplevel="audio_pipeline", parameters=dict(zip(
                         ("PROGRAM_DEPTH", "STATE_DEPTH", "JOB_DEPTH", "MAX_FRAMES", "BUFFERS", "INPUT_PORTS", "OUTPUT_PORTS"), capacities)),
                     build_args=["--Wall", "--timescale", "1ns/1ps"], build_dir=output / "build",
                     always=True, log_file=output / "build.log")
        result = runner.test(hdl_toplevel="audio_pipeline", test_module="test_numeric_contract",
                             test_dir=output, log_file=output / "simulation.log", extra_env={
                                 "AUDIO_CAPACITIES": ",".join(map(str, capacities)),
                                 "NUMERIC_VECTORS": str(ROOT / "target/debug/examples/vectors")})
        report = ET.parse(result).getroot()
        if len(list(report.iter("testcase"))) != 1 or any(list(report.iter(tag)) for tag in ("failure", "error", "skipped")):
            raise SystemExit(f"numeric contract failed; see {output}")
        print(f"{name}: production numeric contract passed")


if __name__ == "__main__":
    main()
