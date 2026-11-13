#!/usr/bin/env python3
"""Shared compatibility entry point; Rust owns all simulation and assertions."""
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def launch(suite):
    args = list(sys.argv[1:])
    selectors = {
        'media': {'--pipeline': 'pipeline', '--deferred': 'deferred',
                  '--deferred-capacity': 'capacity', '--deferred-reuse': 'reuse',
                  '--deferred-stress': 'stress', '--statistics': 'statistics',
                  '--reclaim-timing': 'reclaim-timing'},
        'audio-io': {'--control': 'control'}, 'continuous': {'--demo': 'demo'},
    }
    selected = [(flag, value) for flag, value in selectors.get(suite, {}).items() if flag in args]
    if len(selected) > 1:
        raise SystemExit('select one native suite at a time')
    if selected:
        flag, suite = selected[0]
        args.remove(flag)
    for option in ('--output', '--case', '--workload'):
        for index, value in enumerate(args[:-1]):
            if value == option and args[index+1] != '-':
                args[index+1] = str(Path(args[index+1]).resolve())
    command = ['cargo', 'run', '--locked', '-p', 'rt_fpga_testbench', '--features', 'sim', '--', suite, *args]
    return subprocess.call(command, cwd=ROOT)


if __name__ == '__main__':
    if len(sys.argv) < 2:
        raise SystemExit('usage: run_fpga_testbench.py SUITE [native runner options]')
    component = sys.argv.pop(1)
    raise SystemExit(launch(component))
