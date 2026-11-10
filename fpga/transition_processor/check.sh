#!/usr/bin/env bash
set -euo pipefail
repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
output_dir=${1:-"$repo_root/work/fpga-transition-synthesis"}
mkdir -p "$output_dir"
output_dir=$(cd -- "$output_dir" && pwd)
cd "$repo_root"
sources=(fpga/transition_processor/rtl/tp_ram.sv fpga/transition_processor/rtl/transition_processor.sv)
read_command="read_verilog -sv ${sources[*]}"
yosys -V > "$output_dir/versions.txt"
verilator --version >> "$output_dir/versions.txt"
for configuration in default:64:64:192 small:7:11:25 minimum:1:1:3; do
    IFS=: read -r name loops requests events <<< "$configuration"
    parameters="chparam -set LOOP_DEPTH $loops -set REQUEST_DEPTH $requests -set EVENT_DEPTH $events transition_processor"
    if ! verilator --lint-only --Wall --top-module transition_processor \
        -GLOOP_DEPTH="$loops" -GREQUEST_DEPTH="$requests" -GEVENT_DEPTH="$events" \
        "${sources[@]}" > "$output_dir/$name-lint.log" 2>&1; then
        cat "$output_dir/$name-lint.log"
        exit 1
    fi
    yosys -Q -T -q -l "$output_dir/$name-generic.log" -p \
        "$read_command; $parameters; hierarchy -check -top transition_processor; proc; opt; memory -nomap; opt; check -assert; select -assert-none t:\$dlatch t:\$adlatch t:\$mul t:\$div t:\$mod; stat"
    printf '%s: lint and generic synthesis checks passed\n' "$name"
done
yosys -Q -T -q -l "$output_dir/default-ecp5.log" -p \
    "$read_command; synth_ecp5 -top transition_processor -json \"$output_dir/default-ecp5.json\"; check -assert; stat"
python3 - "$output_dir" <<'PY'
from collections import Counter
import json
from pathlib import Path
import sys
output = Path(sys.argv[1])
netlist = json.loads((output / 'default-ecp5.json').read_text())
cells = Counter(cell['type'] for cell in netlist['modules']['transition_processor']['cells'].values())
(output / 'resources.json').write_text(json.dumps({
    'loop_depth': 64, 'request_depth': 64, 'event_depth': 192,
    'cells': dict(sorted(cells.items())), 'timing_checked': False,
}, indent=2) + '\n')
print('ECP5 mapping:', dict(sorted(cells.items())))
PY
printf 'Logs and netlist: %s\n' "$output_dir"
