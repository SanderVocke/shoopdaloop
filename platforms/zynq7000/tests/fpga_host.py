#!/usr/bin/env python3
"""Exercise the authenticated FreeRTOS FPGA service against scripted registers."""
import argparse
import os
from pathlib import Path
import subprocess
import sys

PLATFORM = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PLATFORM))
import boards
from qemu import Qemu, ROOT
from remote_profile import write_profile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fetch", action="store_true")
    parser.add_argument("--port-base", type=int, default=15440)
    args = parser.parse_args()
    board = boards.load("zturn-v2")
    directory = board.build_dir(ROOT).parent / "fpga-host-test"
    directory.mkdir(parents=True, exist_ok=True)
    key = directory / "test.key"
    descriptor = os.open(key, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(descriptor, "wb") as stream:
        stream.write(os.urandom(32))
    profile = directory / "profile.json"
    write_profile(profile, key, args.port_base)
    build = [sys.executable, PLATFORM / "build.py", "--board", board.name, "--app", "host",
             "--fpga", "--credential-file", key, *(["--fetch"] if args.fetch else [])]
    subprocess.run(build, cwd=ROOT, check=True)
    production = board.build_dir(ROOT) / "host-fpga.elf"
    with Qemu(production, directory / "absent", [], board=board) as emulator:
        emulator.ready("SHOOP_FPGA_HOST_UNAVAILABLE no configured bitstream")
        emulator.ready("SHOOP_HOST_RETURNED")
    subprocess.run([*build, "--fixture"], cwd=ROOT, check=True)
    fixture = board.build_dir(ROOT) / "host-fixture-fpga.elf"
    for nonce, marker in [(2, "SHOOP_FPGA_OWNER_FAILED FPGA mailbox Timeout"),
                          (3, "SHOOP_FPGA_HOST_UNAVAILABLE Incompatible")]:
        with Qemu(fixture, directory / f"refused-{nonce}", [], boot_nonce=nonce, board=board) as emulator:
            emulator.ready(marker)
            emulator.ready("SHOOP_HOST_RETURNED")
    subprocess.run(["cargo", "build", "--locked", "-p", "shoop_host", "--no-default-features",
                    "--example", "zynq_probe"], cwd=ROOT, check=True)
    ports = [(args.port_base, 15400), (args.port_base + 1, 15401)]
    with Qemu(fixture, directory / "ready", ports, boot_nonce=1, board=board) as emulator:
        emulator.ready("SHOOP_HOST_READY")
        with (directory / "client.log").open("w") as log:
            result = subprocess.run([ROOT / "target/debug/examples/zynq_probe", profile], cwd=ROOT,
                                    env={**os.environ, "SHOOP_ZYNQ_FPGA": "1"},
                                    stdout=log, stderr=log, timeout=60)
        if result.returncode:
            raise RuntimeError((directory / "client.log").read_text())
        emulator.ready("SHOOP_HOST_CLEANUP_COMPLETE")
        emulator.ready("SHOOP_HOST_RETURNED")
        assert "SHOOP_MEMORY used=0 " in emulator.serial.read_text()
    print("FPGA FreeRTOS production refusal and authenticated fixture passed")


if __name__ == "__main__":
    main()
