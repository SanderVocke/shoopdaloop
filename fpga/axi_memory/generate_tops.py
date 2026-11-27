#!/usr/bin/env python3
"""Generate thin AXI tops from the public core ports; --check detects drift."""
import argparse
from pathlib import Path
import re

HERE = Path(__file__).resolve().parent
FPGA = HERE.parent

def header(text):
    match = re.search(r"module (\w+) #\((.*?)\)\s*\((.*?)\);", text, re.S)
    ports, declaration = [], None
    for part in match[3].split(","):
        part = part.strip()
        match_port = re.fullmatch(r"((?:input|output)\s+(?:wire|reg)\s*(?:\[[^\]]+\]\s*)?)(\w+)", part)
        if match_port:
            declaration, name = match_port.groups()
        else:
            assert declaration is not None and re.fullmatch(r"\w+", part), part
            name = part
        ports.append((declaration, name))
    return match, ports


def recovery_top(text):
    match, ports = header(text)
    params = match[2].strip()
    param_names = re.findall(r"parameter\s+(?:integer|\[[^\]]+\])\s+(\w+)\s*=", params)
    public = [(p, n) for p, n in ports if n != 'axi_quiesce']
    public += [('input wire ', 'recovery_request'), ('input wire [31:0] ', 'recovery_epoch'),
               ('output wire [2:0] ', 'recovery_state'), ('output wire [31:0] ', 'runtime_epoch'),
               ('output wire ', 'runtime_reset')]
    result = '// Generated integration wrapper; do not edit by hand.\n'
    result += 'module fifo_recovery_pipeline_axi #(\n    ' + params
    result += ',\n    parameter integer RECOVERY_TIMEOUT_CYCLES = 100_000_000,\n    parameter integer RECOVERY_RESET_CYCLES = 128\n) (\n'
    result += ',\n'.join('    '+p+n for p, n in public)+'\n);\n'
    result += '''    wire recovery_quiesce, recovery_reset;
    assign runtime_reset = reset || recovery_reset;
    wire request = recovery_request && recovery_epoch == runtime_epoch && runtime_epoch != 0 && !runtime_reset;
    wire recovery_bus_fault = axi_drain_fault || (recovery_reset && (m_axi_bvalid || m_axi_rvalid));
    host_recovery #(.TIMEOUT_CYCLES(RECOVERY_TIMEOUT_CYCLES), .RESET_CYCLES(RECOVERY_RESET_CYCLES)) recovery (
        .clk(clk), .reset(reset), .seed_epoch(epoch), .request(request),
        .quiescent(axi_quiescent), .bus_fault(recovery_bus_fault),
        .write_start(m_axi_awvalid && m_axi_awready), .write_done(m_axi_bvalid && m_axi_bready),
        .read_start(m_axi_arvalid && m_axi_arready), .read_done(m_axi_rvalid && m_axi_rready && m_axi_rlast),
        .quiesce(recovery_quiesce), .datapath_reset(recovery_reset), .epoch(runtime_epoch), .state(recovery_state));
'''
    result += '    fifo_media_audio_pipeline_axi #(\n        '
    result += ',\n        '.join(f'.{p}({p})' for p in param_names)
    result += ') execution (.reset(runtime_reset), .epoch(runtime_epoch), .axi_quiesce(recovery_quiesce), .*);\nendmodule\n'
    return result


