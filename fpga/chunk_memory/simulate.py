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
sys.path.insert(0, str(ROOT/"fpga/axi_memory/sim"))
from options import add_options, environment, direct_sources


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", choices=["all", *CONFIGS], default="all")
    parser.add_argument("--random-cases", type=int, default=4)
    parser.add_argument("--output", type=Path, default=ROOT / "work/fpga-chunk-memory")
    parser.add_argument("--waves", action="store_true")
    parser.add_argument("--case", type=Path)
    parser.add_argument("--retire-chunks",type=int,default=1)
    add_options(parser, legacy=True)
    args = parser.parse_args()
    try:
        memory_env = environment(args)
    except ValueError as error:
        parser.error(str(error))
    axi = args.memory_model != "legacy"
    if args.random_cases < 0:
        parser.error("random-cases must be nonnegative")
    if args.case and args.config == "all":
        parser.error("--case requires one --config")
    subprocess.run(["cargo", "build", "--locked", "-p", "rt_fpga_testbench"], cwd=ROOT, check=True)
    sys.path[:0] = [str(HERE / "tests"), str(ROOT / "scripts")]
    if sys.prefix != sys.base_prefix:
        os.environ.setdefault("VIRTUAL_ENV", sys.prefix)
    for name, capacities in (CONFIGS if args.config == "all" else {args.config: CONFIGS[args.config]}).items():
        if args.retire_chunks>1:capacities=(capacities[0]*args.retire_chunks,*capacities[1:])
        output = args.output.resolve() / name
        output.mkdir(parents=True, exist_ok=True)
        runner = get_runner("verilator")
        top = "chunk_memory_axi" if axi else "chunk_memory"
        sources = [HERE/"rtl/chunk_ram.sv", HERE/"rtl/chunk_memory.sv"]
        parameters = dict(zip(("SLOTS", "CHUNK_FRAMES", "READ_BUFFERS", "WRITE_BUFFERS"), capacities))
        parameters['RETIRE_CHUNKS']=args.retire_chunks
        if axi:
            sources += [ROOT/"fpga/axi_memory/rtl"/name for name in ("axi_master.sv", "chunk_dma.sv", "word_dma.sv", "word_axi.sv", "chunk_axi.sv", "chunk_memory_axi.sv")]
            parameters.update(AXI_DATA_WIDTH=args.axi_data_width,PHYSICAL_BASE=args.physical_base,BURST_MEMORY=int(not args.word_transfers))
        runner.build(verilog_sources=direct_sources(sources,top,args.memory_model), includes=[ROOT/"fpga/state_processor/rtl"],
                     hdl_toplevel=top, parameters=parameters,
                     build_args=["--Wall", "--timescale", "1ns/1ps"], build_dir=output / "build",
                     always=True, waves=args.waves, log_file=output / "build.log")
        result = runner.test(hdl_toplevel=top, test_module="test_block_retire" if args.retire_chunks>1 else "test_chunk_memory", seed=20261111,
                             testcase="conformance" if args.case else None,
                             test_dir=output, waves=args.waves, log_file=output / "simulation.log",
                             extra_env={"CHUNK_CAPACITIES": ",".join(map(str, capacities)),"CHUNK_RETIRE_CHUNKS":str(args.retire_chunks),
                                        "CHUNK_REFERENCE": str(ROOT / "target/debug/fpga-reference"),
                                        "CHUNK_OUTPUT": str(output), "CHUNK_RANDOM_CASES": str(args.random_cases),
                                        "CHUNK_CASE": str(args.case.resolve()) if args.case else "", **memory_env})
        report = ET.parse(result).getroot()
        if len(list(report.iter("testcase"))) != (1 if args.case or args.retire_chunks>1 else 4) or any(list(report.iter(tag)) for tag in ("failure", "error", "skipped")):
            raise SystemExit(f"Chunk-memory comparison failed: {output / 'simulation.log'}")
        print(f"{name}: chunk-memory comparisons and protocol tests passed", flush=True)


if __name__ == "__main__":
    main()
