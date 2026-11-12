#!/usr/bin/env python3
"""Compare continuous 16-channel fabric audio with production Rust processing."""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import xml.etree.ElementTree as ET
from cocotb.runner import get_runner
from sources import ROOT, sources
from prepare_demo import prepare
import generate_top

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT/'fpga/axi_memory/sim'))
from options import add_options, environment


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--q', type=int, default=32)
    parser.add_argument('--chunk', type=int)
    parser.add_argument('--p', type=int)
    parser.add_argument('--banks', type=int, default=3)
    parser.add_argument('--channels', type=int, default=16)
    parser.add_argument('--monitoring', action='store_true')
    parser.add_argument('--demo', action='store_true')
    parser.add_argument('--audio-period', type=int, default=13)
    parser.add_argument('--frame-cycles', type=int, default=512)
    parser.add_argument('--stalls', action='store_true')
    parser.add_argument('--overrun', action='store_true')
    parser.add_argument('--output', type=Path, default=ROOT/'work/audio-pipeline')
    add_options(parser)
    args = parser.parse_args()
    env = environment(args)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    subprocess.run(['cargo', 'build', '--locked', '-p', 'rt_backend_testbench'], cwd=ROOT, check=True)
    config = prepare(output, args.q, args.chunk, args.channels, args.monitoring)
    top = 'audio_demo' if args.demo else 'audio_loop_pipeline'
    if args.demo:
        parameters = dict(Q=args.q, P=args.p or args.q, CHUNK_FRAMES=args.chunk or args.q,
                          CHANNELS=args.channels, RX_BANKS=args.banks, TX_BANKS=args.banks,
                          AXI_DATA_WIDTH=args.axi_data_width, BOOT_WORDS=config['boot_words'],
                          EVENT_WORDS=config['event_words'], BOOT_FILE='"'+str(output/'boot.hex')+'"',
                          EVENT_FILE='"'+str(output/'events.hex')+'"', INSTRUCTIONS=config['instructions'],
                          STATES=config['states'], BUFFERS=config['buffers'], JOB_BUDGET=config['jobs'])
    else:
        parameters = dict(MAX_FRAMES=args.q, CHUNK_FRAMES=args.chunk or args.q,
                          P=args.p or args.q, INPUT_PORTS=args.channels, OUTPUT_PORTS=args.channels,
                          RX_BANKS=args.banks, TX_BANKS=args.banks, REQUEST_DEPTH=128, EVENT_DEPTH=256,
                          AXI_DATA_WIDTH=args.axi_data_width, PHYSICAL_BASE=args.physical_base)
    manifest = sources(args.demo)
    if args.memory_model == 'instant':
        path = output/f'{top}.sv'
        path.write_text(generate_top.axi.direct_top((HERE/'rtl'/f'{top}.sv').read_text()))
        manifest = [path if p.stem == top else p for p in manifest]
        manifest.append(ROOT/'fpga/axi_memory/tests/axi_direct_memory.sv')
    runner = get_runner('verilator')
    runner.build(verilog_sources=manifest, hdl_toplevel=top, parameters=parameters,
                 includes=[ROOT/'fpga/state_processor/rtl'], build_args=['--Wall', '--timescale', '1ns/1ps'],
                 build_dir=output/'build', log_file=output/'build.log', always=True)
    sys.path[:0] = [str(path) for path in [HERE/'tests', ROOT/'scripts', ROOT/'fpga/loop_pipeline/tests',
                 ROOT/'fpga/transition_processor/tests', ROOT/'fpga/media_store/tests', ROOT/'fpga/verification/tests']]
    result = runner.test(hdl_toplevel=top, test_module='test_demo' if args.demo else 'test_continuous',
                         test_dir=output, log_file=output/'simulation.log', extra_env={**env,
                             'AIO_OUTPUT': str(output), 'AIO_AUDIO_PERIOD': str(args.audio_period),
                             'AIO_FRAME_CYCLES': str(args.frame_cycles), 'AIO_P': str(args.p or args.q),
                             'AIO_STALLS': str(int(args.stalls)), 'AIO_OVERRUN': str(int(args.overrun)),
                             'LP_OUTPUT': str(output)})
    report = ET.parse(result).getroot()
    if not list(report.iter('testcase')) or any(list(report.iter(k)) for k in ('failure', 'error', 'skipped')):
        raise SystemExit(f'continuous audio failed: {output}')
    print(f'{top} continuous production comparison passed: {output}')


if __name__ == '__main__':
    main()
