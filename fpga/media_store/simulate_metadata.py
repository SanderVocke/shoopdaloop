#!/usr/bin/env python3
"""Exercise metadata caches and the shared physical DDR transport."""
import argparse
import os
from pathlib import Path
import sys
import xml.etree.ElementTree as ET
from cocotb.runner import get_runner
HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--output',type=Path,default=ROOT/'work/metadata-units')
    args=p.parse_args()
    sys.path[:0]=[str(HERE/"tests"),str(ROOT/"fpga/axi_memory/sim")]
    if sys.prefix != sys.base_prefix:os.environ.setdefault("VIRTUAL_ENV",sys.prefix)
    for width in (32,64):
        for words in (16,64):
            out=args.output.resolve()/f'{width}-{words}';out.mkdir(parents=True,exist_ok=True)
            runner=get_runner('verilator')
            sources=[ROOT/'fpga/axi_memory/rtl'/f'{name}.sv' for name in ('axi_master','chunk_dma','word_dma','axi_burst_mux','shared_axi')]
            sources += [ROOT/'fpga/chunk_memory/rtl/chunk_ram.sv',HERE/'rtl/metadata_cache.sv',
                        ROOT/'fpga/axi_memory/tests/axi_direct_memory.sv',HERE/'tests/metadata_cache_tb.sv']
            runner.build(verilog_sources=sources,hdl_toplevel='metadata_cache_tb',parameters=dict(DATA_WIDTH=width,LINE_WORDS=words),
                         build_args=['--Wall','--timescale','1ns/1ps'],build_dir=out/'build',log_file=out/'build.log')
            result=runner.test(hdl_toplevel='metadata_cache_tb',test_module='test_metadata_cache',test_dir=out,
                log_file=out/'simulation.log',extra_env={'PYTHONPATH':':'.join(map(str,(HERE/'tests',ROOT/'fpga/axi_memory/sim'))),
                    'METADATA_LINE_WORDS':str(words)})
            tree=ET.parse(result)
            if any(list(tree.iter(tag)) for tag in ('failure','error','skipped')):raise SystemExit(f'failed {out}')
            print(f'metadata cache / AXI{width} / {words} words: passed')


if __name__=='__main__':main()
