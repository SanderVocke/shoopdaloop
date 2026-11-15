#!/usr/bin/env bash
set -euo pipefail
repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$repo_root"
out=${1:-work/metadata-capacity}
mkdir -p "$out"
for slots in 4096 5760000; do
  sources="fpga/chunk_memory/rtl/chunk_ram.sv fpga/media_store/rtl/media_ram.sv fpga/media_store/rtl/metadata_cache.sv fpga/media_store/rtl/media_metadata.sv"
  command="read_verilog -sv $sources; chparam -set VIEWS 128 -set SLOTS $slots media_metadata; hierarchy -check -top media_metadata; proc; opt; memory -nomap; opt; check -assert; select -assert-none t:\$dlatch t:\$adlatch t:\$div t:\$mod; stat; write_json \"$out/$slots-generic.json\""
  yosys -Q -T -q -l "$out/$slots-generic.log" -p "$command"
  yosys -Q -T -q -l "$out/$slots-ecp5.log" -p "read_verilog -sv $sources; chparam -set VIEWS 128 -set SLOTS $slots media_metadata; synth_ecp5 -top media_metadata -json \"$out/$slots-ecp5.json\"; check -assert; stat"
done
python3 - "$out" <<'PY'
from collections import Counter
import json
from pathlib import Path
import subprocess
import sys

output = Path(sys.argv[1])
rows = []
for slots in (4096, 5760000):
    generic = json.loads((output/f'{slots}-generic.json').read_text())['modules']
    memories = []
    def walk(name):
        for cell in generic[name]['cells'].values():
            kind = cell['type']
            if kind in generic:
                walk(kind)
            elif kind == '$mem_v2':
                p = cell['parameters']
                memories.append(int(p['WIDTH'], 2)*int(p['SIZE'], 2))
    walk('media_metadata')
    mapped = json.loads((output/f'{slots}-ecp5.json').read_text())['modules']
    cells = Counter(cell['type'] for module in mapped.values() for cell in module.get('cells', {}).values())
    rows.append(dict(views=128, slots=slots, chunk_frames=32, table_lines=8,
                     reference_lines=4, stack_lines=2, generic_memory_bits=sum(memories), cells=dict(cells)))
assert rows[0]['cells']['DP16KD'] == rows[1]['cells']['DP16KD'], 'recording capacity grew metadata BRAM'
(output/'resources-scaling.json').write_text(json.dumps(dict(
    revision=subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(), results=rows), indent=2)+'\n')
print('Metadata capacity scaling: generic/ECP5 passed; BRAM count is independent of slot capacity')
PY
