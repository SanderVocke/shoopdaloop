#!/usr/bin/env python3
"""Collect reproducible area and cycle evidence without mixing report stages."""

import argparse
import hashlib
import json
import re
import statistics
from pathlib import Path


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def resources(path):
    report = path.read_text()
    labels = {
        'lut': 'Slice LUTs', 'logic_lut': 'LUT as Logic',
        'memory_lut': 'LUT as Memory', 'ff': 'Slice Registers',
        'bram36': 'Block RAM Tile', 'dsp': 'DSPs', 'carry4': 'CARRY4',
        'muxf7': 'F7 Muxes', 'muxf8': 'F8 Muxes',
    }
    result = {}
    for name, label in labels.items():
        match = re.search(r'^\|\s*'+re.escape(label)+r'\*?\s*\|\s*([\d.]+)', report, re.M)
        if not match:
            raise ValueError(f'Missing {label} in {path}')
        value = float(match[1])
        result[name] = int(value) if value.is_integer() else value
    match = re.search(r'^\|\s*Slice\s*\|\s*(\d+)', report, re.M)
    result['slices'] = int(match[1]) if match else None
    return result


def verify_manifest(build, name, prefix):
    manifest = json.loads((build/name).read_text())
    if prefix == 'source':
        actual = {str(path.relative_to(build/prefix)) for path in (build/prefix).rglob('*')
                  if path.suffix in ('.sv', '.svh')}
        if actual != set(manifest):
            raise ValueError('Source manifest file set mismatch')
    for relative, expected in manifest.items():
        path = (build/prefix/relative).resolve()
        if not path.is_relative_to(build.resolve()) or digest(path) != expected:
            raise ValueError(f'Manifest mismatch: {relative}')
    return digest(build/name)


def benchmark(path, expected_sources=None, expected_parameters=None):
    data = json.loads(path.read_text())
    differences = {}
    if expected_sources is not None:
        models = data.get('provenance', {}).get('models', [])
        if not models:
            raise ValueError(f'Missing model provenance: {path}')
        for model in models:
            for name, value in model['sources_sha256'].items():
                if name.startswith('fpga/') and expected_sources.get(name) != value:
                    raise ValueError(f'Benchmark source mismatch: {name}')
            for name, value in model['parameters'].items():
                if name in expected_parameters and str(expected_parameters[name]) != value:
                    differences[name] = {'implementation': expected_parameters[name], 'simulation': value}
    groups = {}
    for sample in data['measurements']:
        groups.setdefault(sample['kind'], []).append(sample['cycles'])
    return {
        'file': str(path), 'sha256': digest(path),
        'scenario': data['scenario'], 'validation': data['validation'],
        'scope': data['scope'], 'loops': data['loops'],
        'quantum_frames': data['quantum_frames'], 'sample_rate': data['sample_rate'],
        'ddr_added_delay_clocks': data['ddr_added_delay_clocks'],
        'midi_state_tracking': data['midi_state_tracking'],
        'implementation_parameter_differences': differences,
        'cycles': {kind: {'count': len(values), 'min': min(values),
                          'median': statistics.median(values), 'max': max(values)}
                   for kind, values in groups.items()},
    }


def collect(build, label, benchmarks=()):
    provenance = json.loads((build/'provenance.json').read_text())
    hashes = {name: verify_manifest(build, name, prefix) for name, prefix in
              [('source-manifest.json', 'source'), ('artifact-manifest.json', '')]}
    sources = json.loads((build/'source-manifest.json').read_text())
    outcome = dict(line.split('=', 1) for line in (build/'outcome.txt').read_text().splitlines()
                   if '=' in line)
    result = {'schema_version': 1, 'label': label, 'provenance': provenance,
              'build_directory': str(build.resolve()), 'manifests': hashes,
              'outcome': outcome, 'stages': {},
              'benchmarks': [benchmark(path, sources, provenance['parameters']) for path in benchmarks]}
    evidence = ['provenance.json', 'source-manifest.json', 'artifact-manifest.json', 'outcome.txt']
    for stage in ('synth', 'opt', 'place', 'route'):
        prefix = f'post_{stage}'
        utilization = build/f'{prefix}_utilization.rpt'
        if not utilization.exists():
            continue
        row = resources(utilization)
        timing = build/f'{prefix}.json'
        controls = build/f'{prefix}_control_sets.rpt'
        row['timing'] = json.loads(timing.read_text()) if timing.exists() else None
        row['control_sets'] = None
        if controls.exists():
            match = re.search(r'\| Total control sets\s+\|\s*(\d+)', controls.read_text())
            if not match:
                raise ValueError(f'Missing control set count: {controls}')
            row['control_sets'] = int(match[1])
        row['timing_met'] = bool(stage == 'route' and outcome.get('status') == '0'
                                 and outcome.get('stage') == 'route' and row['timing']
                                 and row['timing']['wns_ns'] >= 0
                                 and row['timing']['whs_ns'] >= 0)
        result['stages'][stage] = row
        evidence.extend(p.name for p in build.glob(f'{prefix}*')
                        if p.suffix in ('.rpt', '.json'))
    result['evidence_sha256'] = {name: digest(build/name) for name in sorted(set(evidence))}
    return result


def compare(result, control):
    a, b = result['provenance'], control['provenance']
    for field in ('part', 'clock_mhz', 'uncertainty_ns', 'vivado'):
        if a[field] != b[field]:
            raise ValueError(f'Comparison differs in {field}')
    return {'label': control['label'], 'revision': b['revision'],
            'parameter_changes': {key: {'control': b['parameters'].get(key), 'candidate': value}
                                  for key, value in a['parameters'].items()
                                  if value != b['parameters'].get(key)},
            'resource_deltas': {
                stage: {key: value-control['stages'][stage][key]
                        for key, value in row.items()
                        if isinstance(value, (int, float)) and not isinstance(value, bool)
                        and isinstance(control['stages'][stage].get(key), (int, float))}
                for stage, row in result['stages'].items() if stage in control['stages']}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--build', type=Path, required=True)
    parser.add_argument('--label', required=True)
    parser.add_argument('--benchmark', action='append', default=[], type=Path)
    parser.add_argument('--control', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists():
        parser.error('Use a new output path to preserve evidence')
    result = collect(args.build, args.label, args.benchmark)
    if args.control:
        result['comparison'] = compare(result, json.loads(args.control.read_text()))
    args.output.write_text(json.dumps(result, indent=2)+'\n')


if __name__ == '__main__':
    main()
