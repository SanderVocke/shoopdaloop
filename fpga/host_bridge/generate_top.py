#!/usr/bin/env python3
"""Wire the host register bridge to the current AXI media pipeline."""

import argparse
from pathlib import Path
import re
import sys

HERE = Path(__file__).resolve().parent
AXI = HERE.parent / "axi_memory"
sys.path.insert(0, str(AXI))
from generate_tops import header
from generate_semantic import generate as semantic_audio
from generate_semantic_media import generate as semantic_media
from generate_semantic_mmio import generate as semantic_mmio, generate_audio as semantic_audio_host


def generate():
    rtl = HERE / "rtl"
    core, ports = header((AXI / "rtl/media_audio_pipeline_axi.sv").read_text())
    _, mailbox = header((rtl / "host_mailbox.sv").read_text())
    _, dispatch = header((rtl / "host_dispatch.sv").read_text())
    public_mailbox = {"clk", "reset", "epoch", "fault", "irq", "audio_xruns"}
    public_core = ("m_axi_", "sample_")
    mailbox_ports = {name for _, name in mailbox}
    dispatch_ports = {name for _, name in dispatch}
    core_constants = {
        "arena_base": "{32'b0, ARENA_BASE}",
        "maintenance_enable": "!recovery_quiesce", "maintenance_tick": "1'b0",
        "run_program_region": "1'b0", "run_sync_count": "32'b0", "run_instruction_count": "32'b0",
        "run_state_count": "32'b0", "run_buffers": "32'b0",
        "job_enable": "1'b1", "drain_ready": "1'b0",
        "loop_read_ready": "1'b0", "loop_response_valid": "1'b0", "loop_response_error": "1'b0",
        "loop_response_data": "164'b0",
        "store_cmd_start": "64'b0", "store_cmd_length": "64'b0",
        "store_cmd_count": "32'b0", "store_write_valid": "1'b0", "store_write_data": "32'b0",
        "store_read_ready": "1'b0",
        "midi_job_ready": "1'b0", "midi_job_done": "1'b0", "midi_job_error": "4'b0",
        "midi_job_ready": "1'b0", "midi_job_done": "1'b0", "midi_job_error": "4'b0",
    }
    public = []
    local = []
    unused = []
    for decl, name in mailbox:
        if name in public_mailbox or name.startswith("s_axi_"):
            if name == "fault":
                decl = "output wire "
            public.append((decl.replace("output reg", "output wire"), name))
        elif not name.startswith("quantum_"):
            assert name in dispatch_ports or name.startswith(("sync_event_", "recovery_")), name
            local.append((decl, name))
    public.extend([("output wire ", "runtime_reset"), ("output wire [31:0] ", "runtime_epoch")])
    public.extend((decl.replace("output reg", "output wire"), name)
                  for decl, name in dispatch if name.startswith("quantum_") or name in {"audio_enable", "audio_fault"})
    for decl, name in ports:
        if name.startswith(public_core):
            public.append((decl, name))
        elif name not in mailbox_ports and name not in core_constants:
            assert name in dispatch_ports or decl.startswith("output"), name
            local.append((decl, name))
            if name not in dispatch_ports:
                unused.append(name)

    core_params = re.sub(r"\s*parameter integer ENABLE_MIDI = 0,", "", core[2])
    params = core_params.strip().replace("LIVE_LOADING = 0", "LIVE_LOADING = 1")
    params = params.replace("EXPORT_SYNC_EVENTS = 0", "EXPORT_SYNC_EVENTS = 1")
    params = params.replace("parameter integer ENABLE_MIDI = 0,\n    ", "")
    params = params.replace("AXI_DATA_WIDTH = 32", "AXI_DATA_WIDTH = 64")
    params = params.replace("parameter [31:0] PHYSICAL_BASE = 32'h01000000",
                            "parameter [31:0] ARENA_BASE = 32'h30000000")
    params = params.replace(",\n    parameter [63:0] PHYSICAL_BYTES_LIMIT = 64'h100000000", "")
    param_names = re.findall(r"parameter\s+integer\s+(\w+)\s*=", params)
    params += ",\n    parameter [31:0] ARENA_BYTES = 32'h08000000,\n"
    params += "    parameter [31:0] CORE_CLOCK_HZ = 100_000_000,\n"
    params += "    parameter integer CONTROL_CYCLES = CORE_CLOCK_HZ / 6000,\n"
    params += "    parameter integer QUANTUM = 32, parameter integer WATCHDOG_CYCLES = 20_000_000"
    attributes = {
        "clk": ('X_INTERFACE_INFO = "xilinx.com:signal:clock:1.0 clk CLK", '
                'X_INTERFACE_PARAMETER = "ASSOCIATED_BUSIF s_axi:m_axi, ASSOCIATED_RESET reset"'),
        "reset": ('X_INTERFACE_INFO = "xilinx.com:signal:reset:1.0 reset RST", '
                  'X_INTERFACE_PARAMETER = "POLARITY ACTIVE_HIGH"'),
        "irq": ('X_INTERFACE_INFO = "xilinx.com:signal:interrupt:1.0 irq INTERRUPT", '
                'X_INTERFACE_PARAMETER = "SENSITIVITY LEVEL_HIGH"'),
    }
    declarations = []
    for decl, name in public:
        annotation = attributes.get(name)
        if name.startswith(("s_axi_", "m_axi_")):
            bus, signal = name[:5], name[6:].upper()
            annotation = f'X_INTERFACE_INFO = "xilinx.com:interface:aximm:1.0 {bus} {signal}"'
            if signal == "AWADDR":
                protocol = "AXI4LITE" if bus == "s_axi" else "AXI3"
                annotation += f', X_INTERFACE_PARAMETER = "PROTOCOL {protocol}"'
        declarations.append((f"    (* {annotation} *)\n" if annotation else "") + f"    {decl}{name}")
    module_header = " #(\n    " + params + "\n) (\n" + ",\n".join(declarations) + "\n);\n"
    text = '// Generated by generate_top.py.\n`include "state_processor_defs.svh"\n\n'
    text += "module host_pipeline_axi" + module_header
    for decl, name in local:
        text += "    " + re.sub(r"(?:input|output)\s+(?:wire|reg)", "wire", decl) + name + ";\n"
    text += "    wire unused_core_outputs = &{1'b0," + ",".join(unused) + "};\n"
    text += "    wire recovery_quiesce, recovery_reset, mailbox_abort;\n"
    text += "    wire [31:0] mailbox_epoch = recovery_reset ? 32'b0 : runtime_epoch;\n"
    text += "    assign runtime_reset = reset || recovery_reset;\n"
    text += "    assign host_abort = mailbox_abort || recovery_quiesce;\n"
    text += "    host_recovery recovery (.clk(clk), .reset(reset), .seed_epoch(epoch),\n"
    text += "        .request(recovery_request), .state(recovery_status), .epoch(runtime_epoch),\n"
    text += "        .quiesce(recovery_quiesce), .datapath_reset(recovery_reset), .bus_fault(1'b0),\n"
    text += "        .quiescent(!busy && !state_busy && !jobs_busy && !store_busy && !reclaim_active\n"
    text += "            && queued_jobs == 0 && !m_axi_awvalid && !m_axi_arvalid && !m_axi_wvalid),\n"
    text += "        .write_start(m_axi_awvalid && m_axi_awready), .write_done(m_axi_bvalid && m_axi_bready),\n"
    text += "        .read_start(m_axi_arvalid && m_axi_arready), .read_done(m_axi_rvalid && m_axi_rready && m_axi_rlast));\n"
    text += "    initial begin\n"
    text += '        if (LIVE_LOADING != 1 || PROGRAM_REGIONS != 2) $fatal(1,"host requires live loading");\n'
    text += '        if (QUANTUM > MAX_FRAMES || QUANTUM < 1) $fatal(1,"invalid host quantum");\n'
    text += "    end\n"
    text += "    host_mailbox #(.RECOVERY_CAPABLE(1), .CORE_CLOCK_HZ(CORE_CLOCK_HZ), .ARENA_BASE(ARENA_BASE), .ARENA_BYTES(ARENA_BYTES),\n"
    text += "        .PROGRAM_DEPTH(PROGRAM_DEPTH), .STATE_DEPTH(STATE_DEPTH), .JOB_DEPTH(JOB_DEPTH > 512 ? 512 : JOB_DEPTH),\n"
    text += "        .BUFFERS(BUFFERS), .VIEWS(VIEWS), .MEDIA_BLOCKS(SLOTS/BLOCK_CHUNKS),\n"
    text += "        .CHUNK_FRAMES(CHUNK_FRAMES), .BLOCK_CHUNKS(BLOCK_CHUNKS), .QUANTUM(QUANTUM)) mmio (.epoch(mailbox_epoch), .host_abort(mailbox_abort), .*);\n"
    text += "    host_dispatch #(.QUANTUM(QUANTUM), .CHUNK_FRAMES(CHUNK_FRAMES), .INPUT_PORTS(INPUT_PORTS),\n"
    text += "        .OUTPUT_PORTS(OUTPUT_PORTS), .STATE_DEPTH(STATE_DEPTH), .CONTROL_CYCLES(CONTROL_CYCLES),\n"
    text += "        .WATCHDOG_CYCLES(WATCHDOG_CYCLES)) dispatcher (.reset(runtime_reset), .epoch(runtime_epoch), .*);\n"
    text += "    media_audio_pipeline_axi #(\n        "
    text += ",\n        ".join(f".{name}({name})" for name in param_names)
    text += ",\n        .ENABLE_MIDI(0), .PHYSICAL_BASE(ARENA_BASE), .PHYSICAL_BYTES_LIMIT({32'b0, ARENA_BYTES})) core (\n        "
    text += ",\n        ".join(f".{name}({value})" for name, value in core_constants.items())
    text += ",\n        .reset(runtime_reset), .*);\nendmodule\n"
    # Vivado 2021.1 requires a Verilog or VHDL top for block design module references.
    wrapper = "// Generated by generate_top.py.\nmodule host_pipeline_bd" + module_header
    all_parameters = re.findall(r"parameter\s+(?:integer|\[31:0\])\s+(\w+)\s*=", params)
    wrapper += "    host_pipeline_axi #(\n        "
    wrapper += ",\n        ".join(f".{name}({name})" for name in all_parameters)
    wrapper += "\n    ) bridge (\n        "
    wrapper += ",\n        ".join(f".{name}({name})" for _, name in public)
    wrapper += "\n    );\nendmodule\n"
    native = (rtl / "native_audio_mmio.sv").read_text().split("\n);", 1)[0] + "\n);\n"
    native_ports = re.findall(r"^    (?:input|output) (?:wire|reg) (?:\[[^\]]+\] )?(\w+),?$", native, re.M)
    native_wrapper = "// Generated by generate_top.py.\n"
    native_wrapper += native.replace("module native_audio_mmio", "module native_audio_bd").replace("output reg", "output wire")
    native_wrapper += "    native_audio_mmio #(.CORE_CLOCK_HZ(CORE_CLOCK_HZ)) mmio (\n        "
    native_wrapper += ",\n        ".join(f".{name}({name})" for name in native_ports)
    native_wrapper += "\n    );\nendmodule\n"
    return {"host_pipeline_axi.sv": text, "host_pipeline_bd.v": wrapper,
            "native_audio_bd.v": native_wrapper, "fifo_audio_pipeline.sv": semantic_audio(),
            "fifo_media_audio_pipeline.sv": semantic_media(),
            "fifo_mmio_pipeline_axi.sv": semantic_mmio(), "fifo_audio_host_axi.sv": semantic_audio_host()}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    check = parser.parse_args().check
    for name, source in generate().items():
        output = HERE / "rtl" / name
        if check:
            if not output.exists() or output.read_text() != source:
                raise SystemExit(f"Stale generated top: {output}")
        else:
            output.write_text(source)
