#!/usr/bin/env python3
"""Launch both engine carriers from an extracted native application archive."""

import argparse
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import zipfile


def smoke(archive: Path, platform: str) -> None:
    with tempfile.TemporaryDirectory(prefix="Shoop Δ engine ") as directory:
        root = Path(directory)
        if archive.suffix == ".zip":
            with zipfile.ZipFile(archive) as bundle:
                bundle.extractall(root)
        else:
            with tarfile.open(archive) as bundle:
                bundle.extractall(root, filter="data")
        app = root / "shoopdaloop"
        if platform == "macos":
            app /= "ShoopDaLoop.app/Contents/MacOS/shoopdaloop"
        else:
            app /= "shoopdaloop.exe" if platform == "windows" else "shoopdaloop"
        environment = dict(os.environ)
        environment.pop("SHOOP_SRC_TREE", None)
        for transport in ("socket", "shared_memory"):
            subprocess.run(
                [str(app), "--probe-engine-transport", "--engine-transport", transport],
                cwd=root, env=environment, check=True, timeout=60,
            )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--platform", choices=("linux", "macos", "windows"), required=True)
    parser.add_argument("--artifacts", type=Path, required=True)
    args = parser.parse_args()
    archives = sorted(p for p in args.artifacts.iterdir() if p.suffix == ".zip" or p.name.endswith(".tar.gz"))
    if len(archives) != 1:
        raise RuntimeError(f"expected one native archive, found {archives}")
    smoke(archives[0], args.platform)


if __name__ == "__main__":
    main()
