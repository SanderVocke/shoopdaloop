#!/usr/bin/env python3
"""USB discovery/recovery across actual U-Boot SD entry, network updates and rollback."""
import argparse
import contextlib
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / 'deployment'))
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from deployment.build_artifacts import build
from deployment.emulator import BootQemu, ROOT
from deployment.qualification import Qualification, boots, snapshot
from deployment.suite import audit_public_output
from usb.probe import attach, usb_emulator


def resets(peer, path):
    peer.settimeout(5)
    output = bytearray()
    while len(output) < 128:
        block = peer.recv(4096)
        if not block:
            raise RuntimeError('USB fixture closed before reset delivery')
        output.extend(block)
    path.write_bytes(output)
    expected = b''.join(bytes([11, 0xb0 | channel, cc, 0]) for channel in range(16) for cc in [120, 123])
    assert bytes(output) == expected, 'Updater entered without exact USB reset delivery'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--directory', type=Path, default=ROOT / 'work/zynq/usb-deployment')
    parser.add_argument('--fetch', action='store_true')
    args = parser.parse_args()
    directory = args.directory.resolve()
    directory.mkdir(parents=True, exist_ok=False)
    evidence = directory / 'evidence'
    evidence.mkdir()
    result = {'passed': False, 'hardware_qualification': False, 'cases': []}
    try:
        build(directory, args.fetch, 30400, usb=True)
        emulator, manifest = usb_emulator()
        q = Qualification(directory / 'artifacts', directory / 'private', evidence, 30400, 30554)

        @contextlib.contextmanager
        def live(image, label):
            run = evidence / label
            with BootQemu(q.loader, image, run, port_base=q.port_base, updater_port=q.updater_port, qemu=emulator) as vm:
                vm.qmp('stop')
                peer = attach(vm, run)
                vm.qmp('cont')
                try:
                    yield vm, peer
                finally:
                    peer.close()

        image = q.factory('usb-roundtrip')
        before = snapshot(image)
        with live(image, 'roundtrip') as (vm, peer):
            vm.ready('SHOOP_HOST_READY', 150)
            vm.ready('SHOOP_USB_MIDI_MOUNT', 30)
            q.command('usb-status', 'status')
            for identity in ['firmware-b', 'firmware-a']:
                output = q.command('usb-deploy-' + identity, 'deploy', q.artifacts / (identity + '.itb'))
                assert 'confirmed build=' + identity in output
                resets(peer, evidence / (identity + '-reset.bin'))
            text = vm.serial.read_text()
            assert 'SHOOP_USB_FAILED' not in text, 'Controller handshake failed across update'
            images = boots(text)
            assert [entry['build'] for entry in images] == ['firmware-a', 'firmware-b', 'firmware-a']
            assert text.count('SHOOP_USB_MIDI_MOUNT') == 3
            assert text.count('SHOOP_USB_STOP drained=1 controller_stopped=1 lost_packets=0 errors=0') == 2
            assert text.count('SHOOP_HOST_CLEANUP_COMPLETE') == 2
            result['cases'].append({'name': 'usb-network-update-roundtrip', 'passed': True, 'boots': images})
        with live(image, 'cold') as (vm, peer):
            vm.ready('SHOOP_HOST_READY', 150)
            vm.ready('SHOOP_USB_MIDI_MOUNT', 30)
            assert 'SHOOP_USB_FAILED' not in vm.serial.read_text()
            cold = boots(vm.serial.read_text())[-1]
            assert cold['build'] == 'firmware-a' and cold['nonce'] > images[-1]['nonce']
            result['cases'].append({'name': 'usb-cold-entry-after-update', 'passed': True, 'boot': cold})
        q.protected(image, before, 'usb-roundtrip')

        image = q.factory('usb-rollback', mode=1)
        before = snapshot(image)
        with live(image, 'rollback') as (vm, peer):
            vm.ready('Listening for fastboot', 60)
            q.command('usb-rollback-stage', 'stage', q.artifacts / 'fault-before-ready.itb', '--reboot')
            position = vm.ready('SHOOP_BUILD firmware-a', 300)
            vm.ready('SHOOP_HOST_READY', 150, after=position)
            vm.ready('SHOOP_USB_MIDI_MOUNT', 30, after=position)
            vm.qmp('query-status')
            assert 'SHOOP_USB_FAILED' not in vm.serial.read_text()
            images = boots(vm.serial.read_text())
            assert len([event for event in vm.events if event['event'] == 'WATCHDOG']) == 2
            assert images[-1]['build'] == 'firmware-a'
            result['cases'].append({'name': 'usb-watchdog-rollback', 'passed': True, 'boots': images})
        q.protected(image, before, 'usb-rollback')
        result.update(passed=True, emulator=manifest)
    finally:
        rejected = audit_public_output(directory)
        result['public_output_audit'] = {'passed': not rejected, 'quarantined_files': rejected}
        result['passed'] &= not rejected
        (evidence / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    if not result['passed']:
        raise RuntimeError('USB deployment qualification or public-output audit failed')
    print('USB U-Boot entry, update reset delivery, cold discovery and rollback passed')


if __name__ == '__main__':
    main()
