#!/usr/bin/env python3
"""Resume a verified area screen in a new directory without resynthesizing."""

import argparse
import json
from pathlib import Path
import shutil

from area_results import digest, verify_manifest


def prepare(build, output, stage='route'):
    build = build.resolve()
    provenance = json.loads((build/'provenance.json').read_text())
    outcome = dict(line.split('=', 1) for line in (build/'outcome.txt').read_text().splitlines()
                   if '=' in line)
    stages = ['synth', 'opt', 'place', 'route']
    previous = outcome['stage']
    if outcome['status'] != '0' or previous not in stages[:-1] or stage not in stages:
        raise ValueError('A successful synthesis, optimization, or placement checkpoint is required')
    if stages.index(stage) <= stages.index(previous):
        raise ValueError('Resume must advance the implementation stage')
    parents = {name: verify_manifest(build, name, prefix) for name, prefix in
               [('source-manifest.json', 'source'), ('artifact-manifest.json', '')]}
    checkpoint = build/f'post_{previous}.dcp'
    checkpoint_hash = digest(checkpoint)
    template = Path(__file__).with_name('midi_shared_jobs.tcl').read_text()
    parent_template = (build/'run.tcl').read_text()
    while parent_template.startswith(('set stop_after ', 'set resume_after ')):
        parent_template = parent_template.split('\n', 1)[1]
    if parent_template != template:
        raise ValueError('Implementation flow changed; prepare a fresh screen')
    output.mkdir(parents=True, exist_ok=True)
    if any(output.iterdir()):
        raise ValueError('Use an empty output directory to preserve evidence')
    shutil.copytree(build/'source', output/'source')
    for name in ['context.sv', 'context.xdc', 'compiled-profile.json',
                 'source-manifest.json', 'run-container.sh']:
        shutil.copyfile(build/name, output/name)
    for pattern in ['post_*.rpt', 'post_*.json']:
        for path in build.glob(pattern):
            shutil.copyfile(path, output/path.name)
    shutil.copyfile(checkpoint, output/'input.dcp')
    if digest(output/'input.dcp') != checkpoint_hash:
        raise ValueError('Checkpoint changed during preparation')
    (output/'run.tcl').write_text(
        f'set stop_after {stage}\nset resume_after {previous}\n'+template)
    provenance['stage'] = stage
    provenance['resumed_from'] = {
        'directory': str(build), 'stage': previous,
        'checkpoint_sha256': checkpoint_hash, 'manifests': parents,
        'provenance_sha256': digest(build/'provenance.json'),
        'run_tcl_sha256': digest(build/'run.tcl'),
    }
    (output/'provenance.json').write_text(json.dumps(provenance, indent=2)+'\n')
    artifacts = ['context.sv', 'context.xdc', 'run.tcl', 'run-container.sh',
                 'compiled-profile.json', 'input.dcp']
    (output/'artifact-manifest.json').write_text(json.dumps(
        {name: digest(output/name) for name in artifacts}, indent=2)+'\n')
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--build', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--stage', choices=['opt', 'place', 'route'], default='route')
    args = parser.parse_args()
    print(prepare(args.build, args.output.resolve(), args.stage))


if __name__ == '__main__':
    main()
