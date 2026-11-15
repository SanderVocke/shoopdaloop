#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
rtl_dir="$repo_root/fpga/state_processor/rtl"
output_dir=${1:-"$repo_root/work/fpga-state-processor"}
mkdir -p -- "$output_dir"
output_dir=$(cd -- "$output_dir" && pwd)
sources=(sp_ram.sv sp_job_fifo.sv sp_engine.sv state_processor.sv)
source_paths=()
read_command="read_verilog -sv -I\"$rtl_dir\""
for source in "${sources[@]}"; do
    source_paths+=("$rtl_dir/$source")
    read_command+=" \"$rtl_dir/$source\""
done

yosys -V > "$output_dir/versions.txt"
verilator --version >> "$output_dir/versions.txt"

for configuration in default:256:256:256 small:7:11:5 minimum:1:1:1; do
    IFS=: read -r name program_depth state_depth job_depth <<< "$configuration"
    parameters="chparam -set PROGRAM_DEPTH $program_depth -set STATE_DEPTH $state_depth -set JOB_DEPTH $job_depth state_processor"
    if ! verilator --lint-only --Wall --top-module state_processor \
        --Mdir "$output_dir/verilator-$name" -I"$rtl_dir" \
        -GPROGRAM_DEPTH="$program_depth" -GSTATE_DEPTH="$state_depth" -GJOB_DEPTH="$job_depth" \
        "${source_paths[@]}" > "$output_dir/$name-lint.log" 2>&1; then
        cat "$output_dir/$name-lint.log"
        exit 1
    fi
    yosys -Q -T -q -l "$output_dir/$name-generic.log" -p \
        "$read_command; $parameters; hierarchy -check -top state_processor; proc; opt; memory -nomap; opt; check -assert; select -assert-none t:\$dlatch t:\$adlatch; stat; write_json \"$output_dir/$name-generic.json\""
    yosys -Q -T -q -l "$output_dir/$name-ecp5.log" -p \
        "$read_command; $parameters; synth_ecp5 -top state_processor -json \"$output_dir/$name-ecp5.json\"; check -assert; stat"
    printf '%s: lint, generic checks, and ECP5 synthesis passed\n' "$name"
done

printf 'Logs and netlists: %s\n' "$output_dir"
