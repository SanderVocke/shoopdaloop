#!/usr/bin/env python3
"""Check media ownership/protocols or the standalone sample pipeline."""
import argparse
import os
from pathlib import Path
import subprocess
import sys
import xml.etree.ElementTree as ET
from cocotb.runner import get_runner

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
CONFIGS = {"default": (32, 32, 32), "small": (16, 8, 4), "odd": (5, 5, 8),
           "wide": (16, 32, 4), "8x8": (64, 1024, 32)}
sys.path.insert(0, str(ROOT/"fpga/axi_memory/sim"))
from options import add_options, environment, direct_sources


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", choices=["all", *CONFIGS], default="all")
    parser.add_argument("--random-cases", type=int, default=40)
    parser.add_argument("--seed", type=int, default=20261110)
    parser.add_argument("--case", type=Path)
    parser.add_argument("--waves", action="store_true")
    parser.add_argument("--deferred",action="store_true",help="exercise deferred ownership and maintenance budgets")
    parser.add_argument("--pipeline", action="store_true")
    parser.add_argument("--read-buffers", type=int, default=4)
    parser.add_argument("--write-buffers", type=int, default=4)
    parser.add_argument("--response-cycles", type=int, default=1,
                        help="minimum cycles per sample-memory response (pipeline only)")
    parser.add_argument("--timing", action="store_true",
                        help="measure record/play/reuse, or 64-loop playback with --config 8x8 (pipeline only)")
    parser.add_argument("--prefetch-analysis", action="store_true",
                        help="instrument aligned 8x8 timing and estimate within-quantum early prefetch (requires >=64 read buffers)")
    parser.add_argument("--output", type=Path, default=ROOT / "work/fpga-media-sim")
    add_options(parser, legacy=True)
    args = parser.parse_args()
    try:
        memory_env = environment(args)
    except ValueError as error:
        parser.error(str(error))
    axi = args.memory_model != "legacy"
    if axi and args.response_cycles != 1:
        parser.error("--response-cycles is for --memory-model legacy; AXI fixed mode uses --memory-cycles")
    if args.random_cases < 0 or (args.case and args.config == "all"):
        parser.error("use nonnegative --random-cases and one --config for --case")
    if min(args.read_buffers, args.write_buffers) < 1:
        parser.error("buffer capacities must be positive")
    if args.response_cycles < 1 or ((args.response_cycles != 1 or args.timing) and not args.pipeline):
        parser.error("use positive --response-cycles; delay/timing options require --pipeline")
    if args.case:
        parser.error('hardware-shaped cases are retired; use fpga/verification/simulate.py --case with a common scenario')
    if args.timing:
        command=[sys.executable,str(ROOT/'fpga/verification/simulate.py'),'--output',str(args.output),
            '--memory-model',args.memory_model,'--read-buffers',str(args.read_buffers),'--write-buffers',str(args.write_buffers),
            '--metadata-lines',str(args.metadata_lines),'--reference-lines',str(args.reference_lines),'--stack-lines',str(args.stack_lines),
            '--block-chunks',str(args.block_chunks),'--axi-data-width',str(args.axi_data_width),'--dram-library',str(args.dram_library),'--dram-config',str(args.dram_config)]
        if args.config=='8x8':command+=['--benchmark64','--config','8x8']
        if args.word_transfers:command+=['--word-transfers']
        subprocess.run(command,cwd=ROOT,check=True)
        return
    sys.path[:0] = [str(HERE / "tests"), str(ROOT / "fpga/job_processor/tests"),
                   str(ROOT / "fpga/state_processor/tests"), str(ROOT / "scripts")]
    if sys.prefix != sys.base_prefix:
        os.environ.setdefault("VIRTUAL_ENV", sys.prefix)
    configs = {name: CONFIGS[name] for name in ("default", "small", "odd")} if args.config == "all" else {args.config: CONFIGS[args.config]}
    for name, capacities in configs.items():
        if args.block_chunks > 1:
            capacities=(capacities[0],capacities[1]*args.block_chunks,capacities[2])
        output = args.output.resolve() / name
        output.mkdir(parents=True, exist_ok=True)
        runner = get_runner("verilator")
        top = "media_audio_pipeline" if args.pipeline else "media_store"
        sources = [ROOT / "fpga/chunk_memory/rtl/chunk_ram.sv", ROOT / "fpga/chunk_memory/rtl/chunk_memory.sv",
                   HERE / "rtl/media_ram.sv", HERE / "rtl/metadata_cache.sv", HERE / "rtl/media_metadata.sv", HERE / "rtl/media_reclaimer.sv", HERE / "rtl/media_prefetch_resolver.sv", HERE / "rtl/media_store.sv"]
        parameters = dict(zip(("VIEWS", "SLOTS", "CHUNK_FRAMES"), capacities))
        parameters.update(RETIRED_TABLES=args.retired_tables,RECLAIM_BLOCKS=args.reclaim_blocks,RECLAIM_CYCLES=args.reclaim_cycles,RECLAIM_GUARD=args.reclaim_guard,BLOCK_CHUNKS=args.block_chunks,TABLE_LINES=args.metadata_lines,REF_LINES=args.reference_lines,STACK_LINES=args.stack_lines)
        if args.metadata_pages is not None:parameters['TABLE_PAGES']=args.metadata_pages
        parameters.update(READ_BUFFERS=args.read_buffers, WRITE_BUFFERS=args.write_buffers)
        audio = (32, 16, 32, 32, 16, 2, 2) if name == "default" else (16, 8, 16, capacities[2], 8, 2, 2)
        if name == "wide":
            audio = (32, 16, 32, 32, 16, 2, 2)
        if name == "8x8":
            audio = (256, 512, 256, 32, 256, 8, 8)
        if args.pipeline:
            sources += [ROOT / "fpga/state_processor/rtl" / (module+".sv")
                        for module in ("sp_ram", "sp_job_fifo", "sp_engine", "state_processor")]
            sources += [ROOT / "fpga/job_processor/rtl" / (module+".sv")
                        for module in ("jp_sample_ram", "jp_divider", "job_processor", "audio_pipeline")]
            sources += [HERE / "rtl/memory_delay.sv", HERE / "rtl/media_audio_pipeline.sv"]
            parameters.update(zip(("PROGRAM_DEPTH", "STATE_DEPTH", "JOB_DEPTH", "MAX_FRAMES", "BUFFERS", "INPUT_PORTS", "OUTPUT_PORTS"), audio))
            parameters["MEMORY_RESPONSE_CYCLES"] = args.response_cycles
        if axi:
            sources += [ROOT/"fpga/axi_memory/rtl"/name for name in ("axi_master.sv", "axi_burst_mux.sv", "shared_axi.sv", "chunk_dma.sv", "word_dma.sv", "word_axi.sv", "chunk_axi.sv", top+"_axi.sv")]
            top += "_axi"
            parameters.update(AXI_DATA_WIDTH=args.axi_data_width,PHYSICAL_BASE=args.physical_base,BURST_MEMORY=int(not args.word_transfers))
        runner.build(verilog_sources=direct_sources(sources,top,args.memory_model), includes=[ROOT / "fpga/state_processor/rtl"], hdl_toplevel=top,
                     parameters=parameters,
                     build_args=["--Wall", "--timescale", "1ns/1ps"],
                     build_dir=output / "build", always=True, waves=args.waves, log_file=output / "build.log")
        block_test=args.block_chunks>1
        testcase=None if block_test else ['directed_pipeline','randomized_pipeline','pipeline_protocol','playback_stream_protocol'] if args.pipeline else None
        module=('test_block_audio' if args.pipeline else 'test_block_media') if block_test else ('test_media_pipeline' if args.pipeline else 'test_media_store')
        if args.deferred:
            if args.pipeline:parser.error('--deferred uses the standalone store')
            module='test_deferred_media';testcase=None
        result = runner.test(hdl_toplevel=top, test_module=module,
                             testcase=testcase, seed=args.seed,
                             test_dir=output, waves=args.waves, log_file=output / "simulation.log",
                             extra_env={"MEDIA_CAPACITIES": ",".join(map(str, capacities)),
                                        "MEDIA_CONFIG": name,
                                        "MEDIA_READ_BUFFERS": str(args.read_buffers), "MEDIA_WRITE_BUFFERS": str(args.write_buffers),
                                        "AUDIO_CAPACITIES": ",".join(map(str, audio)),
                                        "MEDIA_SEED": str(args.seed), "MEDIA_RANDOM_CASES": str(args.random_cases),
                                        "MEDIA_RESPONSE_CYCLES": str(args.response_cycles),
                                        "MEDIA_PREFETCH_ANALYSIS": str(int(args.prefetch_analysis)),
                                        "MEDIA_OUTPUT": str(output), "MEDIA_CASE": str(args.case.resolve()) if args.case else "", **memory_env})
        if not result.is_file():
            raise SystemExit(f"No results; see {output / 'simulation.log'}")
        report = ET.parse(result).getroot()
        if (len(list(report.iter("testcase"))) != (3 if args.deferred else 1 if block_test else 4 if args.pipeline else 3)
                or any(list(report.iter(tag)) for tag in ("failure", "error", "skipped"))):
            raise SystemExit(f"Media simulation failed; see {output / 'simulation.log'}")
        print(f"{name}: media store passed; results in {output}", flush=True)


if __name__ == "__main__":
    main()
