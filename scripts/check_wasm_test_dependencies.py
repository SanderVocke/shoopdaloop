#!/usr/bin/env python3

import argparse
import json
import re
import subprocess
import sys

FORBIDDEN = {
    "alsa",
    "alsa-sys",
    "cpal",
    "jack",
    "jack-sys",
    "libloading",
    "midir",
    "rodio",
    "tracy-client",
    "tracy-client-sys",
    "tracy-nextest-capture",
    "tracy-nextest-capture-macros",
    "tracing-tracy",
}


def package_names(tree: str) -> set[str]:
    return {
        match.group(1)
        for line in tree.splitlines()
        if (match := re.search(r"(?:^|[\s─├└])([A-Za-z0-9_-]+) v\d", line))
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--package",
        action="append",
        required=True,
        help="Wasm test package to inspect; repeat for multiple packages",
    )
    parser.add_argument("--target", default="wasm32-unknown-unknown")
    args = parser.parse_args()

    metadata_result = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        check=False,
        text=True,
        capture_output=True,
    )
    if metadata_result.returncode:
        sys.stdout.write(metadata_result.stdout)
        sys.stderr.write(metadata_result.stderr)
        return metadata_result.returncode
    metadata = {
        package["name"]: (package.get("metadata") or {}).get("shoop-wasm-test", {})
        for package in json.loads(metadata_result.stdout)["packages"]
    }

    failed = False
    for package in args.package:
        command = [
            "cargo",
            "tree",
            "--locked",
            "-p",
            package,
            "--target",
            args.target,
            "-e",
            "all",
        ]
        if metadata.get(package, {}).get("no-default-features", False):
            command.append("--no-default-features")
        result = subprocess.run(command, check=False, text=True, capture_output=True)
        sys.stdout.write(result.stdout)
        sys.stderr.write(result.stderr)
        if result.returncode:
            return result.returncode
        found = sorted(package_names(result.stdout) & FORBIDDEN)
        if found:
            print(
                f"{package} has forbidden Wasm test dependencies: " + ", ".join(found),
                file=sys.stderr,
            )
            failed = True
        else:
            print(f"{package} Wasm test dependency isolation: ok")
    return int(failed)


if __name__ == "__main__":
    raise SystemExit(main())
