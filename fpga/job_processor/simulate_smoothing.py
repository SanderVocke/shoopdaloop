#!/usr/bin/env python3
"""Run the native smoothing regression."""
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'scripts'))
from run_fpga_testbench import launch

if __name__ == '__main__':
    raise SystemExit(launch('smoothing'))
