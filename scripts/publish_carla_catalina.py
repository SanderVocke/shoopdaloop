#!/usr/bin/env python3
"""Publish only a successful standalone Carla build and its validation metadata."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import zipfile

from check_macos_compatibility import verify_payloads

ROOT = Path(__file__).resolve().parents[1]
FILES = ['Carla-2.5.10-macos-x86_64-10.15-standalone-ab.zip', 'SHA256SUMS',
         'BUILD-INFO.json', 'standalone-probe.json', 'compatibility.txt']
TAG = 'carla-catalina-ab-2.5.10-20261028'


def api(repo, suffix):
    return json.loads(subprocess.check_output(['gh', 'api', f'repos/{repo}/{suffix}'], text=True))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run-id', type=int, required=True)
    parser.add_argument('--repo', default=os.environ.get('GITHUB_REPOSITORY'), required=not os.environ.get('GITHUB_REPOSITORY'))
    parser.add_argument('--output', type=Path, default=Path('publication'))
    args = parser.parse_args()
    run = api(args.repo, f'actions/runs/{args.run_id}')
    if (run['name'] != 'Carla Catalina standalone A/B package' or run['conclusion'] != 'success'
            or run['head_repository']['full_name'] != args.repo):
        raise RuntimeError('Publication requires a successful standalone build from this repository')
    args.output.mkdir(parents=True, exist_ok=False)
    subprocess.run(['gh', 'run', 'download', str(args.run_id), '--repo', args.repo,
                    '--name', f'carla-catalina-standalone-{args.run_id}', '--dir', str(args.output)], check=True)
    checksums = (args.output / 'SHA256SUMS').read_text().splitlines()
    if len(checksums) != 1:
        raise RuntimeError('Expected checksum for only the Carla package')
    expected, name = checksums[0].split()
    if name != FILES[0] or hashlib.sha256((args.output / name).read_bytes()).hexdigest() != expected:
        raise RuntimeError('Package checksum mismatch')
    info = json.loads((args.output / 'BUILD-INFO.json').read_text())
    if (info['build_commit'] != run['head_sha'] or not info['native_sections_match_bundled_runtime']
            or info['architecture'] != 'x86_64' or info['minimum_macos'] != '10.15'):
        raise RuntimeError('Package provenance mismatch')
    probe = info['validation']['standalone_processing']
    if probe['sample_rate'] != 48000 or probe['frames'] != 1024 or probe['source_peak'] <= 0:
        raise RuntimeError('Standalone processing validation is incomplete')
    with zipfile.ZipFile(args.output / FILES[0]) as archive:
        payloads = {name.removeprefix('Carla-Catalina-AB/'): archive.read(name) for name in archive.namelist()
                    if name.startswith('Carla-Catalina-AB/') and not name.endswith('/')}
        for path, expected in info['files'].items():
            if hashlib.sha256(payloads[path]).hexdigest() != expected:
                raise RuntimeError(f'Packaged file checksum mismatch: {path}')
        for app in ['Carla.app', 'Carla-Control.app']:
            verify_payloads({path: data for path, data in payloads.items() if path.startswith(app + '/')}, 'x86_64', '10.15')
    subprocess.run(['gh', 'release', 'create', TAG, *[str(args.output / name) for name in FILES],
                    '--repo', args.repo, '--target', info['build_commit'], '--prerelease',
                    '--title', 'Carla 2.5.10 — Catalina standalone A/B package',
                    '--notes-file', str(ROOT / 'docs/carla-catalina-ab.md')], check=True)


if __name__ == '__main__':
    main()
