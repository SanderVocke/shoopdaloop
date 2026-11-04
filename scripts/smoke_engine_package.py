#!/usr/bin/env python3
"""Launch both engine carriers from an extracted native application archive."""

import argparse
import os
from pathlib import Path
import subprocess
import struct
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
        manager = app.with_name("shoop_host_manager.exe" if platform == "windows" else "shoop_host_manager")
        subprocess.run([str(manager), "--help"], cwd=root, env=environment, check=True, timeout=30)
        if platform == "macos":
            entry = root / "shoopdaloop/ShoopDaLoop Host Manager.app/Contents/MacOS/shoop_host_manager"
            subprocess.run([str(entry), "--help"], cwd=root, env=environment, check=True, timeout=30)
        elif platform == "windows":
            shortcut = root / "shoopdaloop/ShoopDaLoop Host Manager.lnk"
            link = shortcut.read_bytes()
            header_size, = struct.unpack_from("<I", link)
            flags, = struct.unpack_from("<I", link, 20)
            count, = struct.unpack_from("<H", link, 76)
            target = link[78:78 + count * 2].decode("utf-16-le")
            if header_size != 76 or flags != 0x88 or target != manager.name:
                raise RuntimeError("Manager shortcut has an invalid relative target")
            subprocess.run([str(shortcut.parent / target), "--help"], cwd=root, env=environment, check=True, timeout=30)

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
