#!/usr/bin/env python3
"""Compare factory Python records with the exact C codec linked into both guests."""
import hashlib
from pathlib import Path
import subprocess
import sys
import tempfile

DEPLOYMENT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(DEPLOYMENT))
import records
from layout import boards


def main():
    with tempfile.TemporaryDirectory() as temporary:
        boards.load().generate(Path(temporary))
        executable = Path(temporary) / "vectors"
        subprocess.run(["cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-I", temporary, "-I", DEPLOYMENT,
                        DEPLOYMENT / "tests/vectors.c", DEPLOYMENT / "boot_state.c", "-o", executable], check=True)
        golden = subprocess.check_output([executable])
    expected = b"".join([
        records.configuration(bytes(range(32)), "02:00:00:00:00:01", "10.0.2.15", "255.255.255.0", "10.0.2.2", "10.0.2.2"),
        records.selection(hashlib.sha256(b"abc").digest()), records.counter(42), records.slot(b"abc", "firmware-a")])
    assert golden == expected and len(golden) == 2048, "C/Python deployment record ABI diverged"
    for index in range(4):
        raw = golden[index * 512:(index + 1) * 512]
        assert records.valid(raw, index + 1)
        for byte in range(512):
            for bit in range(8):
                damaged = bytearray(raw); damaged[byte] ^= 1 << bit
                assert not records.valid(damaged, index + 1)
    print("C/Python record golden vectors and all single-bit corruption positions passed")


if __name__ == "__main__":
    main()
