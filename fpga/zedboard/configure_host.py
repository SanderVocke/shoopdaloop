#!/usr/bin/env python3
"""Configure the ZedBoard host flavor: the shared bridge profile, driven by the ARM over GP0."""
import argparse
import importlib.util
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(ROOT/'fpga/audio_io'))
from sources import sources

spec = importlib.util.spec_from_file_location('host_bridge', ROOT/'fpga/host_bridge/generate.py')
host_bridge = importlib.util.module_from_spec(spec)
spec.loader.exec_module(host_bridge)

DDR_BYTES = 512*1024*1024


def tcl(value):
    value = str(value)
    for char in ('\\', '"', '$', '[', ']'):
        value = value.replace(char, '\\'+char)
    return '"'+value+'"'


def configure(output, fabric_mhz=100, physical_base=host_bridge.BOARD_BASE):
    output = Path(output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    arena = host_bridge.arena_bytes()
    if physical_base % 4096 or physical_base < 1024*1024 or physical_base + arena > DDR_BYTES:
        raise ValueError('page-aligned host arena must fit inside ZedBoard DDR above the low megabyte')
    if fabric_mhz <= 0:
        raise ValueError('positive fabric clock required')
    top = output/'zedboard_host.v'
    top.write_text(host_bridge.board_top(physical_base), newline='\n')
    manifest = sources() + [ROOT/'fpga/host_bridge/rtl/host_core.sv', top]
    variables = dict(bench_flavor='host', bench_root=ROOT.as_posix(), bench_output=output.as_posix(),
                     bench_fabric_mhz=fabric_mhz)
    script = ''.join('set '+k+' '+tcl(v)+'\n' for k, v in variables.items())
    script += 'set bench_sources [list '+' '.join(tcl(p.as_posix()) for p in manifest)+']\n'
    (output/'manifest.tcl').write_text(script, newline='\n')
    return dict(top=top, arena_base=physical_base, arena_bytes=arena)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=ROOT/'work/zedboard/host')
    parser.add_argument('--fabric-mhz', type=float, default=100)
    parser.add_argument('--physical-base', type=lambda v: int(v, 0), default=host_bridge.BOARD_BASE)
    args = parser.parse_args()
    result = configure(args.output, args.fabric_mhz, args.physical_base)
    print(f"{result['top']}: arena {result['arena_bytes']} bytes at {result['arena_base']:#010x}")


if __name__ == '__main__':
    main()
