#!/usr/bin/env bash
set -euo pipefail
repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
output_dir=${1:-"$repo_root/work/axi-pipeline-synthesis"}
mkdir -p "$output_dir"
output_dir=$(cd -- "$output_dir" && pwd)
cd "$repo_root"
python3 fpga/axi_memory/generate_tops.py --check
sources=()
for item in state_processor:sp_ram,sp_job_fifo,sp_engine,state_processor job_processor:jp_sample_ram,jp_divider,job_processor,audio_pipeline transition_processor:tp_ram,tp_priority,transition_processor chunk_memory:chunk_ram,chunk_memory media_store:media_ram,metadata_cache,media_metadata,media_prefetch_resolver,media_store,memory_delay,media_audio_pipeline loop_pipeline:loop_pipeline axi_memory:axi_master,chunk_dma,word_dma,axi_burst_mux,shared_axi,word_axi,chunk_axi,chunk_memory_axi,media_store_axi,media_audio_pipeline_axi,loop_pipeline_axi; do
    IFS=: read -r directory modules <<< "$item"
    IFS=, read -ra names <<< "$modules"
    for module in "${names[@]}"; do sources+=("fpga/$directory/rtl/$module.sv"); done
done
for width in 32 64; do
    for top in chunk_memory_axi media_store_axi media_audio_pipeline_axi loop_pipeline_axi; do
        verilator --lint-only --Wall --top-module "$top" -GAXI_DATA_WIDTH="$width" \
            -Ifpga/state_processor/rtl "${sources[@]}" > "$output_dir/$top-$width-lint.log" 2>&1
    done
done
yosys -V > "$output_dir/versions.txt"
verilator --version >> "$output_dir/versions.txt"
read_command="read_verilog -sv -Ifpga/state_processor/rtl ${sources[*]}"
yosys -Q -T -q -l "$output_dir/loop-generic.log" -p \
    "$read_command; hierarchy -check -top loop_pipeline_axi; proc; opt; memory -nomap; opt; check -assert; select -assert-none t:\$dlatch t:\$adlatch t:\$div t:\$mod; stat"
yosys -Q -T -q -l "$output_dir/loop-ecp5.log" -p \
    "$read_command; synth_ecp5 -top loop_pipeline_axi -json \"$output_dir/loop-ecp5.json\"; check -assert; stat"
printf 'AXI integration: both widths linted; default loop top generic/ECP5 synthesis passed\n'