def generate():
    word_text = (HERE/"rtl/word_axi.sv").read_text()
    _, word_ports = header(word_text)
    axi_ports = [(prefix.replace("DATA_WIDTH", "AXI_DATA_WIDTH"), name)
                 for prefix, name in word_ports if name.startswith("m_axi_")]
    result = {}
    for directory, name in (("chunk_memory", "chunk_memory"), ("media_store", "media_store"),
                            ("media_store", "media_audio_pipeline"),
                            ("host_bridge", "fifo_media_audio_pipeline")):
        is_pipeline = name.endswith("audio_pipeline")
        match, ports = header((FPGA/directory/"rtl"/f"{name}.sv").read_text())
        params = match[2].strip().replace('BURST_MEMORY = 0', 'BURST_MEMORY = 1')
        param_names = re.findall(r"parameter\s+integer\s+(\w+)\s*=", params)
        public = [(p.replace("output reg", "output wire"), n) for p, n in ports if not n.startswith("memory_") or n == "memory_fault"]
        private = [(p, n) for p, n in ports if n.startswith("memory_") and n != "memory_fault"]
        text = '// Generated integration wrapper; do not edit by hand.\n`include "state_processor_defs.svh"\n\n'
        text += f"module {name}_axi #(\n    {params},\n    parameter integer AXI_DATA_WIDTH = 32,\n    parameter [31:0] PHYSICAL_BASE = 32'h01000000"
        if is_pipeline:
            text += ",\n    parameter [63:0] PHYSICAL_BYTES_LIMIT = 64'h100000000"
        text += "\n) (\n"
        recovery_ports = [("input wire ", "axi_quiesce"), ("output wire ", "axi_quiescent"),
                          ("output wire ", "axi_drain_fault")] if name == 'fifo_media_audio_pipeline' else []
        text += ",\n".join("    "+p+n for p, n in public+recovery_ports+axi_ports)+"\n);\n"
        for prefix, port in private:
            prefix = re.sub(r"(?:input|output)\s+(?:wire|reg)", "wire", prefix)
            text += "    "+prefix+port+";\n"
        text += "    wire [63:0] arena_bytes = 64'(SLOTS)*64'(CHUNK_FRAMES)*4;\n"
        text += "    wire [31:0] physical_base = PHYSICAL_BASE;\n"
        core_ports = ".*"
        if name == 'fifo_media_audio_pipeline':
            text += "    localparam integer PROGRAM_REGIONS = 2;\n"
            core_ports += ", .pipeline_fault(pipeline_fault || axi_quiesce || axi_drain_fault)"
        if is_pipeline:
            core_ports += ", " + ", ".join(
                f".{n}(media_{n})" for p, n in private if n.startswith('memory_meta_'))
            for p, n in private:
                if n.startswith('memory_meta_'):
                    prefix = re.sub(r"(?:input|output)\s+(?:wire|reg)", "wire", p)
                    text += "    "+prefix+"media_"+n+";\n"
        text += f"    {name} #(\n        "+",\n        ".join(f".{p}({p})" for p in param_names)+f") core ({core_ports});\n"
        if name == 'chunk_memory':
            text += """    generate if (BURST_MEMORY != 0) begin : bursts
        chunk_axi #(.DATA_WIDTH(AXI_DATA_WIDTH),.MAX_WORDS(32'(CHUNK_FRAMES))) memory_endpoint (.arena_base(memory_arena_base),.*);
    end else begin : words
        wire unused_stream = &{1'b0,memory_count,memory_write_valid,memory_read_ready};
        assign memory_write_ready = 0;
        assign memory_read_valid = 0;
        word_axi #(.DATA_WIDTH(AXI_DATA_WIDTH)) memory_endpoint (.arena_base(memory_arena_base),
            .memory_record(1'b0), .memory_write_record(128'b0), .memory_read_record(), .*);
    end endgenerate
endmodule
"""
        else:
            text += "    localparam [63:0] STORAGE_WORDS = ((64'(SLOTS)*64'(CHUNK_FRAMES)+15)/16)*16 + 2*((64'(SLOTS/BLOCK_CHUNKS)+15)/16)*16 + 64'(TABLE_PAGES)*16 + 64'(RETIRED_TABLES)*16;\n"
            bytes_used = "STORAGE_WORDS*4"
            if is_pipeline:
                text += """    localparam [63:0] PROGRAM_OFFSET = ((STORAGE_WORDS*4+63)/64)*64;
    localparam [63:0] TOTAL_BYTES = DDR_PROGRAM != 0
        ? PROGRAM_OFFSET + 64'(PROGRAM_REGIONS)*64'(PROGRAM_DEPTH)*16
        : STORAGE_WORDS*4;
    initial if (TOTAL_BYTES > PHYSICAL_BYTES_LIMIT) $fatal(1,"DDR arena exceeds reserved memory");
    initial if (DDR_PROGRAM != 0 && BURST_MEMORY == 0) $fatal(1,"DDR program requires burst memory");
"""
                bytes_used = "TOTAL_BYTES"
                meta_ports = [(p, n) for p, n in private if n.startswith('memory_meta_')]
                text += "    generate if (DDR_PROGRAM != 0) begin : ddr_program\n"
                text += "        program_axi_mux program_bus (\n            .clk(clk), .reset(reset),\n"
                connections = []
                for _, n in meta_ports:
                    port = n.removeprefix('memory_meta_')
                    media = f"media_{n}"
                    program = f"memory_program_{port}"
                    if port == 'address':
                        program = "memory_arena_base + PROGRAM_OFFSET + {44'b0,memory_program_address,4'b0}"
                    connections.append(f"            .media_{port}({media}), .program_{port}({program}), .memory_{port}({n})")
                text += ",\n".join(connections) + "\n        );\n"
                text += "    end else begin : local_program\n"
                for p, n in meta_ports:
                    dst, src = (n, f"media_{n}") if p.startswith("output") else (f"media_{n}", n)
                    text += f"        assign {dst} = {src};\n"
                for p, n in private:
                    if n.startswith('memory_program_') and p.startswith("input"):
                        text += f"        assign {n} = 0;\n"
                unused = ",".join(n for p, n in private if n.startswith('memory_program_') and p.startswith("output"))
                text += "        wire unused_program = &{1'b0," + unused + "};\n"
                text += "    end endgenerate\n"
            text += f"    initial if ({{32'b0,PHYSICAL_BASE}}+{bytes_used} > 64'h100000000) $fatal(1,\"DDR arena exceeds AXI address range\");\n"
            recovery_links = ".quiesce(axi_quiesce),.quiescent(axi_quiescent),.drain_fault(axi_drain_fault)" if recovery_ports else ".quiesce(1'b0),.quiescent(),.drain_fault()"
            text += f"    shared_axi #(.DATA_WIDTH(AXI_DATA_WIDTH),.BURST_MEMORY(BURST_MEMORY),.MAX_SAMPLE_WORDS(32'(CHUNK_FRAMES)),.MAX_METADATA_WORDS(32'd16)) memory_endpoint (.arena_base(memory_arena_base),.arena_bytes({bytes_used}),.sample_bytes(arena_bytes),{recovery_links},.*);\nendmodule\n"
        result[HERE/"rtl"/f"{name}_axi.sv"] = text
        if name == 'fifo_media_audio_pipeline':
            result[HERE/"rtl"/"fifo_recovery_pipeline_axi.sv"] = recovery_top(text)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    for path, text in generate().items():
        if args.check:
            if not path.is_file() or path.read_text() != text:
                raise SystemExit(f"stale generated top: {path}; run {__file__}")
        else:
            path.write_text(text)


if __name__ == "__main__":
    main()
