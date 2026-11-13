#!/usr/bin/env bash
set -euo pipefail
repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
output_dir=${1:-"$repo_root/work/fpga-chunk-synthesis"}
mkdir -p "$output_dir"
output_dir=$(cd -- "$output_dir" && pwd)
cd "$repo_root"
sources=(fpga/chunk_memory/rtl/chunk_ram.sv fpga/chunk_memory/rtl/chunk_priority.sv fpga/chunk_memory/rtl/chunk_memory.sv)
read_command="read_verilog -sv ${sources[*]}"
yosys -V > "$output_dir/versions.txt"
verilator --version >> "$output_dir/versions.txt"
for configuration in default:16:32:4:4 odd:9:8:3:3 minimum:3:1:1:1; do
    IFS=: read -r name slots frames reads writes <<< "$configuration"
    verilator --lint-only --Wall --top-module chunk_memory -GSLOTS="$slots" -GCHUNK_FRAMES="$frames" \
        -GREAD_BUFFERS="$reads" -GWRITE_BUFFERS="$writes" "${sources[@]}" > "$output_dir/$name-lint.log" 2>&1
    parameters="chparam -set SLOTS $slots -set CHUNK_FRAMES $frames -set READ_BUFFERS $reads -set WRITE_BUFFERS $writes chunk_memory"
    yosys -Q -T -q -l "$output_dir/$name-generic.log" -p \
        "$read_command; $parameters; hierarchy -check -top chunk_memory; proc; opt; memory -nomap; opt; check -assert; select -assert-none t:\$dlatch t:\$adlatch t:\$div t:\$mod; stat"
    yosys -Q -T -q -l "$output_dir/$name-ecp5.log" -p \
        "$read_command; $parameters; synth_ecp5 -top chunk_memory -json \"$output_dir/$name-ecp5.json\"; check -assert; stat"
    printf '%s: chunk-memory lint and generic/ECP5 synthesis passed\n' "$name"
done
