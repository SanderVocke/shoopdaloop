#!/usr/bin/env python3

import argparse
import re
import subprocess
import sys

FORBIDDEN = {
    "carla",
    "cpal",
    "eframe",
    "egui",
    "jack",
    "js-sys",
    "libloading",
    "midir",
    "shoop_egui",
    "tracy-client",
    "tracy-client-sys",
    "wasm-bindgen",
    "web-sys",
}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--target", default="wasm32-unknown-unknown")
    args = parser.parse_args()
    command = [
        "cargo",
        "tree",
        "-p",
        "shoop_worklet_client",
        "--target",
        args.target,
        "-e",
        "normal",
    ]
    result = subprocess.run(command, check=False, text=True, capture_output=True)
    sys.stdout.write(result.stdout)
    sys.stderr.write(result.stderr)
    if result.returncode:
        return result.returncode

    packages = {
        match.group(1)
        for line in result.stdout.splitlines()
        if (match := re.search(r"(?:^|[\s─├└])([A-Za-z0-9_-]+) v\d", line))
    }
    found = sorted(packages & FORBIDDEN)
    if found:
        print(
            "shoop_worklet_client has forbidden normal dependencies: " + ", ".join(found),
            file=sys.stderr,
        )
        return 1
    print("shoop_worklet_client dependency isolation: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
