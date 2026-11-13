#!/usr/bin/env python3
"""Cross-build both FPGA variants and verify a bounded refusal when QEMU has no PL bridge."""
import argparse
import json
import os
import subprocess
import sys

import boards
from qemu import Qemu, ROOT


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fetch", action="store_true")
    args = parser.parse_args()
    board = boards.load("zedboard", fpga=True)
    directory = ROOT / "work/zynq/boards/zedboard/fpga-test"
    directory.mkdir(parents=True, exist_ok=True)
    result = {"passed": False, "variants": []}
    (directory / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    credential = directory / "test.key"
    descriptor = os.open(credential, os.O_CREAT | os.O_TRUNC | os.O_WRONLY, 0o600)
    with os.fdopen(descriptor, "wb") as stream:
        stream.write(os.urandom(32))
    credential.chmod(0o600)
    for traced in [False, True]:
        command = [sys.executable, "platforms/zynq7000/build.py", "--app", "host", "--board", "zedboard",
                   "--fpga", "--credential-file", credential]
        if args.fetch and not traced:
            command += ["--fetch"]
        if traced:
            command += ["--fpga-trace", "1024"]
        subprocess.run(command, cwd=ROOT, check=True)
        name = "host-fpga-trace" if traced else "host-fpga"
        elf = board.build_dir(ROOT) / (name + ".elf")
        manifest = json.loads(elf.with_suffix(".json").read_text())
        assert manifest["fpga"] and manifest["fpga_arena"] == {"base": board.heap_end, "bytes": board.arena_bytes}
        assert manifest["fpga_trace_records"] == (1024 if traced else None)
        with Qemu(elf, directory / name, [], board=board) as emulator:
            emulator.ready("SHOOP_FPGA_UNAVAILABLE the PL is not configured", timeout=30)
            serial = emulator.serial.read_text()
            assert "SHOOP_FPGA_BRIDGE_READY" not in serial and "SHOOP_HOST_READY" not in serial, serial
        result["variants"].append({"name": name, "missing_bridge_refused": True,
                                   "elf_sha256": manifest["elf_sha256"]})
        (directory / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    result["passed"] = True
    (directory / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print("Both FPGA firmware variants built, audited and refused a missing bridge")


if __name__ == "__main__":
    main()
