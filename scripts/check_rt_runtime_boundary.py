#!/usr/bin/env python3
"""Audit production/build dependency trees independently of dev-only test support."""

from __future__ import annotations

import argparse
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
TARGETS = (
    "x86_64-unknown-linux-gnu",
    "x86_64-pc-windows-msvc",
    "aarch64-apple-darwin",
    "wasm32-unknown-unknown",
)
FORBIDDEN = {
    "alsa", "alsa-sys", "cpal", "jack", "jack-sys", "midir", "rodio",
    "eframe", "egui", "winit", "rfd", "gtk", "gtk-sys",
    "wasm-bindgen", "js-sys", "web-sys",
}


def violations(tree: str) -> list[str]:
    names = set(re.findall(r"^([A-Za-z0-9_-]+) v\d", tree, re.MULTILINE))
    return sorted(name for name in names if name.startswith("shoop_") or name in FORBIDDEN)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", action="append", help="Override the default four-target audit")
    args = parser.parse_args()
    failed = False
    for target in args.target or TARGETS:
        result = subprocess.run(
            ["cargo", "tree", "--locked", "-p", "rt_runtime", "--all-features",
             "--target", target, "-e", "normal,build", "--prefix", "none", "--format", "{p}"],
            cwd=ROOT, text=True, capture_output=True, check=False,
        )
        if result.returncode:
            sys.stderr.write(result.stdout + result.stderr)
            return result.returncode
        if not re.search(r"^rt_runtime v\d", result.stdout, re.MULTILINE):
            print(f"{target}: missing runtime root in dependency tree", file=sys.stderr)
            return 1
        forbidden = violations(result.stdout)
        if forbidden:
            print(f"{target}: forbidden runtime dependencies: {', '.join(forbidden)}", file=sys.stderr)
            failed = True
        else:
            print(f"{target}: rt_runtime production/build boundary: ok")
    return int(failed)


if __name__ == "__main__":
    raise SystemExit(main())
