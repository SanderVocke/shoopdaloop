#!/usr/bin/env python3
"""Apply the existing independent float-accuracy gate to native replay artifacts."""
import argparse
import json
from pathlib import Path
from rt_backend_testbench.accuracy import compare


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directories', nargs='+', type=Path)
    args = parser.parse_args()
    count = 0
    for root in args.directories:
        for audit_file in sorted(root.rglob('audit.json')):
            directory = audit_file.parent
            files = [directory/f'{name}.json' for name in ('scenario', 'image', 'expected', 'float', 'audit')]
            if not all(path.is_file() for path in files):
                continue
            result = compare(*(json.loads(path.read_text()) for path in files))
            (directory/'float-accuracy.json').write_text(json.dumps(result, indent=2)+'\n')
            count += 1
    if not count:
        raise SystemExit('no complete native numeric reference artifacts found')
    print(f'{count} native reference artifacts passed the independent float accuracy gate')


if __name__ == '__main__':
    main()
