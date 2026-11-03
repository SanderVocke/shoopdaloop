#!/usr/bin/env python3
"""Build pinned upstream QEMU with the Zynq Cadence watchdog model."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[3]
PLATFORM = Path(__file__).resolve().parent.parent
PINS = json.loads((PLATFORM / "pins.json").read_text())
SOURCE = ROOT / "work/zynq/qemu-upstream"
BUILD = ROOT / "work/zynq/qemu-build"
PATCH = PLATFORM / "patches/qemu-cadence-watchdog.patch"


def run(arguments, **kwargs):
    return subprocess.run([str(a) for a in arguments], check=True, **kwargs)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fetch", action="store_true")
    args = parser.parse_args()
    if args.fetch:
        if not (SOURCE / ".git").exists():
            SOURCE.parent.mkdir(parents=True, exist_ok=True)
            run(["git", "init", SOURCE])
            run(["git", "-C", SOURCE, "remote", "add", "origin", "https://github.com/qemu/qemu.git"])
        run(["git", "-C", SOURCE, "fetch", "--depth=1", "origin", PINS["qemu_source"]])
        if not (SOURCE / "configure").exists():
            run(["git", "-C", SOURCE, "checkout", "--detach", PINS["qemu_source"]])
    revision = subprocess.check_output(["git", "-C", SOURCE, "rev-parse", "HEAD"], text=True).strip()
    if revision != PINS["qemu_source"]:
        raise SystemExit("Unexpected QEMU source revision; use the pinned checkout")
    applied = subprocess.run(["git", "-C", SOURCE, "apply", "--reverse", "--check", str(PATCH)],
                             stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0
    if not applied:
        run(["git", "-C", SOURCE, "apply", "--check", PATCH])
        run(["git", "-C", SOURCE, "apply", PATCH])
    changed = subprocess.check_output(["git", "-C", SOURCE, "diff", "--name-only"], text=True).splitlines()
    allowed = {"hw/arm/xilinx_zynq.c", "hw/watchdog/meson.build", "hw/watchdog/cadence_wdt.c",
               "configs/devices/arm-softmmu/shoop.mak"}
    if set(changed) - allowed:
        raise SystemExit("Unrecorded QEMU source modifications")
    BUILD.mkdir(parents=True, exist_ok=True)
    flags = ["--target-list=arm-softmmu", "--with-devices-arm=shoop", "--without-default-features",
             "--enable-tcg", "--enable-slirp", "--disable-docs", "--disable-guest-agent",
             "--disable-tools", "--disable-user", "--disable-werror"]
    epoch = subprocess.check_output(["git", "-C", SOURCE, "show", "-s", "--format=%ct", "HEAD"], text=True).strip()
    environment = {**os.environ, "SOURCE_DATE_EPOCH": epoch}
    run([SOURCE / "configure", *flags], cwd=BUILD, env=environment)
    run(["ninja", "-j" + os.environ.get("CARGO_BUILD_JOBS", "5"), "qemu-system-arm"], cwd=BUILD, env=environment)
    binary = BUILD / "qemu-system-arm"
    manifest = {"source": revision, "patch_sha256": hashlib.sha256(PATCH.read_bytes()).hexdigest(),
                "configure": flags, "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                "version": subprocess.check_output([binary, "--version"], text=True).splitlines()[0],
                "glib": subprocess.check_output(["pkg-config", "--modversion", "glib-2.0"], text=True).strip(),
                "slirp": subprocess.check_output(["pkg-config", "--modversion", "slirp"], text=True).strip()}
    (BUILD / "shoop-build.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(binary)


if __name__ == "__main__":
    main()
