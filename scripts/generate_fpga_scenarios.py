#!/usr/bin/env python3
"""Export common production inputs for native FPGA replay; computes no RTL results."""
import argparse
import json
from pathlib import Path
import sys

from rt_backend_testbench.cases import (
    allocation_boundary, directed, geometry_cases, long_sessions, scale_workload, seeded,
)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--directed', action='store_true')
    parser.add_argument('--random-cases', type=int, default=0)
    parser.add_argument('--seed', type=int, default=20261112)
    parser.add_argument('--long-sessions', action='store_true')
    parser.add_argument('--geometry-cases', action='store_true')
    parser.add_argument('--benchmark64', action='store_true')
    parser.add_argument('--scale-workload', choices=('original', 'streaming', 'mixed'), default='original')
    parser.add_argument('--allocation-boundary', type=int)
    parser.add_argument('--chunk', type=int, default=32)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    if args.random_cases < 0:
        parser.error('random-cases must be nonnegative')
    values = list(directed()) if args.directed else []
    values.extend(seeded(args.random_cases, args.seed))
    if args.long_sessions:
        values.extend(long_sessions())
    if args.geometry_cases:
        values.extend(geometry_cases())
    if args.benchmark64:
        values.append(scale_workload(args.scale_workload))
    if args.allocation_boundary:
        values.append(allocation_boundary(args.allocation_boundary, args.chunk))
    text = ''.join(json.dumps(value, separators=(',', ':'))+'\n' for value in values)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(text)
    else:
        sys.stdout.write(text)


if __name__ == '__main__':
    main()
