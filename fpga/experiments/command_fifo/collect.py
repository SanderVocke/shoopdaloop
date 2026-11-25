#!/usr/bin/env python3
"""Export checksummed, non-board investigation evidence from a completed run."""
import argparse
import hashlib
import json
import pathlib
import re
import shutil

ROOT = pathlib.Path(__file__).resolve().parents[3]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def area(path):
    for line in path.read_text().splitlines():
        columns = [column.strip() for column in line.split("|")]
        if len(columns) == 12 and columns[2] == "(top)":
            return dict(zip(["lut", "ff", "ramb36", "ramb18", "dsp"],
                            [int(columns[index]) for index in [3, 7, 8, 9, 10]]))
    raise ValueError(f"no top utilization row: {path}")


def timing(path):
    lines = path.read_text().splitlines()
    for index, line in enumerate(lines):
        if "WNS(ns)" in line and "TNS(ns)" in line:
            numbers = lines[index + 2].split()
            return {"wns_ns": float(numbers[0]), "whs_ns": float(numbers[4]),
                    "setup_failing_endpoints": int(numbers[2]),
                    "hold_failing_endpoints": int(numbers[6])}
    raise ValueError(f"no timing summary: {path}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run", type=pathlib.Path)
    parser.add_argument("output", type=pathlib.Path)
    args = parser.parse_args()
    run = args.run.resolve()
    output = args.output.resolve()
    assert (run / "status.txt").read_text().strip() == "0"
    assert (run / "build/complete.txt").read_text().strip() == "passed"
    provenance = json.loads((run / "provenance.json").read_text())
    for source in provenance["sources"]:
        assert sha(ROOT / source["path"]) == source["sha256"], source
    assert sha(run / "run.tcl") == provenance["tcl_sha256"]
    evidence = output / "reports"
    evidence.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(run / "provenance.json", evidence / "provenance.json")
    shutil.copyfile(run / "run.tcl", evidence / "run.tcl")
    for path in sorted((run / "build").glob("*")):
        if path.suffix == ".rpt" or path.name.endswith("-primitives.txt"):
            shutil.copyfile(path, evidence / path.name)
    results = {"scope": provenance["scope"], "part": provenance["part"],
               "clock_mhz": provenance["clock_mhz"], "board_accessed": False,
               "production_path_changed": False, "decision": "pending", "cases": {}}
    console = (run / "console.log").read_text()
    assert "SW Build 3247384" in console
    assert "Vivado v2021.1" in console
    results["vivado"] = "2021.1 SW Build 3247384"
    results["launcher_sha256"] = re.search(r"^([a-f0-9]{64})  /tools/.*/bin/vivado$", console, re.M).group(1)
    results["sources"] = provenance["sources"]
    results["oracle_and_test_sources"] = [
        {"path": name, "sha256": sha(ROOT / name)} for name in [
            "src/rust/rt_fpga_rtl/tests/command_kernel.rs",
            "src/rust/rt_fpga_rtl/src/host/control.rs",
            "src/rust/rt_fpga_reference/src/state/sync.rs",
            "Cargo.lock",
        ]
    ]
    results["differential"] = json.loads((ROOT / "work/cmd-fifo-investigation/kernel-cycles.json").read_text())
    for name, case in provenance["cases"].items():
        item = {**case, "synthesis": area(evidence / f"{name}-utilization.rpt")}
        if case["route"]:
            item["routed"] = area(evidence / f"{name}-routed-utilization.rpt")
            item["routed_timing"] = timing(evidence / f"{name}-routed-timing.rpt")
        results["cases"][name] = item
    (output / "results.json").write_text(json.dumps(results, indent=2) + "\n")
    files = sorted(path for path in evidence.iterdir() if path.is_file()) + [output / "results.json"]
    (output / "SHA256SUMS").write_text("".join(f"{sha(path)}  {path.relative_to(output)}\n" for path in files))
    print(json.dumps(results["cases"], indent=2))


if __name__ == "__main__":
    main()
