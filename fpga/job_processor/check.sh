#!/usr/bin/env bash
set -euo pipefail
repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
output_dir=${1:-"$repo_root/work/fpga-job-synthesis"}
mkdir -p "$output_dir"
output_dir=$(cd -- "$output_dir" && pwd)
state_rtl="$repo_root/fpga/state_processor/rtl"
job_rtl="$repo_root/fpga/job_processor/rtl"
sources=()
for source in sp_ram sp_job_fifo sp_engine state_processor; do sources+=("$state_rtl/$source.sv"); done
for source in jp_sample_ram jp_divider job_processor audio_pipeline; do sources+=("$job_rtl/$source.sv"); done
read_command='read_verilog -sv -Ifpga/state_processor/rtl'
for source in "${sources[@]}"; do read_command+=" \"$source\""; done
yosys -V > "$output_dir/versions.txt"
verilator --version >> "$output_dir/versions.txt"
cd "$repo_root"
for top in job_processor audio_pipeline; do
    if ! verilator --lint-only --Wall --top-module "$top" -I"$state_rtl" \
        "${sources[@]}" > "$output_dir/$top-lint.log" 2>&1; then
        cat "$output_dir/$top-lint.log"
        exit 1
    fi
    yosys -Q -T -q -l "$output_dir/$top-generic.log" -p \
        "$read_command; hierarchy -check -top $top; proc; opt; memory -nomap; opt; check -assert; select -assert-none t:\$dlatch t:\$adlatch t:\$div t:\$mod; stat"
    yosys -Q -T -q -l "$output_dir/$top-ecp5.log" -p \
        "$read_command; synth_ecp5 -top $top -json \"$output_dir/$top-ecp5.json\"; check -assert; stat"
    printf '%s: lint and generic/ECP5 synthesis passed\n' "$top"
done
