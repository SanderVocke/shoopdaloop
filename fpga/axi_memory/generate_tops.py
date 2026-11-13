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


def generate():
    word_text = (HERE/"rtl/word_axi.sv").read_text()
    _, word_ports = header(word_text)
    axi_ports = [(prefix.replace("DATA_WIDTH", "AXI_DATA_WIDTH"), name)
                 for prefix, name in word_ports if name.startswith("m_axi_")]
    result = {}
    for directory, name in (("chunk_memory", "chunk_memory"), ("media_store", "media_store"),
                            ("media_store", "media_audio_pipeline"), ("loop_pipeline", "loop_pipeline")):
        match, ports = header((FPGA/directory/"rtl"/f"{name}.sv").read_text())
        params = match[2].strip().replace('BURST_MEMORY = 0', 'BURST_MEMORY = 1')
        param_names = re.findall(r"parameter\s+integer\s+(\w+)\s*=", params)
        public = [(p.replace("output reg", "output wire"), n) for p, n in ports if not n.startswith("memory_") or n == "memory_fault"]
        private = [(p, n) for p, n in ports if n.startswith("memory_") and n != "memory_fault"]
        text = '// Generated integration wrapper; do not edit by hand.\n`include "state_processor_defs.svh"\n\n'
        text += f"module {name}_axi #(\n    {params},\n    parameter integer AXI_DATA_WIDTH = 32,\n    parameter [31:0] PHYSICAL_BASE = 32'h01000000\n) (\n"
        text += ",\n".join("    "+p+n for p, n in public+axi_ports)+"\n);\n"
        for prefix, port in private:
            prefix = re.sub(r"(?:input|output)\s+(?:wire|reg)", "wire", prefix)
            text += "    "+prefix+port+";\n"
        text += "    wire [63:0] arena_bytes = 64'(SLOTS)*64'(CHUNK_FRAMES)*4;\n"
        text += "    wire [31:0] physical_base = PHYSICAL_BASE;\n"
        text += f"    {name} #(\n        "+",\n        ".join(f".{p}({p})" for p in param_names)+") core (.*);\n"
        if name == 'chunk_memory':
            text += """    generate if (BURST_MEMORY != 0) begin : bursts
        chunk_axi #(.DATA_WIDTH(AXI_DATA_WIDTH),.MAX_WORDS(32'(CHUNK_FRAMES))) memory_endpoint (.arena_base(memory_arena_base),.*);
    end else begin : words
        wire unused_stream = &{1'b0,memory_count,memory_write_valid,memory_read_ready};
        assign memory_write_ready = 0;
        assign memory_read_valid = 0;
        word_axi #(.DATA_WIDTH(AXI_DATA_WIDTH)) memory_endpoint (.arena_base(memory_arena_base),.*);
    end endgenerate
endmodule
"""
        else:
            text += "    localparam [63:0] STORAGE_WORDS = ((64'(SLOTS)*64'(CHUNK_FRAMES)+15)/16)*16 + 2*((64'(SLOTS/BLOCK_CHUNKS)+15)/16)*16 + 64'(TABLE_PAGES)*16 + 64'(RETIRED_TABLES)*16;\n"
            text += "    initial if ({32'b0,PHYSICAL_BASE}+STORAGE_WORDS*4 > 64'h100000000) $fatal(1,\"DDR arena exceeds AXI address range\");\n"
            text += "    shared_axi #(.DATA_WIDTH(AXI_DATA_WIDTH),.BURST_MEMORY(BURST_MEMORY),.MAX_SAMPLE_WORDS(32'(CHUNK_FRAMES)),.MAX_METADATA_WORDS(32'd16)) memory_endpoint (.arena_base(memory_arena_base),.arena_bytes(STORAGE_WORDS*4),.sample_bytes(arena_bytes),.*);\nendmodule\n"
        result[HERE/"rtl"/f"{name}_axi.sv"] = text
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
