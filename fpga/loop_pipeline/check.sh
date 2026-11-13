#!/usr/bin/env bash
set -euo pipefail
repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
output_dir=${1:-"$repo_root/work/fpga-loop-synthesis"}
mkdir -p "$output_dir"
output_dir=$(cd -- "$output_dir" && pwd)
cd "$repo_root"
sources=()
sources+=(fpga/chunk_memory/rtl/chunk_ram.sv fpga/chunk_memory/rtl/chunk_memory.sv)
for source in sp_ram sp_job_fifo sp_engine state_processor; do sources+=("fpga/state_processor/rtl/$source.sv"); done
for source in jp_sample_ram jp_divider job_processor audio_pipeline; do sources+=("fpga/job_processor/rtl/$source.sv"); done
for source in tp_ram tp_priority transition_processor; do sources+=("fpga/transition_processor/rtl/$source.sv"); done
for source in media_ram metadata_cache media_metadata media_reclaimer media_prefetch_resolver media_store memory_delay media_audio_pipeline; do sources+=("fpga/media_store/rtl/$source.sv"); done
sources+=(fpga/loop_pipeline/rtl/loop_pipeline.sv)
read_command="read_verilog -sv -Ifpga/state_processor/rtl ${sources[*]}"
yosys -V > "$output_dir/versions.txt"
verilator --version >> "$output_dir/versions.txt"
for configuration in default:8:16:32 small:3:5:11 minimum:1:1:3; do
    IFS=: read -r name loops requests events <<< "$configuration"
    parameters="chparam -set LOOP_DEPTH $loops -set REQUEST_DEPTH $requests -set EVENT_DEPTH $events loop_pipeline"
    verilator --lint-only --Wall --top-module loop_pipeline -Ifpga/state_processor/rtl \
        -GLOOP_DEPTH="$loops" -GREQUEST_DEPTH="$requests" -GEVENT_DEPTH="$events" \
        "${sources[@]}" > "$output_dir/$name-lint.log" 2>&1
    yosys -Q -T -q -l "$output_dir/$name-generic.log" -p \
        "$read_command; $parameters; hierarchy -check -top loop_pipeline; proc; opt; memory -nomap; opt; check -assert; select -assert-none t:\$dlatch t:\$adlatch t:\$div t:\$mod; stat"
    printf '%s: connected pipeline lint and generic synthesis passed\n' "$name"
done
yosys -Q -T -q -l "$output_dir/default-ecp5.log" -p \
    "$read_command; synth_ecp5 -top loop_pipeline -json \"$output_dir/default-ecp5.json\"; check -assert; stat"
python3 - "$output_dir" <<'PY'
from collections import Counter
import json
from pathlib import Path
import sys
output=Path(sys.argv[1])
netlist=json.loads((output/'default-ecp5.json').read_text())
cells=Counter(cell['type'] for cell in netlist['modules']['loop_pipeline']['cells'].values())
(output/'resources.json').write_text(json.dumps({'loops':8,'requests':16,'events':32,
    'program':64,'state':32,'jobs':64,'frames':32,'buffers':16,'input_ports':2,'output_ports':2,
    'views':32,'slots':32,'chunk_frames':32,'memory_response_cycles':1,
    'cells':dict(sorted(cells.items())),'timing_checked':False},indent=2)+'\n')
print('Connected ECP5 mapping:',dict(sorted(cells.items())))
PY
