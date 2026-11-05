#!/usr/bin/env python3
"""Portable MIDI golden vectors and actual TinyUSB with a deterministic fake HCD."""
import errno
import hashlib
import json
import os
import shutil
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[4]
USB = ROOT / 'platforms/zynq7000/usb'
SOURCE = ROOT / 'work/zynq/tinyusb'
BUILD = ROOT / 'work/zynq/usb-portable'


def main():
    pins = json.loads((USB.parent / 'pins.json').read_text())
    revision = subprocess.check_output(['git', '-C', SOURCE, 'rev-parse', 'HEAD'], text=True).strip()
    assert revision == pins['tinyusb'], 'Fetch the pinned TinyUSB using build.py --fetch --usb'
    assert not subprocess.check_output(['git', '-C', SOURCE, 'status', '--porcelain'], text=True).strip()
    BUILD.mkdir(parents=True, exist_ok=True)
    result_path = BUILD / 'result.json'
    result_path.write_text(json.dumps({'passed': False}) + '\n')
    midi = BUILD / 'midi_host.c'
    midi.write_bytes((SOURCE / 'src/class/midi/midi_host.c').read_bytes())
    patch = USB.parent / 'patches/tinyusb-midi-error.patch'
    with patch.open('rb') as stream:
        subprocess.run(['patch', '--batch', str(midi)], stdin=stream, check=True)
    core = BUILD / 'usbh.c'
    core.write_bytes((SOURCE / 'src/host/usbh.c').read_bytes())
    with (USB.parent / 'patches/tinyusb-enumeration-bounds.patch').open('rb') as stream:
        subprocess.run(['patch', '--batch', str(core)], stdin=stream, check=True)
    hub = BUILD / 'hub.c'
    hub.write_bytes((SOURCE / 'src/host/hub.c').read_bytes())
    with (USB.parent / 'patches/tinyusb-hub-status.patch').open('rb') as stream:
        subprocess.run(['patch', '--batch', str(hub)], stdin=stream, check=True)
    ehci = BUILD / 'ehci.c'
    ehci.write_bytes((SOURCE / 'src/portable/ehci/ehci.c').read_bytes())
    with (USB.parent / 'patches/tinyusb-zynq-ehci.patch').open('rb') as stream:
        subprocess.run(['patch', '--batch', str(ehci)], stdin=stream, check=True)
    common = ['gcc' , '-std=c11', '-Wall', '-Wextra', '-Werror', '-Wno-unused-parameter',
              '-fsanitize=address,undefined', '-g']
    cases = {
        'ehci': ['-I' + str(USB / 'tests/fake'), '-I' + str(SOURCE / 'src'),
                 '-I' + str(BUILD), '-I' + str(SOURCE / 'src/portable/ehci'), '-m32',
                 '-ffunction-sections', '-fdata-sections', '-Wl,--gc-sections',
                 '-Wno-pointer-to-int-cast', '-Wno-int-to-pointer-cast', USB / 'tests/ehci_queue.c'],
        'midi': ['-I' + str(USB), USB / 'midi.c', USB / 'tests/midi.c'],
        'descriptors': ['-I' + str(USB), USB / 'descriptors.c', USB / 'tests/descriptors.c'],
        'bridge': ['-I' + str(USB), USB / 'midi.c', USB / 'bridge.c', USB / 'tests/bridge.c'],
        'fake-hcd': ['-I' + str(USB / 'tests/fake'), '-I' + str(SOURCE / 'src'),
                     '-I' + str(SOURCE / 'src/class/midi'), '-I' + str(SOURCE / 'src/host'),
                     *[SOURCE / 'src' / file for file in ['tusb.c', 'common/tusb_fifo.c']],
                     core, midi, hub, USB / 'descriptors.c', '-I' + str(USB), USB / 'tests/fake/host.c'],
    }
    results = {}
    executors = {}
    for name, files in cases.items():
        binary = BUILD / name
        compiled = subprocess.run([*common, *map(str, files), '-o', str(binary)], capture_output=True, text=True)
        (BUILD / (name + '-build.log')).write_text(compiled.stdout + compiled.stderr)
        if compiled.returncode:
            raise RuntimeError(compiled.stderr[-12000:])
        environment = dict(os.environ, ASAN_OPTIONS='detect_leaks=0', UBSAN_OPTIONS='halt_on_error=1')
        command = [binary]
        try:
            result = subprocess.run(command, env=environment, capture_output=True, text=True, timeout=30)
        except OSError as error:
            if error.errno != errno.ENOEXEC or name != 'ehci' or not shutil.which('qemu-i386'):
                raise
            command = ['qemu-i386', binary]
            result = subprocess.run(command, env=environment, capture_output=True, text=True, timeout=30)
        executors[name] = 'qemu-i386' if len(command) > 1 else 'native'
        (BUILD / (name + '.log')).write_text(result.stdout + result.stderr)
        if result.returncode:
            raise RuntimeError(result.stdout + result.stderr)
        results[name] = result.stdout.strip()
        print(result.stdout, end='')
    result_path.write_text(json.dumps({'passed': True, 'tinyusb': revision,
        'patch_sha256': {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
            for p in sorted((USB.parent / 'patches').glob('tinyusb-*.patch'))}, 'cases': results, 'executors': executors}, indent=2) + '\n')


if __name__ == '__main__':
    main()
