"""Build public generic firmware/loader artifacts and disposable private test inputs."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys

import bundle
from emulator import ROOT
from remote_profile import write_profile

PLATFORM = Path(__file__).resolve().parent.parent


def run(arguments, **kwargs):
    subprocess.run([str(a) for a in arguments], cwd=ROOT, check=True, **kwargs)


def build(directory, fetch=False, port_base=16400, native_coverage=False, usb=False):
    artifacts, private = directory / "artifacts", directory / "private"
    artifacts.mkdir(); private.mkdir(mode=0o700)
    keys = private / "keys"; keys.mkdir(mode=0o700)
    run(["openssl", "req", "-new", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
         "-subj", "/CN=Shoop-disposable-deployment-test/", "-keyout", keys / "deployment.key", "-out", keys / "deployment.crt"])
    (keys / "deployment.key").chmod(0o600)
    descriptor = os.open(private / "credential", os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "wb") as credential:
        credential.write(os.urandom(32))
    write_profile(private / "profile.json", (private / "credential").resolve(), port_base)
    for script in ["build_boot.py", "build_qemu.py"]:
        run([sys.executable, PLATFORM / "deployment" / script, *(["--fetch"] if fetch else []), *(["--usb"] if usb and script == "build_qemu.py" else [])])
    for index, (identity, fault) in enumerate([("firmware-a", None), ("firmware-b", None),
                            *[("fault-" + f, f) for f in ["before-ready", "render", "control", "broker", "handoff-version", "cleanup"]]]):
        run([sys.executable, PLATFORM / "build.py", "--app", "host", "--deployment", "--build-id", identity, *(["--usb"] if usb else []),
             *(["--fetch"] if fetch and index == 0 else []), *(["--deployment-test-fault", fault] if fault else [])])
        source = ROOT / "work/zynq/build" / (("host-usb" if usb else "host") + ("-deployment-test.elf" if fault else "-deployment.elf"))
        for suffix in [".elf", ".map", ".json", *([".licenses.txt"] if usb else [])]:
            shutil.copyfile(source.with_suffix(suffix), artifacts / (identity + ("-build.json" if suffix == ".json" else suffix)))
        bundle.create(artifacts / (identity + ".elf"), identity, keys, artifacts / (identity + ".itb"), artifacts / "loader")
    # Signed negative fixtures distinguish target/signature/load-contract refusals.
    negative = bytearray((artifacts / "firmware-b.itb").read_bytes())
    position = negative.rindex(b"firmware-b\0")
    negative[position] ^= 1
    (artifacts / "bad-signature.itb").write_bytes(negative)
    (artifacts / "bad-length.itb").write_bytes((artifacts / "firmware-b.itb").read_bytes()[:-1])
    bad_its = artifacts / "bad-profile.its"
    bad_its.write_text((artifacts / "firmware-b.its").read_text().replace('shoop,profile = "shoop-zynq7000-1g-v1"', 'shoop,profile = "incompatible-board"'))
    run([bundle.BOOT / "tools/mkimage", "-f", bad_its, "-k", keys, artifacts / "bad-profile.itb"], env={**os.environ, "SOURCE_DATE_EPOCH": "0"})
    bad_elf = bytearray((artifacts / "firmware-b.elf").read_bytes())
    start = struct.unpack_from("<I", bad_elf, 28)[0]
    stride, count = struct.unpack_from("<HH", bad_elf, 42)
    phoff = next(start + stride * i for i in range(count) if struct.unpack_from("<I", bad_elf, start + stride * i)[0] == 1)
    struct.pack_into("<II", bad_elf, phoff + 8, 0x3c000000, 0x3c000000)
    (artifacts / "bad-range.elf").write_bytes(bad_elf)
    bundle.create(artifacts / "bad-range.elf", "bad-range", keys, artifacts / "bad-range.itb", artifacts / "loader")
    for source, target in [(bundle.BOOT / "shoop-build.json", "loader-build.json"),
                           (ROOT / ("work/zynq/qemu-usb-build/shoop-build.json" if usb else "work/zynq/qemu-build/shoop-build.json"), "emulator-build.json"),
                           (PLATFORM / "pins.json", "pins.json")]:
        shutil.copyfile(source, artifacts / target)
    shutil.copyfile(bundle.BOOT / "spl/u-boot-spl", artifacts / "loader/u-boot-spl")
    run(["cargo", "build", "--locked", "-p", "shoop_host", "--no-default-features", "--bin", "shoop-deploy", "--example", "zynq_probe"])
    if native_coverage:
        from native_coverage import build as build_cli
        build_cli(directory)
    manifest = {str(p.relative_to(artifacts)): hashlib.sha256(p.read_bytes()).hexdigest()
                for p in sorted(artifacts.rglob("*")) if p.is_file()}
    (artifacts / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    # Neither device credentials nor signing material may occur in any public artifact.
    credential = (private / "credential").read_bytes()
    for path in artifacts.rglob("*"):
        if path.is_file():
            data = path.read_bytes()
            assert credential not in data and b"BEGIN PRIVATE KEY" not in data and b"BEGIN RSA PRIVATE KEY" not in data, path
