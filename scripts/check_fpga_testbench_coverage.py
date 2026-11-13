#!/usr/bin/env python3
"""Guard the complete historical coverage mapping and native simulator boundary."""
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]


def main():
    inventory = json.loads((ROOT/'src/rust/rt_fpga_testbench/coverage.json').read_text())
    rows = inventory['rows']
    assert len(rows) == inventory['original_definitions'] == 75
    assert len({(row['original_source'], row['original_test']) for row in rows}) == 75
    for row in rows:
        source = (ROOT/row['rust_source']).read_text()
        assert re.search(r'\bfn\s+'+re.escape(row['rust_function'])+r'\s*\(', source), row
        target = ROOT/f"src/rust/rt_fpga_testbench/tests/{row['cargo_test']}.rs"
        assert '#[shoop_test]' in target.read_text(), row
        assert re.fullmatch('[0-9a-f]{64}', row['original_definition_sha256']), row
    for path in (ROOT/'fpga').rglob('*.py'):
        assert 'tests' not in path.relative_to(ROOT/'fpga').parts, path
        source = path.read_text()
        assert not re.search(r'^\s*async\s+def\b', source, re.M), path
        if path.name.startswith('simulate') or path.name.endswith('_simulate.py'):
            assert 'from run_fpga_testbench import launch' in source, path
    for path in (ROOT/'fpga').rglob('requirements*.txt'):
        lines = [line.strip() for line in path.read_text().splitlines()
                 if line.strip() and not line.lstrip().startswith('#')]
        assert lines == ['-r ../../scripts/rt_backend_testbench/requirements.txt'], path
    assert not (ROOT/'src/rust/rt_fpga_sim/tests/bench').exists(), 'duplicate pilot harness'
    print('Marlin coverage: all 75 baseline definitions mapped; FPGA Python tools are preparation/launchers only')


if __name__ == '__main__':
    main()
