#!/usr/bin/env python3
"""Boot, update and restart a board's signed firmware on the emulated PS."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

PLATFORM = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PLATFORM))
sys.path.insert(0, str(PLATFORM / "deployment"))
import boards
import bundle
from emulator import BootQemu, ROOT
from remote_profile import write_profile


def run(arguments, **kwargs):
    return subprocess.run([str(a) for a in arguments], cwd=ROOT, check=True, **kwargs)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    boards.argument(parser)
    parser.add_argument("--directory", type=Path, required=True, help="New private test directory")
    args = parser.parse_args()
    board = boards.load(args.board)
    directory = args.directory.resolve()
    directory.mkdir(mode=0o700, parents=True, exist_ok=False)
    private = directory / "private"
    private.mkdir(mode=0o700)
    artifacts = directory / "artifacts"
    artifacts.mkdir()
    keys = private / "keys"
    keys.mkdir(mode=0o700)
    result = {**board.manifest(), "passed": False, "ram_bytes": board.ram}
    result_path = directory / "result.json"
    result_path.write_text(json.dumps(result, indent=2) + "\n")
    run(["openssl", "req", "-new", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
         "-subj", "/CN=Shoop-board-profile-test/", "-keyout", keys / "deployment.key",
         "-out", keys / "deployment.crt"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    (keys / "deployment.key").chmod(0o600)
    credential = private / "credential"
    credential.write_bytes(os.urandom(32))
    credential.chmod(0o600)
    source = board.build_dir(ROOT) / "host-deployment.elf"
    initial_build = json.loads(source.with_suffix(".json").read_text())["build_id"]
    identities = [initial_build, "board-update" if initial_build != "board-update" else "board-update-2"]
    loader = artifacts / "loader"
    for index, identity in enumerate(identities):
        if index:
            run([sys.executable, PLATFORM / "build.py", "--board", args.board,
                 "--app", "host", "--deployment", "--build-id", identity])
        elf = artifacts / (identity + ".elf")
        shutil.copyfile(source, elf)
        shutil.copyfile(source.with_suffix(".json"), elf.with_suffix(".json"))
        bundle.create(elf, identity, keys, artifacts / (identity + ".itb"), loader, board)
    card = private / "card.img"
    run([sys.executable, PLATFORM / "deployment/card.py", "--board", args.board,
         "--output", card, "--boot-files", loader, "--bundle", artifacts / (identities[0] + ".itb"),
         "--build-id", identities[0], "--credential-file", credential])
    profile = private / "profile.json"
    write_profile(profile, credential, 16420)
    run(["cargo", "build", "--locked", "-p", "shoop_host", "--no-default-features",
         "--bin", "shoop-deploy", "--example", "zynq_probe"])
    environment = {**os.environ, "SHOOP_ZYNQ_RECORDING_MIB": str(board.firmware["recording_mib"])}
    nonces = []
    for boot in range(2):
        evidence = private / ("boot" if boot == 0 else "restart")
        with BootQemu(loader / "u-boot.bin", card, evidence, port_base=16420,
                      updater_port=16574, board=board) as vm:
            vm.ready("SHOOP_HOST_READY", timeout=90)
            if boot == 0:
                with (private / "update.log").open("w") as log:
                    run([ROOT / "target/debug/shoop-deploy", "--profile", profile,
                         "--updater", "127.0.0.1:16574", "deploy", artifacts / (identities[1] + ".itb")],
                        stdout=log, stderr=log, timeout=360)
            with (private / ("client.log" if boot == 0 else "restart-client.log")).open("w") as log:
                run([ROOT / "target/debug/examples/zynq_probe", profile], env=environment,
                    stdout=log, stderr=log, timeout=360)
            vm.ready("SHOOP_HOST_RETURNED", timeout=30)
            serial = vm.serial.read_text()
            boots = re.findall(r"SHOOP_BOOT_IMAGE .*?nonce=(\d+) build=([\w.-]+)", serial)
            assert boots and boots[-1][1] == identities[1], "Updated firmware was not running"
            nonces.extend(int(nonce) for nonce, _ in boots)
            memory = [{k: int(v) for k, v in re.findall(r"(\w+)=(\d+)", line)}
                      for line in serial.splitlines() if line.startswith("SHOOP_MEMORY ")]
            assert memory and memory[-1]["used"] == 0
            assert max(m["peak"] for m in memory) < board.memory["rust_mib"] * boards.MIB
    assert len(nonces) >= 3 and nonces == sorted(set(nonces)), "Boot authority was reused"
    result.update(passed=True, nonces=nonces, builds=identities)
    result_path.write_text(json.dumps(result, indent=2) + "\n")
    print(f"{args.board}: signed SD boot, A/B update, cleanup and persistent restart identity passed")


if __name__ == "__main__":
    main()
