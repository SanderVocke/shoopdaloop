#!/usr/bin/env bash
set -euo pipefail
repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
output_dir=${1:-"$repo_root/work/axi-synthesis"}
mkdir -p "$output_dir"
output_dir=$(cd -- "$output_dir" && pwd)
cd "$repo_root"
sources=(fpga/axi_memory/rtl/axi_master.sv fpga/axi_memory/rtl/word_axi.sv)
for width in 32 64; do
    for top in axi_master word_axi; do
        name=$top-$width
        verilator --lint-only --Wall --top-module "$top" -GDATA_WIDTH="$width" "${sources[@]}" > "$output_dir/$name-lint.log" 2>&1
        yosys -Q -T -q -l "$output_dir/$name-ecp5.log" -p \
            "read_verilog -sv ${sources[*]}; chparam -set DATA_WIDTH $width $top; synth_ecp5 -top $top -json \"$output_dir/$name.json\"; check -assert; select -assert-none t:\$dlatch t:\$adlatch t:\$div t:\$mod; stat"
        printf '%s: lint and ECP5 synthesis passed\n' "$name"
    done
done
