#!/usr/bin/env python3
"""Compare the chunk-memory RTL service with the synchronous Rust reference."""
import argparse
import os
from pathlib import Path
import subprocess
import sys
import xml.etree.ElementTree as ET
from cocotb.runner import get_runner

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
CONFIGS = {"default": (16, 32, 4, 4), "small": (8, 4, 2, 2),
           "odd": (9, 8, 3, 3), "minimum": (3, 1, 1, 1)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", choices=["all", *CONFIGS], default="all")
    parser.add_argument("--random-cases", type=int, default=4)
    parser.add_argument("--output", type=Path, default=ROOT / "work/fpga-chunk-memory")
    parser.add_argument("--waves", action="store_true")
    parser.add_argument("--case", type=Path)
    args = parser.parse_args()
    if args.random_cases < 0:
        parser.error("random-cases must be nonnegative")
    if args.case and args.config == "all":
        parser.error("--case requires one --config")
    subprocess.run(["cargo", "build", "--locked", "-p", "rt_fpga_testbench"], cwd=ROOT, check=True)
    sys.path[:0] = [str(HERE / "tests"), str(ROOT / "scripts")]
    if sys.prefix != sys.base_prefix:
        os.environ.setdefault("VIRTUAL_ENV", sys.prefix)
    for name, capacities in (CONFIGS if args.config == "all" else {args.config: CONFIGS[args.config]}).items():
        output = args.output.resolve() / name
        output.mkdir(parents=True, exist_ok=True)
        runner = get_runner("verilator")
        runner.build(verilog_sources=[HERE / "rtl/chunk_ram.sv", HERE / "rtl/chunk_memory.sv"],
                     hdl_toplevel="chunk_memory", parameters=dict(zip(
                         ("SLOTS", "CHUNK_FRAMES", "READ_BUFFERS", "WRITE_BUFFERS"), capacities)),
                     build_args=["--Wall", "--timescale", "1ns/1ps"], build_dir=output / "build",
                     always=True, waves=args.waves, log_file=output / "build.log")
        result = runner.test(hdl_toplevel="chunk_memory", test_module="test_chunk_memory", seed=20261111,
                             testcase="conformance" if args.case else None,
                             test_dir=output, waves=args.waves, log_file=output / "simulation.log",
                             extra_env={"CHUNK_CAPACITIES": ",".join(map(str, capacities)),
                                        "CHUNK_REFERENCE": str(ROOT / "target/debug/fpga-reference"),
                                        "CHUNK_OUTPUT": str(output), "CHUNK_RANDOM_CASES": str(args.random_cases),
                                        "CHUNK_CASE": str(args.case.resolve()) if args.case else ""})
        report = ET.parse(result).getroot()
        if len(list(report.iter("testcase"))) != (1 if args.case else 3) or any(list(report.iter(tag)) for tag in ("failure", "error", "skipped")):
            raise SystemExit(f"Chunk-memory comparison failed: {output / 'simulation.log'}")
        print(f"{name}: chunk-memory comparisons and protocol tests passed", flush=True)


if __name__ == "__main__":
    main()
