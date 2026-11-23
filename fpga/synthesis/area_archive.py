#!/usr/bin/env python3
"""Collect a candidate and retain compact, independently verifiable raw evidence."""

import argparse
import gzip
import hashlib
import io
import json
from pathlib import Path
import re
import tarfile

from area_results import collect, compare, digest


def archive(build, label, output, benchmarks=(), control=None):
    if not re.fullmatch(r'[a-zA-Z0-9_-]+', label):
        raise ValueError('Use a simple candidate label')
    summary = collect(build, label, benchmarks)
    if control:
        summary['comparison'] = compare(summary, json.loads(control.read_text()))
    output.mkdir(parents=True, exist_ok=True)
    paths = {suffix: output/f'{label}{suffix}' for suffix in
             ['-summary.json', '-raw.tar.gz', '-retention.json']}
    if any(path.exists() for path in paths.values()):
        raise ValueError('Use new output paths to preserve evidence')
    members = {path.name: path for path in build.iterdir() if path.is_file()
               and (path.suffix in ('.rpt', '.json', '.tcl', '.sv', '.xdc', '.sh')
                    or path.name in ('setup.txt', 'outcome.txt', 'console.log'))}
    members.update({str(path.relative_to(build)): path for path in (build/'source').rglob('*')
                    if path.is_file()})
    for index, path in enumerate(benchmarks):
        members[f'benchmarks/{index}-{path.name}'] = path
    hashes = {name: digest(path) for name, path in sorted(members.items())}
    with paths['-raw.tar.gz'].open('xb') as raw:
        with gzip.GzipFile(fileobj=raw, filename='', mode='wb', mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode='w') as tar:
                for name, path in sorted(members.items()):
                    data = path.read_bytes()
                    if hashlib.sha256(data).hexdigest() != hashes[name]:
                        raise ValueError(f'Evidence changed during retention: {name}')
                    info = tarfile.TarInfo(name)
                    info.size = len(data)
                    info.mode = 0o644
                    tar.addfile(info, io.BytesIO(data))
    retention = {
        'archive': paths['-raw.tar.gz'].name,
        'archive_sha256': digest(paths['-raw.tar.gz']),
        'members_sha256': hashes,
        'local_checkpoints': {str(path.resolve()): {'sha256': digest(path), 'bytes': path.stat().st_size}
                              for path in sorted(build.glob('*.dcp'))},
        'checkpoint_scope': 'Checkpoints remain in the local build directory; the compressed archive contains source, scripts, reports, and callback samples.',
    }
    summary['retained_evidence'] = paths['-retention.json'].name
    paths['-retention.json'].write_text(json.dumps(retention, indent=2)+'\n')
    paths['-summary.json'].write_text(json.dumps(summary, indent=2)+'\n')
    return paths['-summary.json']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--build', type=Path, required=True)
    parser.add_argument('--label', required=True)
    parser.add_argument('--output-directory', type=Path, required=True)
    parser.add_argument('--benchmark', type=Path, action='append', default=[])
    parser.add_argument('--control', type=Path)
    args = parser.parse_args()
    print(archive(args.build, args.label, args.output_directory, args.benchmark, args.control))


if __name__ == '__main__':
    main()
