#!/usr/bin/env python3
"""Portable and actual-guest USB qualification, independently of physical hardware."""
import argparse
import json
import os
import signal
from pathlib import Path
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from suite import ROOT, interrupted, run_phase


def main():
    signal.signal(signal.SIGTERM, interrupted)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fetch', action='store_true')
    parser.add_argument('--directory', type=Path, default=ROOT / 'work/zynq/usb-suite')
    args = parser.parse_args()
    directory = args.directory.resolve()
    directory.mkdir(parents=True, exist_ok=True)
    phases = [
        ('model', 'deployment/build_qemu.py', ['--usb', *(['--fetch'] if args.fetch else [])]),
        ('direct-hs', 'usb/integration.py', ['--engine-config', str(ROOT / 'platforms/zynq7000/input-history.json'),
                                          *(['--fetch'] if args.fetch else [])]),
        ('portable', 'usb/tests/run.py', []),
        ('sixteen-cables', 'usb/integration.py', ['--cables', '16']),
        ('direct-fs', 'usb/integration.py', ['--full-speed']),
        ('nested-hubs', 'usb/integration.py', ['--full-speed', '--tiers', '2', '--cables', '16']),
        ('simultaneous-devices', 'usb/integration.py', ['--devices', '4', '--cables', '4', '--tiers', '1']),
        ('hotplug-soak', 'usb/integration.py', ['--hotplug-cycles', '100']),
        ('content-canary', 'usb/integration.py', ['--corrupt']),
        ('endpoint-stall', 'usb/integration.py', ['--fault', 'stall']),
        ('shutdown-timeout', 'usb/integration.py', ['--fault', 'nak']),
        ('deployment', 'usb/deployment_gate.py', ['--directory', str(directory / 'deployment'),
                                                *(['--fetch'] if args.fetch else [])]),
    ]
    results = []
    try:
        for name, script, arguments in phases:
            print(f'Running USB {name}', flush=True)
            start = time.monotonic()
            result = {'name': name, 'passed': False}
            results.append(result)
            log_path = directory / (name + '.log')
            with log_path.open('w') as log:
                case_directory = ['--directory', str(directory / name)] if script == 'usb/integration.py' else []
                code = run_phase([sys.executable, str(ROOT / 'platforms/zynq7000' / script), *arguments, *case_directory], dict(os.environ), log)
            result.update(passed=code == 0, seconds=round(time.monotonic()-start, 2))
            if code:
                raise RuntimeError(log_path.read_text()[-10000:])
    finally:
        (directory / 'result.json').write_text(json.dumps({'passed': len(results) == len(phases) and all(r['passed'] for r in results),
            'hardware_qualification': False, 'phases': results}, indent=2) + '\n')


if __name__ == '__main__':
    main()
