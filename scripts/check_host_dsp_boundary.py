#!/usr/bin/env python3
"""Audit optional DSP features without workspace feature unification."""
from __future__ import annotations

import argparse
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]
TARGETS = ("x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc", "aarch64-apple-darwin", "wasm32-unknown-unknown")
FORBIDDEN = {"shoop_engine", "shoop_app_reference", "shoop_app", "shoop_app_api", "shoop_backend", "shoop_egui", "shoopdaloop", "shoop_session", "shoop_scripting", "cpal", "jack", "jack-sys", "midir", "iceoryx2"}


def violations(tree: str, features: tuple[str, ...]) -> list[str]:
    names = set(re.findall(r"^([A-Za-z0-9_-]+) v\d", tree, re.MULTILINE))
    forbidden = set(FORBIDDEN)
    if "builtin-fx" not in features:
        forbidden.add("fundsp")
    if "oxisynth" not in features:
        forbidden.update({"oxisynth", "soundfont", "oxisynth-chorus", "oxisynth-reverb"})
    return sorted(names & forbidden)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", action="append")
    args = parser.parse_args()
    for target in args.target or TARGETS:
        for features in [(), ("builtin-fx",), ("oxisynth",), ("builtin-fx", "oxisynth")]:
            command = ["cargo", "tree", "--locked", "-p", "shoop_dsp", "--no-default-features", "--target", target,
                       "-e", "normal,build", "--prefix", "none", "--format", "{p}"]
            if features:
                command += ["--features", ",".join(features)]
            result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True)
            if result.returncode:
                print(result.stdout + result.stderr)
                return result.returncode
            invalid = violations(result.stdout, features)
            if invalid:
                print(f"{target} {features}: forbidden DSP dependencies: {', '.join(invalid)}")
                return 1
            print(f"{target}: DSP features {','.join(features) or 'neither'}: isolated")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
