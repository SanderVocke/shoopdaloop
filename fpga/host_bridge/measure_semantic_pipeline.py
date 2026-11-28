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
MMIO_SCOPE = ("Q32 indexed-MMIO/semantic/audio/media/recovery component with low-range discovery/diagnostics/recovery/abort, configuration/CLEAR_VIEW/bulk-media/ENABLE_AUDIO/DRAIN_STOP/optional-trace services and qualified safety sidebands; "
              "complete candidate preparation/lifecycle service map, no matched trusted software, audio adapter, PS/DDR, thermal or protected board integration; "
              "frontend and existing execution/backend hierarchy reported separately")
MMIO_SOURCES = {"semantic_registers.sv", "fifo_config_service.sv", "fifo_mmio.sv", "fifo_mmio_pipeline_axi.sv"}
BOUNDARIES = {
    "audio_execution": "dut/execution/core/pipeline/execution",
    "media_store": "dut/execution/core/media",
    "axi_backend": "dut/execution/memory_endpoint",
    "program_backend": "dut/execution/ddr_program.program_bus",
}


def freeze(source, digest, common_script, *, mmio=False):
    sources = []
    module = "fifo_mmio_pipeline_axi" if mmio else "fifo_recovery_pipeline_axi"
    base = "dut/pipeline" if mmio else "dut"
    boundaries = {key: path.replace("dut/", base + "/", 1) for key, path in BOUNDARIES.items()}
    frontend = FRONTEND_SOURCES | (MMIO_SOURCES if mmio else set())

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
    if mmio:
        for relative in ["fpga/host_bridge/generate_semantic_mmio.py", "fpga/host_bridge/generate_top.py",
                         "fpga/host_bridge/semantic_registers.json"]:
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
    fault_constants = {} if mmio else {"pipeline_fault_cause": "32'd0"}
    fixed = constants | fault_constants
    root_file = source / ("fpga/host_bridge/rtl/fifo_mmio_pipeline_axi.sv" if mmio
                          else "fpga/axi_memory/rtl/fifo_recovery_pipeline_axi.sv")
    _, ports = header(root_file.read_text())
    assert set(fixed) <= {name for _, name in ports}
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
                             for prefix, name in ports if name not in fixed) + "\n);\n"
    profile += f"    {module} #(\n"
    profile += ",\n".join(f"        .{name}({value})" for name, value in params.items()) + "\n    ) dut (\n"
    profile += ",\n".join(f"        .{name}({fixed[name] if name in fixed else name})"
                             for _, name in ports) + "\n    );\nendmodule\n"
    probe = source / "semantic_profile.sv"
    probe.write_text(profile)
    sources.append({"path": "generated/semantic_profile.sv", "sha256": digest(probe)})
    files = [p for p in sorted(source.glob("fpga/*/rtl/*.sv"))
             if p.parent.parent.name != "host_bridge" or p.name in frontend]
    assert {p.name for p in files if p.parent.parent.name == "host_bridge"} == frontend
    if mmio:
        package = source / "fpga/host_bridge/rtl/semantic_registers.sv"
        files.remove(package)
        files.insert(0, package)
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
    if mmio:
        reports = reports.replace("if {ROUTE_ENABLED}", '''opt_design
report_utilization -hierarchical -file component-optimized-utilization.rpt
report_timing_summary -file component-optimized-timing.rpt
write_checkpoint -force component-optimized.dcp
if {ROUTE_ENABLED}''')
    partition = '''set counts [dict create]
foreach cell [get_cells -hierarchical -filter {IS_PRIMITIVE == 1}] {
    set name [get_property NAME $cell]
    set region frontend_and_glue
'''
    for region, boundary in boundaries.items():
        partition += f'    if {{[string match "{boundary}/*" $name]}} {{set region {region}}}\n'
    partition += '''    dict incr counts [list $region [get_property REF_NAME $cell]]
}
set report [open component-partitions.txt w]
puts $report "REGION PRIMITIVE COUNT"
foreach key [lsort [dict keys $counts]] {puts $report "$key [dict get $counts $key]"}
close $report
'''
    required = [*boundaries.values(), f"{base}/recovery", f"{base}/retained_fault",
                f"{base}/execution/core/pipeline/consumer", f"{base}/execution/core/pipeline/observations",
                f"{base}/execution/core/pipeline/snapshots", f"{base}/execution/core/owner/loader", f"{base}/execution/core/owner"]
    if mmio:
        required.append("dut/mmio")
    for boundary in required:
        partition += f'if {{[llength [get_cells -quiet {boundary}]] != 1}} {{error "missing accounting boundary {boundary}"}}\n'
    marker = "set done [open complete.txt w]"
    assert reports.count(marker) == 1
    tcl += reports.replace(marker, partition + marker)
    return sources, tcl, {"parameters": params, "fixed_audio_inputs": constants,
                          "fixed_fault_inputs": fault_constants,
                          "literal_port_defines": definitions,
                          "compiled_sources": [str(p.relative_to(source)) for p in files],
                          "excluded_execution_boundaries": boundaries,
                          "remaining_primitives": "frontend_and_glue; raw counts are not packed LUT area",
                          "indexed_mmio": mmio,
                          "mmio_epoch_reset_coupled": mmio,
                          "standard_optimization": mmio,
                          "low_range_service_included": mmio,
                          "configuration_service_only": False,
                          "clear_view_service_included": mmio,
                          "optional_trace_mmio_included": mmio,
                          "bulk_media_service_included": mmio,
                          "enable_audio_service_included": mmio,
                          "drain_stop_service_included": mmio,
                          "complete_preparation_service_included": mmio,
                          "low_range_discovery_recovery_included": mmio,
                          "abort_mmio_included": mmio,
                          "qualified_fault_sidebands_included": mmio,
                          "scope": MMIO_SCOPE if mmio else SCOPE}
