#!/usr/bin/env python3
"""Audit the new engine, protocol and hosts for application and legacy dependencies."""

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

PACKAGES = (
    "shoop_rt_engine", "shoop_rt_protocol", "shoop_rt_host_native", "shoop_rt_host_worklet",
)
FORBIDDEN = {
    "shoop_app", "shoop_app_api", "shoop_backend", "shoop_engine", "shoop_egui",
    "shoopdaloop", "shoop_session", "shoop_worklet_client", "shoop_audio_worklet",
    "shoop_audio_protocol",
}


def violations(tree: str) -> list[str]:
    names = set(re.findall(r"^([A-Za-z0-9_-]+) v\d", tree, re.MULTILINE))
    return sorted(name for name in names if name in FORBIDDEN)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", action="append")
    args = parser.parse_args()
    failed = False
    for package in PACKAGES:
        for target in args.target or RUNTIME.TARGETS:
            result = subprocess.run(
                ["cargo", "tree", "--locked", "-p", package, "--all-features",
                 "--target", target, "-e", "normal,build", "--prefix", "none", "--format", "{p}"],
                cwd=ROOT, text=True, capture_output=True, check=False,
            )
            if result.returncode:
                sys.stderr.write(result.stdout + result.stderr)
                return result.returncode
            if not re.search(rf"^{package} v\d", result.stdout, re.MULTILINE):
                print(f"{target}: missing {package} root", file=sys.stderr)
                return 1
            forbidden = violations(result.stdout)
            if forbidden:
                print(f"{target}: forbidden {package} dependencies: {', '.join(forbidden)}",
                      file=sys.stderr)
                failed = True
            else:
                print(f"{target}: {package} production/build boundary: ok")
    return int(failed)


if __name__ == "__main__":
    raise SystemExit(main())
