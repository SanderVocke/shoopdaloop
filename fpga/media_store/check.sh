#!/usr/bin/env bash
set -euo pipefail
repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
output_dir=${1:-"$repo_root/work/fpga-media-synthesis"}
mkdir -p "$output_dir"
output_dir=$(cd -- "$output_dir" && pwd)
cd "$repo_root"
sources=()
sources+=(fpga/chunk_memory/rtl/chunk_ram.sv fpga/chunk_memory/rtl/chunk_memory.sv)
for module in sp_ram sp_job_fifo sp_engine state_processor; do sources+=("fpga/state_processor/rtl/$module.sv"); done
for module in jp_sample_ram jp_divider job_processor audio_pipeline; do sources+=("fpga/job_processor/rtl/$module.sv"); done
sources+=(fpga/media_store/rtl/media_ram.sv fpga/media_store/rtl/media_prefetch_resolver.sv fpga/media_store/rtl/media_store.sv fpga/media_store/rtl/memory_delay.sv fpga/media_store/rtl/media_audio_pipeline.sv)
read_command='read_verilog -sv -Ifpga/state_processor/rtl'
for source in "${sources[@]}"; do read_command+=" \"$source\""; done
yosys -V > "$output_dir/versions.txt"
verilator --version >> "$output_dir/versions.txt"
for top in media_store media_audio_pipeline; do
    verilator --lint-only --Wall --top-module "$top" -Ifpga/state_processor/rtl \
        "${sources[@]}" > "$output_dir/$top-lint.log" 2>&1
    yosys -Q -T -q -l "$output_dir/$top-generic.log" -p \
        "$read_command; hierarchy -check -top $top; proc; opt; memory -nomap; opt; check -assert; select -assert-none t:\$dlatch t:\$adlatch t:\$div t:\$mod; stat"
    yosys -Q -T -q -l "$output_dir/$top-ecp5.log" -p \
        "$read_command; synth_ecp5 -top $top -json \"$output_dir/$top-ecp5.json\"; check -assert; stat"
    printf '%s: lint and generic/ECP5 synthesis passed\n' "$top"
done
