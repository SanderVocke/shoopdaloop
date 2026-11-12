#!/usr/bin/env python3
"""Verify the lockstep bank transport without a physical audio adapter."""
import argparse
import sys
from pathlib import Path
import xml.etree.ElementTree as ET
from cocotb.runner import get_runner

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--q', type=int, default=3)
    parser.add_argument('--p', type=int)
    parser.add_argument('--banks', type=int, default=3)
    parser.add_argument('--channels', type=int, default=2)
    parser.add_argument('--audio-period', type=int, default=13)
    parser.add_argument('--output', type=Path, default=ROOT/'work/audio-io')
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    p = args.p if args.p is not None else args.q
    parameters = dict(Q=args.q, P=p, INPUTS=args.channels, OUTPUTS=args.channels,
                      RX_BANKS=args.banks, TX_BANKS=args.banks, WATCHDOG_CYCLES=100000)
    runner = get_runner('verilator')
    runner.build(verilog_sources=[HERE/'rtl/audio_lane_ram.sv', HERE/'rtl/audio_io.sv'],
                 hdl_toplevel='audio_io', parameters=parameters,
                 build_args=['--Wall', '--timescale', '1ns/1ps'],
                 build_dir=output/'build', log_file=output/'build.log', always=True)
    sys.path.insert(0, str(HERE/'tests'))
    result = runner.test(hdl_toplevel='audio_io', test_module='test_audio_io',
                         test_dir=output, log_file=output/'simulation.log', extra_env={
                             'AIO_Q': str(args.q), 'AIO_P': str(p), 'AIO_BANKS': str(args.banks),
                             'AIO_CHANNELS': str(args.channels), 'AIO_AUDIO_PERIOD': str(args.audio_period)})
    report = ET.parse(result).getroot()
    if any(list(report.iter(k)) for k in ('failure', 'error', 'skipped')):
        raise SystemExit(f'audio I/O failed: {output}')
    print(f'Q={args.q}, P={p}, banks={args.banks}: audio I/O passed')


if __name__ == '__main__':
    main()
