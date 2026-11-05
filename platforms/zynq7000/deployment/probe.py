#!/usr/bin/env python3
"""Qualify native clients through real U-Boot, SD and network image loading."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
from emulator import BootQemu, ROOT


def client_command(ui):
    if ui:
        built = subprocess.run(["cargo", "test", "--locked", "-p", "shoopdaloop", "--no-default-features", "--bin", "shoopdaloop",
                                "--no-run", "--message-format=json"], cwd=ROOT, check=True, capture_output=True, text=True)
        executable = next(json.loads(line)["executable"] for line in built.stdout.splitlines() if json.loads(line).get("executable"))
        return [executable, "--exact", "native_recovery_tests::zynq_firmware_ui_workflow", "--ignored", "--nocapture"]
    subprocess.run(["cargo", "build", "--locked", "-p", "shoop_host", "--no-default-features", "--example", "zynq_probe"], cwd=ROOT, check=True)
    return [ROOT / "target/debug/examples/zynq_probe"]


def metrics(serial):
    memory = [{k: int(v) for k, v in re.findall(r"(\w+)=(\d+)", line)}
              for line in serial.splitlines() if line.startswith("SHOOP_MEMORY ")]
    c_memory = [{k: int(v) for k, v in re.findall(r"(\w+)=(\d+)", line)}
                for line in serial.splitlines() if line.startswith("SHOOP_C_MEMORY ")]
    assert memory and memory[-1]["used"] == 0 and max(m["peak"] for m in memory) < 768 * 1024 * 1024
    assert min(m["c_min_free"] for m in memory) > 1024 * 1024
    assert c_memory and c_memory[-1]["sockets"] == c_memory[-1]["netconns"] == 0
    assert "SHOOP_DUMMY_MIDI_RESET packets=32" in serial
    assert "SHOOP_HOST_CLEANUP_COMPLETE" in serial
    return memory, c_memory


def run(loader, card, profile, directory, build, ui=False, warm=False, tftp=None, port_base=16400, updater_port=16554):
    directory.mkdir(parents=True, exist_ok=True)
    result = {"passed": False, "boots": [], "ui": ui, "warm_reset": warm, "network_boot": tftp is not None}
    (directory / "result.json").write_text(json.dumps(result) + "\n")
    command = client_command(ui)
    previous_nonce = None
    emulator = None
    try:
        for boot in range(1 if ui else 2):
            after = 0
            if not warm or boot == 0:
                emulator = BootQemu(loader, card, directory / f"boot-{boot}", port_base=port_base,
                                    updater_port=updater_port, tftp=tftp if boot == 0 else None)
            else:
                after = len(emulator.serial.read_text())
                emulator.qmp("system_reset")
            network_boot = tftp is not None and boot == 0
            emulator.ready("SHOOP_HOST_READY", timeout=120, after=after)
            expected = "SHOOP_DEPLOY_RAM_READY" if network_boot else "SHOOP_DEPLOY_CONFIRMED"
            assert expected in emulator.serial.read_text()[after:], "Incorrect boot source or missing health confirmation"
            environment = dict(os.environ)
            for key in ["SHOOP_ZYNQ_PREVIOUS_AUTHORITY", "SHOOP_ZYNQ_AUTHORITY_OUTPUT"]:
                environment.pop(key, None)
            environment["SHOOP_ZYNQ_PROFILE"] = str(profile)
            authority = directory / "authority.json"
            environment["SHOOP_ZYNQ_AUTHORITY_OUTPUT" if boot == 0 else "SHOOP_ZYNQ_PREVIOUS_AUTHORITY"] = str(authority)
            arguments = command if ui else [*command, profile]
            with (directory / f"client-{boot}.log").open("w") as log:
                completed = subprocess.run(arguments, cwd=ROOT, env=environment, stdout=log, stderr=log, timeout=360)
            if completed.returncode:
                raise RuntimeError((directory / f"client-{boot}.log").read_text()[-12000:])
            emulator.ready("SHOOP_HOST_RETURNED", timeout=30, after=after)
            serial = emulator.serial.read_text()[after:]
            images = re.findall(r"SHOOP_BOOT_IMAGE slot=(\d+) trial=(\d+) nonce=(\d+) build=([\w.-]+)", serial)
            assert len(images) == 1 and images[0][3] == build, "Unexpected image or reset during healthy boot"
            slot, trial, nonce, _ = images[0]
            assert (int(slot) == 0xffffffff) == network_boot
            assert f"SHOOP_BUILD {build}" in serial
            assert previous_nonce is None or int(nonce) > previous_nonce
            previous_nonce = int(nonce)
            memory, c_memory = metrics(serial)
            result["boots"].append({"source": "network" if network_boot else "sd", "slot": int(slot), "trial": int(trial),
                                    "nonce": int(nonce), "build": build, "memory": memory, "c_memory": c_memory})
            if not warm:
                emulator.close(); emulator = None
        result["passed"] = True
        print("Deployment client qualification passed:", "native UI" if ui else "warm restart" if warm else "cold restart")
    finally:
        if emulator is not None:
            emulator.close()
        (directory / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for option in ["loader", "card", "profile", "directory"]:
        parser.add_argument("--" + option, type=Path, required=True)
    parser.add_argument("--build-id", required=True)
    parser.add_argument("--ui", action="store_true")
    parser.add_argument("--warm", action="store_true")
    parser.add_argument("--tftp", type=Path)
    parser.add_argument("--port-base", type=int, default=16400)
    parser.add_argument("--updater-port", type=int, default=16554)
    args = parser.parse_args()
    if args.ui and (args.warm or args.tftp):
        parser.error("The UI probe runs one local boot")
    run(args.loader, args.card, args.profile.resolve(), args.directory.resolve(), args.build_id,
        ui=args.ui, warm=args.warm, tftp=args.tftp, port_base=args.port_base, updater_port=args.updater_port)


if __name__ == "__main__":
    main()
