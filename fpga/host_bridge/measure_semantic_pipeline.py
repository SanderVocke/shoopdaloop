"""Fixed-profile source snapshot for the semantic/media/recovery component."""
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path[:0] = [str(ROOT / "platforms/zynq7000"), str(ROOT / "fpga/axi_memory")]
import boards
from generate_tops import header

SCOPE = ("Q32 semantic/media/recovery component with current production RTL; "
         "no MMIO, audio adapter, PS/DDR, thermal or protected board integration; "
         "frontend and existing execution/backend hierarchy reported separately")
FRONTEND_SOURCES = {
    "fifo_edit_kernel.sv", "fifo_group_banks.sv", "fifo_group_consumer.sv",
    "fifo_media_loader.sv", "fifo_media_arbiter.sv", "fifo_observations.sv",
    "fifo_snapshots.sv", "fifo_audio_pipeline.sv", "fifo_media_audio_pipeline.sv",
    "host_recovery.sv", "host_fault_record.sv",
}
BOUNDARIES = {
    "audio_execution": "dut/execution/core/pipeline/execution",
    "media_store": "dut/execution/core/media",
    "axi_backend": "dut/execution/memory_endpoint",
    "program_backend": "dut/execution/ddr_program.program_bus",
}


def freeze(source, digest, common_script):
    sources = []

    def copy(original):
        relative = original.relative_to(ROOT)
        destination = source / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(original.read_bytes())
        sources.append({"path": str(relative), "sha256": digest(destination)})
        return destination

    directories = ["state_processor", "job_processor", "chunk_memory", "media_store", "axi_memory", "host_bridge"]
    for directory in directories:
        for original in sorted((ROOT / "fpga" / directory / "rtl").iterdir()):
            if original.suffix in {".sv", ".svh"}:
                copy(original)
    for relative in ["platforms/zynq7000/boards/zturn-v2.json", "platforms/zynq7000/boards.py",
                     "fpga/host_bridge/zturn_v2.tcl", "fpga/host_bridge/measure_semantic.py",
                     "fpga/host_bridge/measure_semantic_pipeline.py"]:
        copy(ROOT / relative)
    board = boards.load("zturn-v2")
    if board.data["part"] != "xc7z020clg400-2" or board.data["fpga"]["clock_hz"] != 100_000_000:
        raise ValueError("component measurement requires the approved Z-turn part/clock")
    params = {"EXTERNAL_IO": 1, "ENABLE_MIDI": 0, "DDR_PROGRAM": 1,
              "PROGRAM_DEPTH": 1024, "STATE_DEPTH": 512, "JOB_DEPTH": 1024,
              "LOOP_DEPTH": 128, "BUFFERS": 512, "INPUT_PORTS": 1, "OUTPUT_PORTS": 2,
              "VIEWS": 512, "CHUNK_FRAMES": 32, "BLOCK_CHUNKS": 32,
              "SLOTS": 31 * 1024 * 1024 // 32, "AXI_DATA_WIDTH": 64,
              "PHYSICAL_BASE": board.rust_end, "PHYSICAL_BYTES_LIMIT": board.pl_bytes}
    constants = {"run_quantum": 32, "run_chunk_frames": 32, "run_inputs": 1, "run_outputs": 2}
    root_file = source / "fpga/axi_memory/rtl/fifo_recovery_pipeline_axi.sv"
    _, ports = header(root_file.read_text())
    assert set(constants) <= {name for _, name in ports}
    definitions = dict(re.findall(r"^`define\s+(\w+)\s+(\d+)\s*$",
                                 (source / "fpga/state_processor/rtl/state_processor_defs.svh").read_text(), re.M))

    def port_type(prefix):
        prefix = re.sub(r"`(\w+)", lambda m: definitions[m[1]], prefix)
        for name, value in params.items():
            prefix = re.sub(rf"\b{name}\b", str(value), prefix)
        for width in re.findall(r"\[([^]]+)\]", prefix):
            assert not re.search(r"[A-Za-z_]", width), f"unresolved port width: {prefix}"
        return prefix

    profile = '`include "state_processor_defs.svh"\nmodule semantic_profile (\n'
    profile += ",\n".join("    " + port_type(prefix) + name
                             for prefix, name in ports if name not in constants) + "\n);\n"
    profile += "    fifo_recovery_pipeline_axi #(\n"
    profile += ",\n".join(f"        .{name}({value})" for name, value in params.items()) + "\n    ) dut (\n"
    profile += ",\n".join(f"        .{name}({constants[name] if name in constants else name})"
                             for _, name in ports) + "\n    );\nendmodule\n"
    probe = source / "semantic_profile.sv"
    probe.write_text(profile)
    sources.append({"path": "generated/semantic_profile.sv", "sha256": digest(probe)})
    files = [p for p in sorted(source.glob("fpga/*/rtl/*.sv"))
             if p.parent.parent.name != "host_bridge" or p.name in FRONTEND_SOURCES]
    assert {p.name for p in files if p.parent.parent.name == "host_bridge"} == FRONTEND_SOURCES
    files.append(probe)
    tcl = '''
set_param general.maxThreads 4
if {[version -short] ne "2021.1"} { error "Vivado 2021.1 required" }
create_project -in_memory -part xc7z020clg400-2
set_property include_dirs {/source/fpga/state_processor/rtl /source/fpga/host_bridge/rtl} [current_fileset]
'''
    tcl += "read_verilog -sv [list " + " ".join("/source/" + str(p.relative_to(source)) for p in files) + "]\n"
    tcl += "synth_design -mode out_of_context -flatten_hierarchy none -top semantic_profile -part xc7z020clg400-2\n"
    reports = common_script[common_script.index("create_clock -name core"):].replace("consumer-", "component-")
    partition = '''set counts [dict create]
foreach cell [get_cells -hierarchical -filter {IS_PRIMITIVE == 1}] {
    set name [get_property NAME $cell]
    set region frontend_and_glue
'''
    for region, boundary in BOUNDARIES.items():
        partition += f'    if {{[string match "{boundary}/*" $name]}} {{set region {region}}}\n'
    partition += '''    dict incr counts [list $region [get_property REF_NAME $cell]]
}
set report [open component-partitions.txt w]
puts $report "REGION PRIMITIVE COUNT"
foreach key [lsort [dict keys $counts]] {puts $report "$key [dict get $counts $key]"}
close $report
'''
    for boundary in [*BOUNDARIES.values(), "dut/recovery", "dut/retained_fault",
                     "dut/execution/core/pipeline/consumer", "dut/execution/core/pipeline/observations",
                     "dut/execution/core/pipeline/snapshots", "dut/execution/core/owner/loader", "dut/execution/core/owner"]:
        partition += f'if {{[llength [get_cells -quiet {boundary}]] != 1}} {{error "missing accounting boundary {boundary}"}}\n'
    marker = "set done [open complete.txt w]"
    assert reports.count(marker) == 1
    tcl += reports.replace(marker, partition + marker)
    return sources, tcl, {"parameters": params, "fixed_audio_inputs": constants,
                          "literal_port_defines": definitions,
                          "compiled_sources": [str(p.relative_to(source)) for p in files],
                          "excluded_execution_boundaries": BOUNDARIES,
                          "remaining_primitives": "frontend_and_glue; raw counts are not packed LUT area",
                          "scope": SCOPE}
