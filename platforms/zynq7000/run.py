#!/usr/bin/env python3
"""Build a provisioned production host and keep QEMU alive for native UI use."""
import argparse
from pathlib import Path
import subprocess
import sys
import time
from qemu import Qemu, ROOT
from remote_profile import write_profile
import boards


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--credential-file", required=True, type=Path)
    parser.add_argument("--fetch", action="store_true")
    parser.add_argument("--port-base", type=int, default=15400)
    boards.argument(parser)
    args = parser.parse_args()
    board = boards.load(args.board)
    credential = args.credential_file.resolve()
    build = [sys.executable, str(Path(__file__).parent / "build.py"), "--app", "host", "--credential-file", str(credential), "--board", args.board]
    if args.fetch:
        build.append("--fetch")
    subprocess.run(build, cwd=ROOT, check=True)
    directory = ROOT / "work/zynq/interactive"
    if args.board != boards.DEFAULT:
        directory = directory.parent / "boards" / args.board / directory.name
    directory.mkdir(parents=True, exist_ok=True)
    profile = directory / "profile.json"
    write_profile(profile, credential, args.port_base)
    with Qemu(board.build_dir(ROOT) / "host.elf", directory,
              [(args.port_base, 15400), (args.port_base+1, 15401)], board=board) as emulator:
        emulator.ready("SHOOP_HOST_READY", timeout=60)
        print(f"Connect with: cargo run -p shoopdaloop -- --remote-profile {profile}", flush=True)
        print("Ctrl+C stops the emulator. The UI can also request confirmed host shutdown.", flush=True)
        try:
            while "SHOOP_HOST_RETURNED" not in emulator.serial.read_text():
                if emulator.process.poll() is not None or "FATAL" in emulator.serial.read_text():
                    raise RuntimeError("Firmware failed; inspect " + str(directory))
                time.sleep(.25)
        except KeyboardInterrupt:
            pass


if __name__ == "__main__":
    main()
