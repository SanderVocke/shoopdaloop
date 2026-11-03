#!/usr/bin/env python3
"""Build pinned upstream QEMU with the Zynq Cadence watchdog model."""
import argparse
import hashlib
import json
import os
import shutil
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[3]
PLATFORM = Path(__file__).resolve().parent.parent
PINS = json.loads((PLATFORM / "pins.json").read_text())
SOURCE = ROOT / "work/zynq/qemu-upstream"
BUILD = ROOT / "work/zynq/qemu-build"
PATCH = PLATFORM / "patches/qemu-cadence-watchdog.patch"
PYTHON = ROOT / "work/zynq/qemu-venv/bin/python"


def run(arguments, **kwargs):
    return subprocess.run([str(a) for a in arguments], check=True, **kwargs)


def verify_sources(patches, fixture=None):
    with tempfile.TemporaryDirectory() as temporary:
        expected = Path(temporary)
        paths = set()
        for patch in patches:
            entries = subprocess.check_output(['git', 'apply', '--numstat', patch], text=True).splitlines()
            for entry in entries:
                path = entry.split('\t', 2)[2]
                paths.add(path)
                destination = expected / path
                if destination.exists():
                    continue
                base = subprocess.run(['git', '-C', SOURCE, 'show', 'HEAD:' + path], capture_output=True)
                if base.returncode == 0:
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    destination.write_bytes(base.stdout)
            run(['git', 'apply', patch], cwd=expected)
        for path in paths:
            if (SOURCE / path).read_bytes() != (expected / path).read_bytes():
                raise SystemExit('QEMU source differs from the recorded patches: ' + path)
        if fixture:
            target = 'hw/usb/dev-shoop-midi.c'
            paths.add(target)
            if (SOURCE / target).read_bytes() != fixture.read_bytes():
                raise SystemExit('QEMU MIDI fixture differs from its recorded source')
        changed = subprocess.check_output(['git', '-C', SOURCE, 'diff', '--name-only'], text=True).splitlines()
        untracked = subprocess.check_output(['git', '-C', SOURCE, 'ls-files', '--others', '--exclude-standard'], text=True).splitlines()
        if (set(changed) | set(untracked)) - paths:
            raise SystemExit('Unrecorded QEMU source modifications')


def main():
    global SOURCE, BUILD
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fetch", action="store_true")
    parser.add_argument("--usb", action="store_true", help="Include the USB MIDI test fixture and ChipIdea model adaptations")
    args = parser.parse_args()
    if args.usb:
        SOURCE = ROOT / "work/zynq/qemu-usb-upstream"
        BUILD = ROOT / "work/zynq/qemu-usb-build"
    if args.fetch:
        if not PYTHON.exists():
            run([sys.executable, "-m", "venv", PYTHON.parent.parent])
        run([PYTHON, "-m", "pip", "install", "meson==" + PINS["qemu_meson"], "pycotap==" + PINS["qemu_pycotap"]])
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
    usb_patch = PLATFORM / "patches/qemu-usb-midi.patch"
    usb_fixture = PLATFORM / "usb/qemu/dev-midi.c"
    if args.usb:
        applied = subprocess.run(["git", "-C", SOURCE, "apply", "--reverse", "--check", str(usb_patch)],
                                 stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0
        if not applied:
            run(["git", "-C", SOURCE, "apply", "--check", usb_patch])
            run(["git", "-C", SOURCE, "apply", usb_patch])
        shutil.copyfile(usb_fixture, SOURCE / "hw/usb/dev-shoop-midi.c")
    changed = subprocess.check_output(["git", "-C", SOURCE, "diff", "--name-only"], text=True).splitlines()
    allowed = {"hw/arm/xilinx_zynq.c", "hw/watchdog/meson.build", "hw/watchdog/cadence_wdt.c",
               "configs/devices/arm-softmmu/shoop.mak"}
    if args.usb:
        allowed |= {"hw/usb/hcd-ehci.c", "hw/usb/hcd-ehci.h", "hw/usb/chipidea.c", "hw/usb/meson.build",
                    "include/hw/usb/chipidea.h"}
    if set(changed) - allowed:
        raise SystemExit("Unrecorded QEMU source modifications")
    verify_sources([PATCH, *([usb_patch] if args.usb else [])], usb_fixture if args.usb else None)
    BUILD.mkdir(parents=True, exist_ok=True)
    versions = json.loads(subprocess.check_output([PYTHON, "-c", "import importlib.metadata,json; print(json.dumps({p:importlib.metadata.version(p) for p in ['meson','pycotap']}))"], text=True))
    if versions != {"meson": PINS["qemu_meson"], "pycotap": PINS["qemu_pycotap"]}:
        raise SystemExit("Unexpected QEMU Python build tools; use --fetch to install the pins")
    flags = ["--python=" + str(PYTHON), "--target-list=arm-softmmu", "--with-devices-arm=shoop", "--without-default-features",
             "--enable-tcg", "--enable-slirp", "--disable-docs", "--disable-guest-agent",
             "--disable-tools", "--disable-user", "--disable-werror"]
    epoch = subprocess.check_output(["git", "-C", SOURCE, "show", "-s", "--format=%ct", "HEAD"], text=True).strip()
    environment = {**os.environ, "SOURCE_DATE_EPOCH": epoch}
    run([SOURCE / "configure", *flags], cwd=BUILD, env=environment)
    run(["ninja", "-j" + os.environ.get("CARGO_BUILD_JOBS", "5"), "qemu-system-arm"], cwd=BUILD, env=environment)
    binary = BUILD / "qemu-system-arm"
    manifest = {"source": revision, "patch_sha256": hashlib.sha256(PATCH.read_bytes()).hexdigest(),
                "usb_patch_sha256": hashlib.sha256(usb_patch.read_bytes()).hexdigest() if args.usb else None,
                "usb_fixture_sha256": hashlib.sha256(usb_fixture.read_bytes()).hexdigest() if args.usb else None,
                "python_packages": versions,
                "configure": flags, "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                "version": subprocess.check_output([binary, "--version"], text=True).splitlines()[0],
                "glib": subprocess.check_output(["pkg-config", "--modversion", "glib-2.0"], text=True).strip(),
                "slirp": subprocess.check_output(["pkg-config", "--modversion", "slirp"], text=True).strip()}
    (BUILD / "shoop-build.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(binary)


if __name__ == "__main__":
    main()
