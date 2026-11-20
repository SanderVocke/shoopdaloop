#!/usr/bin/env python3
"""Check that SD boot authenticates both the firmware and fabric before PCAP."""
import argparse
import hashlib
import json
from pathlib import Path
import secrets
import sys
import tempfile

PLATFORM = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PLATFORM / "deployment"))

from card import create
from emulator import BootQemu
from layout import boards


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("release", type=Path, help="Directory holding shoop.itb and its loader")
    parser.add_argument("--port-base", type=int, default=15450)
    args = parser.parse_args()
    board = boards.load("zturn-v2")
    release = args.release.resolve()
    loader = release / "loader"
    metadata = json.loads((release / "shoop.json").read_text())
    fit = (release / "shoop.itb").read_bytes()
    raw = (release / "shoop.fpga.bin").read_bytes()
    assert hashlib.sha256(raw).hexdigest() == metadata["fpga_payload_sha256"]
    position = fit.index(raw)
    damaged = bytearray(fit)
    damaged[position + len(raw) // 2] ^= 1
    files = {"BOOT    BIN": (loader / "boot.bin").read_bytes(), "U-BOOT  IMG": (loader / "u-boot.img").read_bytes()}
    credential = secrets.token_bytes(32)
    check = Path(tempfile.mkdtemp(prefix="qemu-sd-", dir=release))
    for name, data in [("signed", fit), ("tampered", damaged)]:
        directory = check / name
        directory.mkdir()
        disk = directory / "card.img"
        create(disk, files, data, metadata["build"], credential, 7, board=board)
        with BootQemu(loader / "u-boot.bin", disk, directory,
                      port_base=args.port_base, updater_port=args.port_base + 2, board=board) as emulator:
            emulator.ready("SHOOP_UPDATER_READY")
            serial = emulator.serial.read_text()
            if name == "signed":
                assert f"SHOOP_FPGA_PROGRAM bytes={len(raw)}" in serial, serial
                assert "SHOOP_FPGA_PROGRAM_REFUSED" in serial, serial
            else:
                assert "SHOOP_IMAGE_REFUSED slot=0" in serial, serial
                assert "SHOOP_FPGA_PROGRAM" not in serial, serial
            assert "SHOOP_BOOT_IMAGE" not in serial, serial
    (check / "result.json").write_text(json.dumps({
        "signed_pair_reached_pcap": True,
        "tampered_fabric_refused_before_pcap": True,
        "physical_pcap_qualified": False,
    }, indent=2) + "\n")
    print(f"SD boot authenticated the FPGA pair and refused an altered fabric payload. Evidence in {check}")


if __name__ == "__main__":
    main()
