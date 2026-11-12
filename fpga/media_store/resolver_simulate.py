#!/usr/bin/env python3
"""Check isolated metadata indexing under independent stalls."""
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
    parser.add_argument("--config", choices=["all", "default", "small", "odd"], default="all")
    parser.add_argument("--seed", type=int, default=20261111)
    parser.add_argument("--output", type=Path, default=ROOT / "work/fpga-resolver-sim")
    args = parser.parse_args()
    sys.path[:0] = [str(HERE / "tests"), str(ROOT / "scripts")]
    if sys.prefix != sys.base_prefix:
        os.environ.setdefault("VIRTUAL_ENV", sys.prefix)
    for name in ("default", "small", "odd") if args.config == "all" else (args.config,):
        views, slots, cf = CONFIGS[name]
        output = args.output.resolve() / name
        output.mkdir(parents=True, exist_ok=True)
        runner = get_runner("verilator")
        runner.build(verilog_sources=[HERE / "rtl/media_prefetch_resolver.sv"],
                     hdl_toplevel="media_prefetch_resolver", parameters={"SLOTS": slots, "CHUNK_FRAMES": cf},
                     build_args=["--Wall", "--timescale", "1ns/1ps"],
                     build_dir=output / "build", always=True, log_file=output / "build.log")
        report = runner.test(hdl_toplevel="media_prefetch_resolver", test_module="test_prefetch_resolver",
                             seed=args.seed, test_dir=output, log_file=output / "simulation.log",
                             extra_env={"RESOLVER_CAPACITIES": f"{views},{slots},{cf}",
                                        "RESOLVER_OUTPUT": str(output)})
        tree = ET.parse(report).getroot()
        if len(list(tree.iter("testcase"))) != 1 or any(list(tree.iter(t)) for t in ("failure", "error", "skipped")):
            raise SystemExit(f"Resolver comparison failed; see {output / 'simulation.log'}")
        print(f"{name}: isolated resolver passed", flush=True)


if __name__ == "__main__":
    main()
