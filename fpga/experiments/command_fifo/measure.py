#!/usr/bin/env python3
"""Freeze and measure the isolated command kernel, without board integration."""
import argparse
import hashlib
import json
import pathlib
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[3]
IMAGE = "docker.io/siliconbootcamp/xilinx-vivado@sha256:d24586405894b07f6af006d5cb15ba01c7e8585de97b28b6121fcf6282db7a5f"
PART = "xc7z020clg400-2"
SCRIPT = r'''
set_param general.maxThreads 4
puts "INVESTIGATION_TOOL [version -short]"
foreach {name top loops route} {kernel fifo_edit_kernel 128 1 probe16 fifo_edit_probe 16 0 probe32 fifo_edit_probe 32 0 probe128 fifo_edit_probe 128 1 probe512 fifo_edit_probe 512 0} {
    create_project -in_memory -part xc7z020clg400-2
    read_verilog -sv /source/edit_kernel.sv
    read_verilog -sv /source/probe.sv
    synth_design -mode out_of_context -flatten_hierarchy none -top $top -part xc7z020clg400-2 -generic LOOPS=$loops
    create_clock -name core -period 10 [get_ports clk]
    set_input_delay -clock core 1 [get_ports -filter {DIRECTION == IN && NAME != clk}]
    set_output_delay -clock core 1 [all_outputs]
    report_utilization -hierarchical -file $name-utilization.rpt
    report_timing_summary -file $name-synthesis-timing.rpt
    set report [open $name-primitives.txt w]
    foreach {label filter} {LUT {REF_NAME =~ LUT*} FF {REF_NAME =~ FD*} CARRY4 {REF_NAME == CARRY4} RAMB36 {REF_NAME == RAMB36E1} RAMB18 {REF_NAME == RAMB18E1} DSP {REF_NAME == DSP48E1}} {
        puts $report "$label [llength [get_cells -hierarchical -filter $filter]]"
    }
    puts $report "PORT_BITS [llength [get_ports]]"
    puts $report "SEQUENTIAL [llength [get_cells -hierarchical -filter {IS_SEQUENTIAL == 1}]]"
    close $report
    write_checkpoint -force $name-synthesis.dcp
    if {$route} {
        opt_design
        place_design
        route_design
        report_utilization -hierarchical -file $name-routed-utilization.rpt
        report_timing_summary -file $name-routed-timing.rpt
        report_timing -max_paths 10 -file $name-paths.rpt
        report_drc -file $name-drc.rpt
        write_checkpoint -force $name-routed.dcp
    }
    close_project
}
set done [open complete.txt w]; puts $done passed; close $done
'''


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=pathlib.Path)
    parser.add_argument("--run", action="store_true")
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    source = output / "source"
    source.mkdir()
    manifests = []
    for name in ["edit_kernel.sv", "probe.sv"]:
        original = pathlib.Path(__file__).parent / name
        copied = source / name
        copied.write_bytes(original.read_bytes())
        manifests.append({"path": str(original.relative_to(ROOT)), "sha256": digest(copied)})
    (output / "run.tcl").write_text(SCRIPT)
    provenance = {
        "base": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "working_tree_snapshot": True,
        "image": IMAGE,
        "part": PART,
        "clock_mhz": 100,
        "input_output_delay_ns": 1,
        "sources": manifests,
        "tcl_sha256": digest(output / "run.tcl"),
        "scope": "isolated kernel and synthetic local RAM probe; no PS/audio/FIFO/frontend or board",
        "cases": {"kernel": {"loops": 128, "route": True}, **{
            f"probe{loops}": {"loops": loops, "states": 512, "route": loops == 128}
            for loops in [16, 32, 128, 512]
        }},
    }
    (output / "provenance.json").write_text(json.dumps(provenance, indent=2) + "\n")
    if not args.run:
        return
    build = output / "build"
    build.mkdir()
    home = output / "home"
    home.mkdir()
    command = [
        "podman", "run", "--rm", "--network=none", "--cap-drop=ALL",
        "--security-opt=no-new-privileges", "--cpus=8", "--memory=24g", "--shm-size=1g",
        "--mount", f"type=bind,source={source},target=/source,readonly",
        "--mount", f"type=bind,source={build},target=/build",
        "--mount", f"type=bind,source={output / 'run.tcl'},target=/run.tcl,readonly",
        "--mount", f"type=bind,source={home},target=/home/agent",
        "--workdir", "/build", "-e", "HOME=/home/agent",
        "-e", "LD_PRELOAD=/lib/x86_64-linux-gnu/libudev.so.1",
        "--entrypoint", "/bin/bash", IMAGE, "-c",
        "set -e; sha256sum /tools/Xilinx/Vivado/2021.1/bin/vivado; "
        "/tools/Xilinx/Vivado/2021.1/bin/vivado -version; "
        "/tools/Xilinx/Vivado/2021.1/bin/vivado -mode batch -source /run.tcl",
    ]
    with (output / "console.log").open("w") as log:
        result = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, check=False)
    status = result.returncode or (0 if (build / "complete.txt").exists() else 1)
    (output / "status.txt").write_text(str(status) + "\n")
    raise SystemExit(status)


if __name__ == "__main__":
    main()
