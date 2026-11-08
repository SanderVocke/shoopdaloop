#!/usr/bin/env python3
"""Zip benchmark evidence, excluding connection credentials and raw stack memory."""
import argparse
from pathlib import Path
import zipfile

SAFE = {
    'callbacks.csv', 'config.json', 'run.json', 'summary.json', 'results.json',
    'summary.csv', 'machine.json', 'scheduling.json', 'sampling.json',
    'final-state.json', 'post-window-state.json', 'categories.csv',
    'stacks.jsonl.gz', 'perf-report.txt', 'symbolization.log', 'perf.log',
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('results', type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    root = args.results.resolve()
    if not root.is_dir():
        parser.error('results must be an existing directory')
    paths = [p for p in sorted(root.rglob('*')) if p.is_file() and not p.is_symlink()
             and (p.name in SAFE or (p.name.startswith('media-') and p.suffix == '.json'))]
    if not any(p.name == 'callbacks.csv' for p in paths):
        parser.error('no callback evidence found')
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(args.output, 'x', compression=zipfile.ZIP_DEFLATED) as archive:
        for path in paths:
            archive.write(path, Path(root.name) / path.relative_to(root))
        manifest = Path(__file__).resolve().parent.parent / 'BUILD_INFO.json'
        if manifest.exists():
            archive.write(manifest, 'BUILD_INFO.json')
    print(f'{args.output}: {len(paths)} evidence files; credentials and raw perf data excluded')


if __name__ == '__main__':
    main()
