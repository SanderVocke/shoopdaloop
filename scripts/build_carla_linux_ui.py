#!/usr/bin/env python3
"""Freeze Carla's external Rack/Patchbay UI for a relocatable Linux component."""

from __future__ import annotations

import argparse
import os
import shutil
import sys
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    source = args.source.resolve()
    output = args.output.resolve()
    resources = output / "resources"
    if output.exists():
        shutil.rmtree(output)
    resources.mkdir(parents=True)

    sys.path.insert(0, str(source / "source" / "frontend"))
    os.chdir(source)
    from cx_Freeze import Executable, setup

    setup(
        name="ShoopDaLoop-Carla-UI",
        version="2.5.10",
        options={
            "build_exe": {
                "build_exe": str(resources),
                "zip_include_packages": ["*"],
                "zip_exclude_packages": ["PyQt5"],
                "optimize": 1,
            }
        },
        executables=[
            Executable(
                str(source / "bin" / "resources" / "carla-plugin"),
                target_name="carla-plugin",
            )
        ],
        script_args=["build_exe"],
    )
    plugin = resources / "carla-plugin"
    if not plugin.is_file():
        raise RuntimeError("cx_Freeze did not produce carla-plugin")
    shutil.copy2(plugin, resources / "carla-plugin-patchbay")
    (resources / "carla-plugin-patchbay").chmod(plugin.stat().st_mode)


if __name__ == "__main__":
    main()
