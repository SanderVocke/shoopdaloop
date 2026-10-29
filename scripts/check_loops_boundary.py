#!/usr/bin/env python3
"""Audit the portable loop library without relaxing the generic runtime boundary."""

from __future__ import annotations

import argparse
import importlib.util
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "runtime_boundary_policy", ROOT / "scripts/check_rt_runtime_boundary.py"
)
RUNTIME = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUNTIME)


def violations(tree: str) -> list[str]:
    names = set(re.findall(r"^([A-Za-z0-9_-]+) v\d", tree, re.MULTILINE))
    return sorted(
        name for name in names
        if (name.startswith("shoop_") and name != "shoop_loops")
        or name in RUNTIME.FORBIDDEN
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", action="append")
    args = parser.parse_args()
    failed = False
    for target in args.target or RUNTIME.TARGETS:
        result = subprocess.run(
            ["cargo", "tree", "--locked", "-p", "shoop_loops", "--all-features",
             "--target", target, "-e", "normal,build", "--prefix", "none", "--format", "{p}"],
            cwd=ROOT, text=True, capture_output=True, check=False,
        )
        if result.returncode:
            sys.stderr.write(result.stdout + result.stderr)
            return result.returncode
        if not re.search(r"^shoop_loops v\d", result.stdout, re.MULTILINE):
            print(f"{target}: missing loop library root", file=sys.stderr)
            return 1
        forbidden = violations(result.stdout)
        if forbidden:
            print(f"{target}: forbidden loop dependencies: {', '.join(forbidden)}", file=sys.stderr)
            failed = True
        else:
            print(f"{target}: loop library production/build boundary: ok")
    return int(failed)


if __name__ == "__main__":
    raise SystemExit(main())
