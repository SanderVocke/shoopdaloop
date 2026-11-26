#!/usr/bin/env python3
"""Freeze and measure the partial semantic frontend, without board integration."""
import argparse
import hashlib
import json
import pathlib
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[2]
IMAGE = "docker.io/siliconbootcamp/xilinx-vivado@sha256:d24586405894b07f6af006d5cb15ba01c7e8585de97b28b6121fcf6282db7a5f"
PART = "xc7z020clg400-2"
LAUNCHER = "2924389be0c4297f3e2c4d267e22904a89962575497d0f5fd7eb15dc959e5505"
SCOPE = "partial semantic bank/consumer/kernel frontend; no MMIO, snapshots, SP observations, loader arbitration, SP/JP, PS/DDR or board"
SCRIPT = r'''
set_param general.maxThreads 4
if {[version -short] ne "2021.1"} { error "Vivado 2021.1 required" }
create_project -in_memory -part xc7z020clg400-2
foreach source {fifo_edit_kernel.sv fifo_group_banks.sv fifo_group_consumer.sv} {
    read_verilog -sv /source/$source
}
synth_design -mode out_of_context -flatten_hierarchy none -top fifo_group_consumer -part xc7z020clg400-2 -generic {LOOPS=128 STATES=512 PROGRAM_DEPTH=1024 BUFFERS=512}
create_clock -name core -period 10 [get_ports clk]
set_input_delay -clock core 1 [get_ports -filter {DIRECTION == IN && NAME != clk}]
set_output_delay -clock core 1 [all_outputs]
report_utilization -hierarchical -file consumer-utilization.rpt
report_timing_summary -file consumer-synthesis-timing.rpt
set report [open consumer-primitives.txt w]
foreach {label filter} {LUT {REF_NAME =~ LUT*} FF {REF_NAME =~ FD*} CARRY4 {REF_NAME == CARRY4} RAMB36 {REF_NAME == RAMB36E1} RAMB18 {REF_NAME == RAMB18E1} DSP {REF_NAME == DSP48E1}} {
    puts $report "$label [llength [get_cells -hierarchical -filter $filter]]"
}
puts $report "PORT_BITS [llength [get_ports]]"
close $report
write_checkpoint -force consumer-synthesis.dcp
if {ROUTE_ENABLED} {
    opt_design
    place_design
    route_design
    report_utilization -hierarchical -file consumer-routed-utilization.rpt
    report_timing_summary -file consumer-routed-timing.rpt
    report_timing -max_paths 10 -file consumer-paths.rpt
    report_drc -file consumer-drc.rpt
    write_checkpoint -force consumer-routed.dcp
}
set done [open complete.txt w]; puts $done passed; close $done
'''


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=pathlib.Path)
    parser.add_argument("--run", action="store_true")
    parser.add_argument("--route", action="store_true")
    parser.add_argument("--catalog", type=pathlib.Path)
    args = parser.parse_args()
    catalog = args.catalog.resolve() if args.catalog else None
    catalog_info = None
    catalog_mount = []
    if catalog:
        component = catalog / "xilinx/processing_system7_v5_5/component.xml"
        expected = "5c884fb6b1f01c0cdde22724d3fdf462cdd05f1f31066ea07452c1815006ad10"
        if not component.is_file() or digest(component) != expected:
            parser.error("catalog must contain the verified Vivado 2021.1 PS7 component")
        catalog_info = {"path": str(catalog), "ps7_component_sha256": expected}
        catalog_mount = ["--mount", f"type=bind,source={catalog},target=/tools/Xilinx/Vivado/2021.1/data/ip,readonly"]
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    source = output / "source"
    source.mkdir()
    sources = []
    for name in ["fifo_edit_kernel.sv", "fifo_group_banks.sv", "fifo_group_consumer.sv"]:
        original = pathlib.Path(__file__).parent / "rtl" / name
        copied = source / name
        copied.write_bytes(original.read_bytes())
        sources.append({"path": str(original.relative_to(ROOT)), "sha256": digest(copied)})
    script = output / "run.tcl"
    script.write_text(SCRIPT.replace("ROUTE_ENABLED", "1" if args.route else "0"))
    provenance = {
        "base": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "working_tree_snapshot": True,
        "image": IMAGE,
        "expected_launcher_sha256": LAUNCHER,
        "runner_sha256": digest(pathlib.Path(__file__)),
        "catalog": catalog_info,
        "part": PART,
        "clock_mhz": 100,
        "input_output_delay_ns": 1,
        "profile": {"loops": 128, "states": 512, "instructions": 1024, "buffers": 512},
        "sources": sources,
        "tcl_sha256": digest(script),
        "scope": SCOPE,
        "route": args.route,
    }
    (output / "provenance.json").write_text(json.dumps(provenance, indent=2) + "\n")
    if not args.run:
        return
    build, home = output / "build", output / "home"
    build.mkdir()
    home.mkdir()
    command = [
        "podman", "run", "--rm", "--pull=never", "--network=none", "--cap-drop=ALL",
        "--security-opt=no-new-privileges", "--cpus=8", "--memory=24g", "--shm-size=1g",
        "--mount", f"type=bind,source={source},target=/source,readonly",
        "--mount", f"type=bind,source={build},target=/build",
        "--mount", f"type=bind,source={script},target=/run.tcl,readonly",
        "--mount", f"type=bind,source={home},target=/home/agent",
        *catalog_mount,
        "--workdir", "/build", "-e", "HOME=/home/agent",
        "-e", "LD_PRELOAD=/lib/x86_64-linux-gnu/libudev.so.1",
        "--entrypoint", "/bin/bash", IMAGE, "-c",
        "set -e; launcher=/tools/Xilinx/Vivado/2021.1/bin/vivado; "
        'sha256sum "$launcher"; '
        f'test "$(sha256sum "$launcher" | cut -d" " -f1)" = "{LAUNCHER}"; '
        '"$launcher" -version; '
        '"$launcher" -mode batch -source /run.tcl',
    ]
    with (output / "console.log").open("w") as log:
        result = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, check=False)
    status = result.returncode or (0 if (build / "complete.txt").exists() else 1)
    (output / "status.txt").write_text(str(status) + "\n")
    raise SystemExit(status)


if __name__ == "__main__":
    main()
