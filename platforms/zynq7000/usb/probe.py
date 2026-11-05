#!/usr/bin/env python3
"""Exercise actual firmware enumeration through the pinned USB controller model."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import socket
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from qemu import Qemu, ROOT


def usb_emulator():
    binary = ROOT / "work/zynq/qemu-usb-build/qemu-system-arm"
    manifest = json.loads(binary.with_name("shoop-build.json").read_text())
    platform = ROOT / "platforms/zynq7000"
    pins = json.loads((platform / "pins.json").read_text())
    expected = {
        "source": pins["qemu_source"],
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "patch_sha256": hashlib.sha256((platform / "patches/qemu-cadence-watchdog.patch").read_bytes()).hexdigest(),
        "usb_patch_sha256": hashlib.sha256((platform / "patches/qemu-usb-midi.patch").read_bytes()).hexdigest(),
        "usb_fixture_sha256": hashlib.sha256((platform / "usb/qemu/dev-midi.c").read_bytes()).hexdigest(),
    }
    if any(manifest.get(key) != value for key, value in expected.items()):
        raise RuntimeError("USB emulator does not match its pinned source and model manifest")
    return binary, manifest


def usb0_bus(vm):
    tree = vm.qmp("human-monitor-command", {"command-line": "info qtree"})
    match = re.search(r"mmio 00000000e0002000/[^\n]+\n\s+bus: ([^\n]+)", tree)
    if not match:
        raise RuntimeError("USB0 bus was not found in the actual machine tree")
    return match.group(1).strip()


def attach(vm, directory, cables=1, highspeed=True, tiers=0, open_peer=True, fault=None):
    bus = usb0_bus(vm)
    port = "1"
    for tier in range(tiers):
        vm.qmp("device_add", {"driver": "usb-hub", "id": f"hub{tier}", "bus": bus, "port": port})
        port += ".1"
    path = directory / "midi.sock"
    vm.qmp("chardev-add", {"id": "midi", "backend": {"type": "socket", "data": {
        "addr": {"type": "unix", "data": {"path": str(path)}}, "server": True, "wait": False,
        "logfile": str(directory / "midi-chardev.bin")}}})
    peer = socket.socket(socket.AF_UNIX) if open_peer else None
    try:
        if peer:
            peer.settimeout(5)
            peer.connect(str(path))
        vm.qmp("device_add", {"driver": "usb-shoop-midi", "id": "midi-device", "chardev": "midi",
                              "bus": bus, "port": port, "cables": cables, "high-speed": highspeed,
                              "serial": "shoop-midi-serial", "stall-out": fault == 'stall', "nak-out": fault == 'nak'})
    except BaseException:
        if peer:
            peer.close()
        raise
    return peer


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--full-speed", action="store_true")
    parser.add_argument("--tiers", type=int, choices=[0, 1, 2], default=0)
    parser.add_argument("--cables", type=int, choices=range(1, 17), default=1)
    parser.add_argument("--directory", type=Path, default=ROOT / "work/zynq/usb-enumeration")
    args = parser.parse_args()
    binary, manifest = usb_emulator()
    os.environ["QEMU"] = str(binary)
    directory = args.directory.resolve()
    with Qemu(ROOT / "work/zynq/build/host-usb.elf", directory, []) as vm:
        vm.ready("SHOOP_USB_READY")
        with attach(vm, directory, args.cables, not args.full_speed, args.tiers) as peer:
            vm.ready(f"rx={args.cables} tx={args.cables}")
            peer.sendall(bytes([9, 0x90, 60, 100, 8, 0x80, 60, 0]))
            vm.ready("SHOOP_HOST_READY")
            result = {"passed": True, "gate": "usb-enumeration", "cables": args.cables,
                      "highspeed_capable": not args.full_speed, "hub_tiers": args.tiers, "emulator": manifest}
            (directory / "result.json").write_text(json.dumps(result, indent=2) + "\n")
            print(json.dumps({key: value for key, value in result.items() if key != "emulator"}))


if __name__ == "__main__":
    main()
