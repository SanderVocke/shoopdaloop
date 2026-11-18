#!/usr/bin/env python3
"""Record and verify a matched Z-turn V2 FPGA diagnostic image."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import secrets
import subprocess

import boards

ROOT = Path(__file__).resolve().parents[2]
PART = "xc7z020clg400-2"
RTL_ABI = ROOT / "fpga/host_bridge/registers.json"


def digest(path):
    sha256 = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1 << 20), b""):
            sha256.update(block)
    return sha256.hexdigest()


def contract(board, abi_path=RTL_ABI):
    abi = json.loads(abi_path.read_text())
    return {**board.manifest(), "part": PART, "abi_id": abi["id"], "abi_version": abi["version"],
            "abi_sha256": digest(abi_path), "arena_base": board.rust_end, "arena_bytes": board.pl_bytes,
            **board.data["fpga"]}


def write_json(path, data):
    path.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")


def stamp(directory, tool):
    board = boards.load("zturn-v2")
    generated = json.loads((directory / "board/board.json").read_text())
    if generated != board.data:
        raise ValueError("Vivado build uses a different board profile")
    files = ["zturn_v2.bit", "zturn_v2.xsa", "ps7_init.tcl", "ps-properties.txt", "fpga-abi.json", "geometry.txt"]
    write_json(directory / "fpga-build.json", {
        "contract": contract(board, directory / "fpga-abi.json"), "vivado": tool,
        "sha256": {name: digest(directory / name) for name in files},
    })


def pair(directory, elf, output):
    fpga = json.loads((directory / "fpga-build.json").read_text())
    firmware = json.loads(elf.with_suffix(".json").read_text())
    if (firmware.get("fpga_contract") != fpga["contract"] or not firmware.get("fpga_probe") or firmware.get("fixture") or
            firmware.get("elf_sha256") != digest(elf)):
        raise ValueError("Firmware does not match the FPGA diagnostic image")
    inputs = {"firmware": (elf, firmware["elf_sha256"])}
    for name, sha256 in fpga["sha256"].items():
        inputs[name] = (directory / name, sha256)
    if any(digest(path) != expected for path, expected in inputs.values()):
        raise ValueError("Build artifact differs from the recorded digest")
    output.parent.mkdir(parents=True, exist_ok=True)
    write_json(output, {"contract": fpga["contract"], "vivado": fpga["vivado"], "artifacts": {
        name: {"path": os.path.relpath(path, output.parent), "sha256": sha256}
        for name, (path, sha256) in inputs.items()
    }})


def verify(path):
    manifest = json.loads(path.read_text())
    artifacts = manifest["artifacts"]
    names = {"firmware", "zturn_v2.bit", "zturn_v2.xsa", "ps7_init.tcl", "ps-properties.txt", "fpga-abi.json", "geometry.txt"}
    if set(artifacts) != names or manifest["contract"] != contract(boards.load("zturn-v2")):
        raise ValueError("Unexpected artifacts or FPGA contract")
    for name, artifact in artifacts.items():
        if digest(path.parent / artifact["path"]) != artifact["sha256"]:
            raise ValueError(f"Changed FPGA artifact {name}")
    return manifest


def jtag(manifest_path, server, xsdb):
    manifest = verify(manifest_path)
    artifacts = manifest["artifacts"]
    files = [(manifest_path.parent / artifacts[name]["path"]).resolve()
             for name in ["ps7_init.tcl", "zturn_v2.bit", "firmware"]]
    nonce = secrets.randbits(64) or 1
    script = Path(__file__).with_name("fpga_probe.tcl")
    subprocess.run([xsdb, script, server, *files, str(nonce & 0xffffffff), str(nonce >> 32)],
                   check=True, timeout=120)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="op", required=True)
    stamp_parser = commands.add_parser("stamp")
    stamp_parser.add_argument("directory", type=Path)
    stamp_parser.add_argument("tool")
    pair_parser = commands.add_parser("pair")
    pair_parser.add_argument("directory", type=Path)
    pair_parser.add_argument("elf", type=Path)
    pair_parser.add_argument("output", type=Path)
    verify_parser = commands.add_parser("verify")
    verify_parser.add_argument("manifest", type=Path)
    jtag_parser = commands.add_parser("jtag", help="Verify a pair, then program and start its probe through XSDB")
    jtag_parser.add_argument("manifest", type=Path)
    jtag_parser.add_argument("--server", required=True)
    jtag_parser.add_argument("--xsdb", default="xsdb")
    args = parser.parse_args()
    if args.op == "stamp":
        stamp(args.directory, args.tool)
    elif args.op == "pair":
        pair(args.directory, args.elf, args.output)
    elif args.op == "verify":
        verify(args.manifest)
        print("Verified FPGA diagnostic pairing")
    else:
        jtag(args.manifest, args.server, args.xsdb)


if __name__ == "__main__":
    main()
