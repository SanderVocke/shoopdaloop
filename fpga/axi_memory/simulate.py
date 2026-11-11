#!/usr/bin/env python3
"""Exercise the AXI endpoint and one-word adapter independently."""
import argparse
import os
from pathlib import Path
import sys
import xml.etree.ElementTree as ET
from cocotb.runner import get_runner

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE/"sim"))
from options import add_options, environment


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data-width", choices=("32", "64", "all"), default="all")
    add_options(parser)
    parser.add_argument("--output", type=Path, default=HERE.parents[1]/"work/axi-memory")
    args = parser.parse_args()
    try:
        memory_env = environment(args)
    except ValueError as error:
        parser.error(str(error))
    sys.path[:0] = [str(HERE/"sim"), str(HERE/"tests")]
    if sys.prefix != sys.base_prefix:
        os.environ.setdefault("VIRTUAL_ENV", sys.prefix)
    for width in ((32, 64) if args.data_width == "all" else (int(args.data_width),)):
        for word in (False, True):
            top = "word_axi" if word else "axi_master"
            output = args.output.resolve()/f"{top}-{width}"
            output.mkdir(parents=True, exist_ok=True)
            runner = get_runner("verilator")
            runner.build(verilog_sources=[HERE/"rtl/axi_master.sv", HERE/"rtl/word_axi.sv"],
                         hdl_toplevel=top, parameters={"DATA_WIDTH": width},
                         build_args=["--Wall", "--timescale", "1ns/1ps"], build_dir=output/"build",
                         log_file=output/"build.log", always=True)
            result = runner.test(hdl_toplevel=top, test_module="test_word" if word else "test_axi",
                                 test_dir=output, log_file=output/"simulation.log",
                                 extra_env={"AXI_OUTPUT": str(output), **memory_env})
            report = ET.parse(result).getroot()
            if len(list(report.iter("testcase"))) != (1 if word else 2) or any(list(report.iter(t)) for t in ("failure", "error", "skipped")):
                raise SystemExit(f"AXI simulation failed: {output/'simulation.log'}")
            print(f"{top}/{width}: passed", flush=True)


if __name__ == "__main__":
    main()
