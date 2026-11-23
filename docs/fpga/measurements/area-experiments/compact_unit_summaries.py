"""Rebuild compact standalone case statistics from retained per-request samples."""
import hashlib
import json
from pathlib import Path
import statistics
import tarfile

HERE = Path(__file__).resolve().parent

for summary_path in sorted(HERE.glob('*-summary.json')):
    summary = json.loads(summary_path.read_text())
    cases = summary.get('cases')
    if not isinstance(cases, list) or not cases or not any('measurements' in c or 'measurement_count' in c for c in cases):
        continue
    retention = json.loads((HERE/summary['retention']).read_text())
    archive = HERE/retention['archive']
    assert hashlib.sha256(archive.read_bytes()).hexdigest() == retention['archive_sha256']
    with tarfile.open(archive) as tar:
        samples = [m for m in tar.getmembers() if m.name.startswith('samples/') and m.name.endswith('.json')]
        assert len(samples) == 1
        raw = tar.extractfile(samples[0]).read()
        assert hashlib.sha256(raw).hexdigest() == retention['members_sha256'][samples[0].name]
        source = json.loads(raw)
    compact = []
    for case in source['cases']:
        rows = case['measurements']
        result = {k: v for k, v in case.items() if k != 'measurements'}
        result['measurement_count'] = len(rows)
        result['populations'] = {}
        for op, error in sorted({(r.get('op', r.get('opcode')), r['error']) for r in rows}):
            group = [r for r in rows if r.get('op', r.get('opcode')) == op and r['error'] == error]
            cycles = [r['cycles'] for r in group]
            result['populations'][f'opcode{op}/expected_error{error}'] = dict(
                count=len(group), min=min(cycles), median=statistics.median(cycles), max=max(cycles))
            if 'memory_transactions' in group[0]:
                result['populations'][f'opcode{op}/expected_error{error}']['memory_transactions'] = sum(r['memory_transactions'] for r in group)
        compact.append(result)
    summary['cases'] = compact
    summary['per_request_samples'] = retention['archive']
    summary_path.write_text(json.dumps(summary, indent=2)+'\n')
    print(summary_path.name, sum(c['measurement_count'] for c in compact), 'retained rows')
