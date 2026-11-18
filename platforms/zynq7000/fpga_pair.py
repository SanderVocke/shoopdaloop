#!/usr/bin/env python3
"""Record and verify a matched Z-turn V2 FPGA and firmware build."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import secrets
import select
import subprocess
import termios
import time
import tty

import boards

ROOT = Path(__file__).resolve().parents[2]
PART = "xc7z020clg400-2"
RTL_ABI = ROOT / "fpga/host_bridge/registers.json"
NATIVE_AUDIO_ABI = ROOT / "fpga/host_bridge/native_audio.json"
BUILD_FILES = {"zturn_v2.bit", "zturn_v2.xsa", "ps7_init.tcl", "ps-properties.txt", "fpga-abi.json", "geometry.txt"}


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
    files = BUILD_FILES | ({"audio.txt"} if (directory / "audio.txt").exists() else set())
    write_json(directory / "fpga-build.json", {
        "contract": contract(board, directory / "fpga-abi.json"), "vivado": tool,
        "sha256": {name: digest(directory / name) for name in sorted(files)},
    })


def pair(directory, elf, output):
    fpga = json.loads((directory / "fpga-build.json").read_text())
    firmware = json.loads(elf.with_suffix(".json").read_text())
    renderer = firmware.get("fpga_renderer", False)
    native_audio = firmware.get("native_i2s", False)
    if (firmware.get("fpga_contract") != fpga["contract"] or
            sum(map(bool, (renderer, native_audio, firmware.get("fpga_probe")))) != 1 or firmware.get("fixture") or
            firmware.get("elf_sha256") != digest(elf)):
        raise ValueError("Firmware does not match the FPGA image")
    inputs = {"firmware": (elf, firmware["elf_sha256"])}
    for name, sha256 in fpga["sha256"].items():
        inputs[name] = (directory / name, sha256)
    if any(digest(path) != expected for path, expected in inputs.values()):
        raise ValueError("Build artifact differs from the recorded digest")
    output.parent.mkdir(parents=True, exist_ok=True)
    geometry = (directory / "geometry.txt").read_text().strip()
    if renderer and geometry != "looping":
        raise ValueError("FPGA renderer requires the looping geometry")
    if native_audio != (geometry == "native"):
        raise ValueError("Native I2S firmware requires the audio-only bitstream")
    write_json(output, {"contract": fpga["contract"], "vivado": fpga["vivado"], "renderer": renderer,
                        "native_i2s": native_audio,
                        "deployment": bool(firmware.get("deployment")), "artifacts": {
        name: {"path": os.path.relpath(path, output.parent), "sha256": sha256}
        for name, (path, sha256) in inputs.items()
    }})


def verify(path):
    manifest = json.loads(path.read_text())
    artifacts = manifest["artifacts"]
    names = BUILD_FILES | {"firmware"}
    native_audio = bool(manifest.get("native_i2s"))
    abi = NATIVE_AUDIO_ABI if native_audio else RTL_ABI
    if set(artifacts) not in (names, names | {"audio.txt"}) or manifest["contract"] != contract(boards.load("zturn-v2"), abi):
        raise ValueError("Unexpected artifacts or FPGA contract")
    for name, artifact in artifacts.items():
        if digest(path.parent / artifact["path"]) != artifact["sha256"]:
            raise ValueError(f"Changed FPGA artifact {name}")
    return manifest


def jtag(manifest_path, server, xsdb, uart=None, uart_log=None):
    manifest = verify(manifest_path)
    if manifest.get("deployment"):
        raise ValueError("JTAG startup requires the direct-load ELF")
    artifacts = manifest["artifacts"]
    files = [(manifest_path.parent / artifacts[name]["path"]).resolve()
             for name in ["ps7_init.tcl", "zturn_v2.bit", "firmware"]]
    nonce = secrets.randbits(64) or 1
    script = Path(__file__).with_name("fpga_probe.tcl")
    command = [xsdb, script, server, *files, str(nonce & 0xffffffff), str(nonce >> 32)]
    if uart is None:
        subprocess.run(command, check=True, timeout=120)
        return
    marker = b"SHOOP_HOST_READY" if manifest.get("renderer") or manifest.get("native_i2s") else b"SHOOP_FPGA_PROBE_READY"
    capture_uart(command, uart, uart_log, marker)


def capture_uart(command, uart, uart_log, marker):
    uart_log.parent.mkdir(parents=True, exist_ok=True)
    descriptor = os.open(uart, os.O_RDONLY | os.O_NOCTTY | os.O_NONBLOCK)
    try:
        tty.setraw(descriptor)
        attributes = termios.tcgetattr(descriptor)
        attributes[4:6] = [termios.B115200, termios.B115200]
        termios.tcsetattr(descriptor, termios.TCSANOW, attributes)
        termios.tcflush(descriptor, termios.TCIFLUSH)
        with uart_log.open("wb") as log, subprocess.Popen(command) as process:
            deadline = time.monotonic() + 120
            tail = b""
            ready = False
            try:
                while time.monotonic() < deadline:
                    status = process.poll()
                    if status not in (None, 0):
                        raise subprocess.CalledProcessError(status, command)
                    if status == 0 and ready:
                        return
                    if select.select([descriptor], [], [], 0.1)[0]:
                        data = os.read(descriptor, 8192)
                        log.write(data)
                        log.flush()
                        window = tail + data
                        if any(failure in window for failure in (
                            b"FATAL", b"SHOOP_FPGA_OWNER_FAILED", b"SHOOP_FPGA_HOST_UNAVAILABLE",
                            b"SHOOP_FPGA_PROBE_UNAVAILABLE",
                            b"SHOOP_NATIVE_I2S_UNAVAILABLE",
                        )):
                            raise RuntimeError(f"FPGA boot failed. See {uart_log}")
                        ready |= marker in window
                        tail = window[-64:]
                raise TimeoutError(f"FPGA boot did not reach {marker.decode()}. See {uart_log}")
            finally:
                if process.poll() is None:
                    process.kill()
    finally:
        os.close(descriptor)


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
    jtag_parser = commands.add_parser("jtag", help="Verify a pair, then program and start the firmware through XSDB")
    jtag_parser.add_argument("manifest", type=Path)
    jtag_parser.add_argument("--server", required=True)
    jtag_parser.add_argument("--xsdb", default="xsdb")
    jtag_parser.add_argument("--uart", type=Path)
    jtag_parser.add_argument("--uart-log", type=Path)
    args = parser.parse_args()
    if args.op == "stamp":
        stamp(args.directory, args.tool)
    elif args.op == "pair":
        pair(args.directory, args.elf, args.output)
    elif args.op == "verify":
        verify(args.manifest)
        print("Verified FPGA pairing")
    else:
        if (args.uart is None) != (args.uart_log is None):
            parser.error("--uart and --uart-log are used together")
        jtag(args.manifest, args.server, args.xsdb, args.uart, args.uart_log)


if __name__ == "__main__":
    main()
