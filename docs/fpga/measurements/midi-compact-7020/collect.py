#!/usr/bin/env python3
"""Retain text evidence from completed compact-MIDI implementation runs."""

import argparse
import json
import re
import shutil
from pathlib import Path


def resources(path):
    text = path.read_text()
    labels = {
        "lut": "Slice LUTs",
        "ff": "Slice Registers",
        "bram36": "Block RAM Tile",
        "dsp": "DSPs",
    }
    result = {}
    for key, label in labels.items():
        match = re.search(
            r"^\| " + re.escape(label) + r"\*?\s+\|\s*([\d.]+)", text, re.M
        )
        if not match:
            raise ValueError(f"Missing {label} in {path}")
        result[key] = float(match[1])
        if result[key].is_integer():
            result[key] = int(result[key])
    slices = re.search(r"^\| Slice\s+\|\s*(\d+)", text, re.M)
    if slices:
        result["slices"] = int(slices[1])
    return result


def collect(source, destination):
    outcome = (source / "outcome.txt").read_text()
    if "stage=route\nstatus=0\n" not in outcome:
        raise ValueError(f"Implementation did not finish: {source}: {outcome}")
    destination.mkdir(parents=True, exist_ok=True)
    for path in source.iterdir():
        if path.is_file() and path.suffix in {".json", ".rpt", ".xdc", ".sv", ".tcl"}:
            shutil.copy2(path, destination / path.name)
    for name in ("console.log", "outcome.txt", "setup.txt"):
        shutil.copy2(source / name, destination / name)
    provenance = json.loads((source / "provenance.json").read_text())
    result = {
        "mhz": provenance["clock_mhz"],
        "revision": provenance["revision"],
        "evidence": destination.name,
        "outcome": outcome.strip(),
    }
    for stage in ("post_synth", "post_opt", "post_place", "post_route"):
        result[stage] = resources(source / f"{stage}_utilization.rpt")
        result[stage]["timing"] = json.loads((source / f"{stage}.json").read_text())
        controls = (source / f"{stage}_control_sets.rpt").read_text()
        match = re.search(r"\| Total control sets\s+\|\s*(\d+)", controls)
        if match:
            result[stage]["control_sets"] = int(match[1])
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "workspace", type=Path, help="Parent of compact-pipelined-100/150"
    )
    args = parser.parse_args()
    root = Path(__file__).resolve().parent
    results = {
        "part": "xc7z020clg484-1",
        "runs": [
            collect(
                args.workspace / f"compact-pipelined-{mhz}", root / f"compact-{mhz}"
            )
            for mhz in (100, 150)
        ],
    }
    (root / "summary.json").write_text(json.dumps(results, indent=2) + "\n")


if __name__ == "__main__":
    main()
