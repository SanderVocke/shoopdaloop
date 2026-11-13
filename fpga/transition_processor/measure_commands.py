#!/usr/bin/env python3
"""Measure transition control costs through the shared native Marlin driver."""
import argparse
from pathlib import Path
import subprocess

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, default=HERE)
    parser.add_argument('--requests', type=int, choices=(1, 11, 64, 272), default=272)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    if output.exists() and any(output.iterdir()):
        parser.error('output must be empty to preserve provenance')
    source = args.source.resolve()
    rtl = source/'rtl' if (source/'rtl').is_dir() else source
    text = (rtl/'transition_processor.sv').read_text()
    command = ['cargo', 'run', '--locked', '-p', 'rt_fpga_testbench', '--features', 'sim', '--',
               'command-costs', '--requests', str(args.requests), '--transition-source', str(rtl), '--output', str(output)]
    if 'S_SUBMIT_SCAN' in text:
        command.append('--legacy-allocation')
    subprocess.run(command, cwd=ROOT, check=True)


if __name__ == '__main__':
    main()
