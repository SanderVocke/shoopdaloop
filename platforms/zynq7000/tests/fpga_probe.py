#!/usr/bin/env python3
"""Exercise absent fabric and scripted FPGA mailbox startup on FreeRTOS."""
import argparse
from pathlib import Path
import subprocess
import sys

PLATFORM = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PLATFORM))
import boards
from qemu import Qemu, ROOT


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fetch", action="store_true")
    args = parser.parse_args()
    board = boards.load("zturn-v2")
    directory = board.build_dir(ROOT).parent / "fpga-probe-test"
    command = [sys.executable, PLATFORM / "build.py", "--board", board.name, "--app", "host",
               "--fpga-probe"]
    if args.fetch:
        command.append("--fetch")
    subprocess.run(command, check=True, cwd=ROOT)
    elf = board.build_dir(ROOT) / "host-fpga-probe.elf"
    with Qemu(elf, directory, [], board=board) as emulator:
        emulator.ready("SHOOP_FPGA_PROBE_UNAVAILABLE no configured bitstream", timeout=30)
        emulator.ready("SHOOP_FPGA_PROBE_RETURNED", timeout=5)
    print("FreeRTOS refused the missing FPGA mailbox and returned without starting a session")
    subprocess.run([*command, "--fixture"], check=True, cwd=ROOT)
    fixture = board.build_dir(ROOT) / "host-fixture-fpga-probe.elf"
    for nonce, name, marker in [
        (1, "ready", "SHOOP_FPGA_PROBE_READY"),
        (2, "timeout", "SHOOP_FPGA_PROBE_UNAVAILABLE snapshot timeout"),
        (3, "incompatible", "SHOOP_FPGA_PROBE_UNAVAILABLE Incompatible"),
    ]:
        with Qemu(fixture, directory / name, [], boot_nonce=nonce, board=board) as emulator:
            emulator.ready(marker, timeout=30)
            emulator.ready("SHOOP_FPGA_PROBE_RETURNED", timeout=5)
    print("FreeRTOS exercised compatible, unresponsive and incompatible FPGA mailboxes")


if __name__ == "__main__":
    main()
