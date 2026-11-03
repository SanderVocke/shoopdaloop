#!/usr/bin/env python3
"""Exact MIDI through actual FreeRTOS USB transfers, engine recording and playback."""
import argparse
import json
import os
from pathlib import Path
import re
import socket
import subprocess
import sys
import threading
import time

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from qemu import Qemu, ROOT
from remote_profile import write_profile
from probe import attach, usb0_bus, usb_emulator


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cables', type=int, choices=range(1, 17), default=1)
    parser.add_argument('--tiers', type=int, choices=[0, 1, 2], default=0)
    parser.add_argument('--full-speed', action='store_true')
    parser.add_argument('--port-base', type=int, default=28400)
    parser.add_argument('--devices', type=int, choices=[1, 4], default=1)
    parser.add_argument('--fetch', action='store_true')
    parser.add_argument('--hotplug-cycles', type=int, default=1)
    parser.add_argument('--fault', choices=['stall', 'nak'])
    parser.add_argument('--directory', type=Path)
    parser.add_argument('--corrupt', action='store_true', help='Require the independent content-mismatch assertion to fail')
    args = parser.parse_args()
    if args.devices * args.cables > 16 or (args.devices > 1 and not args.tiers):
        parser.error('Multiple devices require a hub, with at most 16 cables in aggregate')
    if not 1 <= args.hotplug_cycles <= 100:
        parser.error('Hotplug cycles must be in 1..100')
    directory = ROOT / 'work/zynq' / f'usb-integration-{args.cables}-{args.tiers}-{int(args.full_speed)}-{int(args.corrupt)}-{args.devices}{"-" + args.fault if args.fault else ""}'
    if args.hotplug_cycles > 1:
        directory = directory.with_name(directory.name + f'-c{args.hotplug_cycles}')
    if args.directory:
        directory = args.directory.resolve()
    directory.mkdir(parents=True, exist_ok=True)
    result_path = directory / 'result.json'
    result_path.write_text(json.dumps({'passed': False, 'state': 'running'}) + '\n')
    credential = directory / 'test.key'
    descriptor = os.open(credential, os.O_CREAT | os.O_TRUNC | os.O_WRONLY, 0o600)
    with os.fdopen(descriptor, 'wb') as stream:
        stream.write(os.urandom(32))
    credential.chmod(0o600)
    profile = directory / 'profile.json'
    write_profile(profile, credential, args.port_base)
    subprocess.run([sys.executable, 'platforms/zynq7000/build.py', '--app', 'host', '--usb',
                    '--credential-file', credential, *(['--fetch'] if args.fetch else [])], cwd=ROOT, check=True)
    subprocess.run(['cargo', 'build', '--locked', '-p', 'shoop_host', '--no-default-features',
                    '--example', 'zynq_probe'], cwd=ROOT, check=True)
    emulator, manifest = usb_emulator()
    symbols = subprocess.check_output(['arm-none-eabi-nm', '--defined-only', ROOT / 'work/zynq/build/host-usb.elf'], text=True)
    consumed = int(re.search(r'^([0-9a-fA-F]+) B shoop_usb_consumed$', symbols, re.MULTILINE)[1], 16)
    pending = int(re.search(r'^([0-9a-fA-F]+) B shoop_usb_pending_packets$', symbols, re.MULTILINE)[1], 16)
    environment = dict(os.environ, QEMU=str(emulator), SHOOP_ZYNQ_USB_SOCKET=str(directory / 'midi.sock'),
                       SHOOP_ZYNQ_USB_CABLES=str(args.cables), SHOOP_ZYNQ_USB_DEVICES=str(args.devices),
                       SHOOP_ZYNQ_USB_CYCLES=str(args.hotplug_cycles),
                       SHOOP_ZYNQ_USB_CONTROL=str(directory / 'control.sock'))
    if args.corrupt:
        environment['SHOOP_ZYNQ_USB_CORRUPT'] = '1'
    if args.fault:
        environment['SHOOP_ZYNQ_USB_FAULT'] = args.fault
    os.environ['QEMU'] = str(emulator)
    control_path = directory / 'control.sock'
    control_path.unlink(missing_ok=True)
    errors = []
    metrics = {}
    with Qemu(ROOT / 'work/zynq/build/host-usb.elf', directory,
              [(args.port_base, 15400), (args.port_base + 1, 15401)]) as vm:
        vm.ready('SHOOP_USB_READY')
        attach(vm, directory, args.cables, not args.full_speed, args.tiers, open_peer=False, fault=args.fault)
        vm.ready(f'rx={args.cables} tx={args.cables}')
        peers = []
        for index in range(1, args.devices):
            path = str(directory / f'midi-{index}.sock')
            vm.qmp('chardev-add', {'id': f'midi-{index}', 'backend': {'type': 'socket', 'data': {
                'addr': {'type': 'unix', 'data': {'path': path}}, 'server': True, 'wait': False, 'logfile': str(directory / f'midi-{index}-chardev.bin')}}})
            peer = socket.socket(socket.AF_UNIX)
            peer.connect(path)
            peer.setblocking(False)
            peers.append(peer)
            vm.qmp('device_add', {'driver': 'usb-shoop-midi', 'id': f'midi-device-{index}', 'chardev': f'midi-{index}',
                'bus': usb0_bus(vm), 'port': '1.' * args.tiers + str(index + 1),
                'cables': args.cables, 'high-speed': not args.full_speed, 'serial': 'shoop-midi-serial',
                'pcap': str(directory / f'midi-{index}-usb.pcap')})
        vm.ready('SHOOP_HOST_READY')
        (directory / 'topology.txt').write_text(vm.qmp('human-monitor-command', {'command-line': 'info qtree'}))
        with socket.socket(socket.AF_UNIX) as server:
            server.bind(str(control_path))
            server.listen(1)
            server.settimeout(.25)
            done = threading.Event()
            traffic = [bytearray() for _ in peers]

            def background():
                try:
                    while not done.is_set():
                        for index, peer in enumerate(peers):
                            peer.sendall(bytes([0x0f, 0xf8, 0, 0]))
                            try:
                                while block := peer.recv(4096):
                                    traffic[index].extend(block)
                            except BlockingIOError:
                                pass
                        done.wait(.02)
                    for index, peer in enumerate(peers):
                        try:
                            while block := peer.recv(4096):
                                traffic[index].extend(block)
                        except BlockingIOError:
                            pass
                except BaseException as error:
                    errors.append(repr(error))

            def topology():
                while not done.is_set():
                    try:
                        connection, _ = server.accept()
                    except socket.timeout:
                        continue
                    with connection:
                        connection.settimeout(5)
                        command = connection.recv(128).decode().strip()
                        try:
                            if command == 'detach':
                                vm.qmp('device_del', {'id': 'midi-device'})
                            elif command == 'attach':
                                vm.qmp('device_add', {'driver': 'usb-shoop-midi', 'id': 'midi-device',
                                    'chardev': 'midi', 'bus': usb0_bus(vm), 'port': '1' + '.1' * args.tiers,
                                    'cables': args.cables, 'high-speed': not args.full_speed, 'serial': 'shoop-midi-serial'})
                            elif command.startswith('consumed '):
                                _, slot, count = command.split()
                                slot, count = int(slot), int(count)
                                assert 0 <= slot < 16 and 0 < count <= 1024
                                deadline = time.monotonic() + 25
                                while True:
                                    value = vm.qmp('human-monitor-command', {'command-line': f'xp /1wx {consumed + 4 * slot:#x}'})
                                    observed = int(value.rsplit(':', 1)[1].strip(), 16)
                                    if observed >= count:
                                        break
                                    if time.monotonic() >= deadline:
                                        raise TimeoutError(f'USB input consumption: {observed}, expected {count}')
                                    time.sleep(.02)
                            elif command == 'drained':
                                deadline = time.monotonic() + 10
                                while True:
                                    output = vm.qmp('human-monitor-command', {'command-line': f'xp /8wx {pending:#x}'})
                                    values = [int(word, 16) for line in output.splitlines()
                                              for word in line.partition(':')[2].split()]
                                    if len(values) == 8 and not any(values):
                                        break
                                    if time.monotonic() >= deadline:
                                        raise TimeoutError(f'USB OUT completion owners did not drain: {values}')
                                    time.sleep(.02)
                            else:
                                raise ValueError(command)
                            connection.sendall(b'ok\n')
                        except BaseException as error:
                            errors.append(repr(error))
                            connection.sendall(b'failed\n')
            thread = threading.Thread(target=topology)
            thread.start()
            traffic_thread = threading.Thread(target=background)
            traffic_thread.start()
            try:
                with open(directory / 'client.log', 'w') as log:
                    client = subprocess.run([ROOT / 'target/debug/examples/zynq_probe', profile], cwd=ROOT,
                                            env=environment, stdout=log, stderr=log, timeout=240)
            finally:
                done.set()
                thread.join(timeout=6)
                traffic_thread.join(timeout=6)
                assert not thread.is_alive(), 'USB topology owner survived test'
                assert not traffic_thread.is_alive(), 'USB traffic owner survived test'
                for peer in peers:
                    peer.close()
        control_path.unlink(missing_ok=True)
        client_log = (directory / 'client.log').read_text()
        print(client_log)
        if args.corrupt:
            assert client.returncode != 0 and 'actual USB IN recording changed payload' in client_log, 'Content mismatch was not detected'
        else:
            if client.returncode or errors:
                raise RuntimeError(f'USB client failed: {client.returncode} {errors}')
            serial = vm.serial.read_text()
            if args.fault == 'nak':
                vm.ready('SHOOP_USB_STOP drained=0 controller_stopped=1', timeout=15)
                serial = vm.serial.read_text()
                assert 'SHOOP_HOST_CLEANUP_COMPLETE' not in serial
                assert 'Unresponsive USB OUT refused shutdown' in client_log
                elapsed = int(re.search(r'SHOOP_USB_STOP[^\n]*elapsed_us=(\d+)', serial)[1])
                assert 5_000_000 <= elapsed <= 5_200_000, f'Guest recovery deadline: {elapsed}'
            else:
                vm.ready('SHOOP_HOST_RETURNED', timeout=15)
                serial = vm.serial.read_text()
                assert 'SHOOP_HOST_CLEANUP_COMPLETE' in serial
                if args.fault == 'stall':
                    assert 'SHOOP_USB_MIDI_ERROR' in serial
                    assert 'SHOOP_USB_STOP drained=1 controller_stopped=1' in serial
                else:
                    assert 'SHOOP_USB_STOP drained=1 controller_stopped=1 lost_packets=0 errors=0' in serial
            assert 'SHOOP_USB_FAILED' not in serial, 'Controller handshake failed'
            memory = [dict((k, int(v)) for k, v in re.findall(r'(\w+)=(\d+)', line))
                      for line in serial.splitlines() if line.startswith('SHOOP_MEMORY ')]
            assert memory and (args.fault == 'nak' or memory[-1]['used'] == 0)
            assert max(m['peak'] for m in memory) < 768 * 1024 * 1024
            metrics = {'memory': memory, 'usb_diagnostics': [line for line in serial.splitlines()
                if line.startswith(('SHOOP_USB_STOP ', 'SHOOP_USB_COUNTER ', 'SHOOP_USB_PRESSURE ', 'SHOOP_USB_MIDI_ERROR '))]}
            for index, output in enumerate(traffic):
                (directory / f'background-{index}-out.bin').write_bytes(output)
            for index, output in enumerate(traffic):
                for cable in range(args.cables):
                    expected = b''.join(bytes([cable << 4 | 11, 0xb0 | channel, cc, 0])
                                        for channel in range(16) for cc in [120, 123])
                    actual = b''.join(output[i:i+4] for i in range(0, len(output), 4) if output[i] >> 4 == cable)
                    assert actual.endswith(expected), f'Unrelated device {index} cable {cable} reset failed'
        result_path.write_text(json.dumps({'passed': True, 'gate': 'usb-' + args.fault if args.fault else 'usb-content-canary' if args.corrupt else 'usb-engine-integration',
            'cables': args.cables, 'devices': args.devices, 'hub_tiers': args.tiers, 'highspeed_capable': not args.full_speed,
            'hotplug_cycles': args.hotplug_cycles,
            'emulator': manifest, 'firmware': json.loads((ROOT / 'work/zynq/build/host-usb.json').read_text()),
            'metrics': metrics}, indent=2) + '\n')
        print(result_path)


if __name__ == '__main__':
    main()
