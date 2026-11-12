#!/usr/bin/env python3
"""Run production-backed full-loop verification (same options as verification/simulate.py)."""
from pathlib import Path
import runpy

if __name__ == '__main__':
    runpy.run_path(str(Path(__file__).resolve().parents[1]/'verification/simulate.py'),run_name='__main__')
