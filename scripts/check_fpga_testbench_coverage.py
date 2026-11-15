#!/usr/bin/env python3
"""Guard historical coverage dispositions and the native component boundary."""
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]


def main():
    inventory = json.loads((ROOT/'src/rust/rt_fpga_testbench/coverage.json').read_text())
    rows = inventory['rows']
    assert len(rows) == inventory['original_definitions'] == 75
    assert len({(row['original_source'], row['original_test']) for row in rows}) == 75
    for retired in ('fpga/transition_processor', 'fpga/loop_pipeline',
                    'src/rust/rt_fpga_adapter', 'src/rust/rt_fpga_sim',
                    'src/rust/shoop_rt_host_fpga_sim'):
        assert not (ROOT/retired).exists(), retired
    forbidden = ('rt_fpga_adapter', 'rt_fpga_sim', 'shoop_rt_host_fpga_sim')
    for manifest in (ROOT/'src/rust').glob('*/Cargo.toml'):
        assert not any(name in manifest.read_text() for name in forbidden), manifest
    for source in (ROOT/'src/rust/rt_fpga_testbench/src').rglob('*.rs'):
        assert not any(name in source.read_text() for name in forbidden), source
    retained = 0
    for row in rows:
        assert re.fullmatch('[0-9a-f]{64}', row['original_definition_sha256']), row
        assert row['disposition'] in ('retained', 'retired'), row
        if row['disposition'] == 'retired':
            assert row['reason'], row
            assert not (ROOT/row['rust_source']).exists(), row
            continue
        retained += 1
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
    print(f'Marlin coverage: {retained} retained and {len(rows)-retained} explicitly retired baseline definitions')


if __name__ == '__main__':
    main()
