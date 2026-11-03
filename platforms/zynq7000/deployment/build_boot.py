#!/usr/bin/env python3
"""Build pinned Zynq SPL and the restricted Shoop U-Boot updater."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

DEPLOYMENT = Path(__file__).resolve().parent
ROOT = DEPLOYMENT.parents[2]
PINS = json.loads((DEPLOYMENT.parent / "pins.json").read_text())
SOURCE = ROOT / "work/zynq/u-boot"
BUILD = ROOT / "work/zynq/u-boot-build"
PATCH = DEPLOYMENT.parent / "patches/uboot-network-deployment.patch"


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
            run(["git", "-C", SOURCE, "remote", "add", "origin", "https://github.com/u-boot/u-boot.git"])
        run(["git", "-C", SOURCE, "fetch", "--depth=1", "origin", PINS["u_boot"]])
        if not (SOURCE / "Makefile").exists():
            run(["git", "-C", SOURCE, "checkout", "--detach", PINS["u_boot"]])
    revision = subprocess.check_output(["git", "-C", SOURCE, "rev-parse", "HEAD"], text=True).strip()
    if revision != PINS["u_boot"]:
        raise SystemExit("U-Boot source differs from the pinned revision")
    changes = subprocess.check_output(["git", "-C", SOURCE, "diff", "--name-only"], text=True).splitlines()
    if set(changes) - {"cmd/Makefile", "net/fastboot_udp.c", "include/fastboot.h", "drivers/fastboot/fb_command.c", "drivers/net/zynq_gem.c"}:
        raise SystemExit("Unrecorded U-Boot source changes")
    applied = subprocess.run(["git", "-C", SOURCE, "apply", "--reverse", "--check", PATCH], capture_output=True)
    if applied.returncode:
        run(["git", "-C", SOURCE, "apply", "--check", PATCH])
        run(["git", "-C", SOURCE, "apply", PATCH])
    copies = {"uboot.c": "cmd/shoop.c", "boot_state.c": "cmd/shoop_boot_state.c",
              "shoop_boot_state.h": "include/shoop_boot_state.h", "shoop_layout.h": "include/shoop_layout.h"}
    for source, target in copies.items():
        shutil.copyfile(DEPLOYMENT / source, SOURCE / target)
    original = subprocess.check_output(["git", "-C", SOURCE, "show", "HEAD:cmd/Makefile"], text=True)
    (SOURCE / "cmd/Makefile").write_text(original + "\nobj-y += shoop.o shoop_boot_state.o\n")
    epoch = subprocess.check_output(["git", "-C", SOURCE, "show", "-s", "--format=%ct", "HEAD"], text=True).strip()
    environment = {**os.environ, "SOURCE_DATE_EPOCH": epoch}
    make = ["make", "-C", SOURCE, "O=" + str(BUILD), "CROSS_COMPILE=" + os.environ.get("ARM_CROSS_COMPILE", "arm-none-eabi-"), "DEVICE_TREE=zynq-zc702"]
    run([*make, "xilinx_zynq_virt_defconfig"], env=environment)
    config = [SOURCE / "scripts/config", "--file", BUILD / ".config"]
    enable = ["NET_LEGACY", "UDP_FUNCTION_FASTBOOT", "FASTBOOT_OEM_BOARD", "USE_BOOTCOMMAND", "ENV_IS_NOWHERE", "ENV_OVERWRITE", "CMD_ELF"]
    disable = ["NET_LWIP", "EFI_LOADER", "TOOLS_MKEFICAPSULE", "FASTBOOT_FLASH", "FASTBOOT_OEM_RUN", "FASTBOOT_UUU_SUPPORT",
               "FASTBOOT_CMD_OEM_CONSOLE", "USB_FUNCTION_FASTBOOT", "ENV_IS_IN_FAT", "ENV_IS_IN_SPI_FLASH",
               "ENV_IS_IN_NAND", "ENV_IS_IN_MMC", "ENV_IS_IN_EXT4", "USE_PREBOOT"]
    strings = {"DEFAULT_DEVICE_TREE": "zynq-zc702", "OF_LIST": "zynq-zc702", "SPL_OF_LIST": "zynq-zc702", "BOOTCOMMAND": "shoop boot"}
    for name in disable:
        run([*config, "--disable", name])
    for name in enable:
        run([*config, "--enable", name])
    for name, value in strings.items():
        run([*config, "--set-str", name, value])
    for name, value in {"FASTBOOT_BUF_ADDR": "0x38000000", "FASTBOOT_BUF_SIZE": "0x02000000", "SYS_LOAD_ADDR": "0x38000000"}.items():
        run([*config, "--set-val", name, value])
    run([*config, "--set-val", "BOOTDELAY", "0"])
    run([*make, "olddefconfig"], env=environment)
    run([*make, "-j" + os.environ.get("CARGO_BUILD_JOBS", "5")], env=environment)
    manifest = {"source": revision, "profile": "zynq-zc702", "first_stage": "U-Boot Zynq SPL",
                "patch_sha256": hashlib.sha256(PATCH.read_bytes()).hexdigest(),
                "reference_ps_init": "board/xilinx/zynq/zynq-zc702/ps7_init_gpl.c",
                "source_sha256": {source: hashlib.sha256((DEPLOYMENT / source).read_bytes()).hexdigest() for source in copies},
                "config_sha256": hashlib.sha256((BUILD / ".config").read_bytes()).hexdigest(),
                "files": {name: hashlib.sha256((BUILD / name).read_bytes()).hexdigest() for name in ["u-boot.bin", "u-boot-nodtb.bin", "u-boot.dtb", "spl/boot.bin", "spl/u-boot-spl"]}}
    (BUILD / "shoop-build.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(BUILD)


if __name__ == "__main__":
    main()
