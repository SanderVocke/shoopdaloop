#!/usr/bin/env python3
"""Build the pinned headless Linux Carla library for required native tests."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import subprocess
import sys
import tarfile

from carla_runtime import LOCK_DATA, digest


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--archive", type=Path)
    parser.add_argument("--environment-file", type=Path)
    parser.add_argument("--jobs", type=int, default=2)
    args = parser.parse_args()
    if sys.platform != "linux" or not 1 <= args.jobs <= 32:
        parser.error("requires Linux and 1–32 build jobs")
    directory = args.directory.resolve()
    directory.mkdir(parents=True, exist_ok=True)
    source = directory / f"Carla-{LOCK_DATA['version']}"
    if source.exists():
        parser.error("use a fresh build directory")
    archive = args.archive.resolve() if args.archive else directory / "source.tar.gz"
    if not archive.exists():
        subprocess.run(["curl", "--fail", "--location", "--retry", "3", "--output",
                        str(archive), LOCK_DATA["source"]["url"]], check=True)
    if digest(archive) != LOCK_DATA["source"]["sha256"]:
        raise RuntimeError("Carla source checksum does not match runtime-lock.json")
    with tarfile.open(archive) as bundle:
        bundle.extractall(directory, filter="data")
    header = source / "source/includes/CarlaNative.h"
    if digest(header) != LOCK_DATA["source"]["native_header_sha256"]:
        raise RuntimeError("Carla native header does not match runtime-lock.json")
    subprocess.run(["make", f"-j{args.jobs}", "plugin", "HAVE_QT4=false", "HAVE_QT5=false",
                    "HAVE_DGL=false", "HAVE_X11=false", "HAVE_HYLIA=false"], cwd=source, check=True)
    library = source / "bin/libcarla_native-plugin.so"
    resources = source / "source/frontend"
    if not library.is_file() or not resources.is_dir():
        raise RuntimeError("native Carla test runtime was not built")
    environment = {"SHOOP_CARLA_NATIVE_LIBRARY": str(library),
                   "SHOOP_CARLA_RESOURCE_DIR": str(resources)}
    if args.environment_file:
        with args.environment_file.open("a") as handle:
            for key, value in environment.items():
                handle.write(f"{key}={value}\n")
    print(json.dumps({"version": LOCK_DATA["version"], "environment": environment,
                      "source_sha256": digest(archive), "header_sha256": digest(header)}))


if __name__ == "__main__":
    main()
