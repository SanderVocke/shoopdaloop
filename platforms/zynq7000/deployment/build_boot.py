#!/usr/bin/env python3
"""Build pinned Zynq SPL and the restricted Shoop U-Boot updater."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import boards
import fpga_pair

DEPLOYMENT = Path(__file__).resolve().parent
ROOT = DEPLOYMENT.parents[2]
PINS = json.loads((DEPLOYMENT.parent / "pins.json").read_text())
SOURCE = ROOT / "work/zynq/u-boot"
BUILD = ROOT / "work/zynq/u-boot-build"
PATCH = DEPLOYMENT.parent / "patches/uboot-network-deployment.patch"
IDLE_PATCH = DEPLOYMENT.parent / "patches/uboot-fastboot-idle.patch"
ZTURN_V2_PHY_PATCH = DEPLOYMENT.parent / "patches/uboot-zturn-v2-phy.patch"


def run(arguments, **kwargs):
    return subprocess.run([str(a) for a in arguments], check=True, **kwargs)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fetch", action="store_true")
    parser.add_argument("--fpga-build", type=Path, help="Use the verified PS initialization from this Vivado build")
    parser.add_argument("--directory", type=Path, help="Use a separate loader build directory")
    boards.argument(parser)
    args = parser.parse_args()
    board = boards.load(args.board)
    global BUILD, SOURCE
    BUILD = (args.directory or board.boot_dir(ROOT)).resolve()
    board.generate(BUILD)
    if args.fetch:
        if not (SOURCE / ".git").exists():
            SOURCE.parent.mkdir(parents=True, exist_ok=True)
            run(["git", "init", SOURCE])
            run(["git", "-C", SOURCE, "remote", "add", "origin", "https://github.com/u-boot/u-boot.git"])
        run(["git", "-C", SOURCE, "fetch", "--depth=1", "origin", PINS["u_boot"]])
        if not (SOURCE / "Makefile").exists():
            run(["git", "-C", SOURCE, "checkout", "--detach", PINS["u_boot"]])
    if board.name != boards.DEFAULT or args.directory:
        shared = SOURCE
        SOURCE = BUILD.with_name(BUILD.name + "-source") if args.directory else BUILD.parent / "u-boot-source"
        if not (SOURCE / ".git").exists():
            run(["git", "clone", "--shared", "--no-checkout", shared, SOURCE])
            run(["git", "-C", SOURCE, "checkout", "--detach", PINS["u_boot"]])
    revision = subprocess.check_output(["git", "-C", SOURCE, "rev-parse", "HEAD"], text=True).strip()
    if revision != PINS["u_boot"]:
        raise SystemExit("U-Boot source differs from the pinned revision")
    expected_ps = {board.boot["ps_init"]: board.boot["ps_init_sha256"],
                   "arch/arm/mach-zynq/include/mach/ps7_init_gpl.h": PINS["reference_ps_init_sha256"]["arch/arm/mach-zynq/include/mach/ps7_init_gpl.h"]}
    reference = {path: hashlib.sha256((SOURCE / path).read_bytes()).hexdigest() for path in expected_ps}
    if reference != expected_ps:
        raise SystemExit("Reference PS initialization differs from its recorded inputs")
    fpga_ps = None
    if args.fpga_build is not None:
        if not board.pl_bytes:
            parser.error("FPGA PS initialization requires a board with a PL arena")
        manifest = json.loads((args.fpga_build / "fpga-build.json").read_text())
        xsa = args.fpga_build / "zturn_v2.xsa"
        geometry = args.fpga_build / "geometry.txt"
        abi = fpga_pair.NATIVE_AUDIO_ABI if geometry.read_text().strip() == "native" else fpga_pair.RTL_ABI
        if (manifest["contract"] != fpga_pair.contract(board, abi)
                or fpga_pair.digest(geometry) != manifest["sha256"]["geometry.txt"]
                or fpga_pair.digest(xsa) != manifest["sha256"]["zturn_v2.xsa"]):
            raise SystemExit("XSA and board do not match the FPGA build")
        generated = BUILD / "fpga-ps"
        generated.mkdir(exist_ok=True)
        sources = fpga_pair.ps_init_sources(xsa)
        for name, source in sources.items():
            # Vivado 2021.1 emits non-prototype declarations that current U-Boot rejects.
            functions = rb"(?:ps7_init|ps7_post_config|ps7_debug|ps7GetSiliconVersion|perf_reset_and_start_timer)"
            prototype = rb"(?m)^((?:(?:int|void)[ \t]+)?" + functions + rb")[ \t]*\(\)"
            (generated / name).write_bytes(re.sub(prototype, rb"\1(void)", source))
        fpga_ps = {"xsa_sha256": fpga_pair.digest(xsa),
                   "contract": fpga_pair.ps_contract(board, sources),
                   "xsa_sources": {name: hashlib.sha256(source).hexdigest() for name, source in sources.items()},
                   "sources": {name: fpga_pair.digest(generated / name) for name in sources}}
    changes = subprocess.check_output(["git", "-C", SOURCE, "diff", "--name-only"], text=True).splitlines()
    allowed = {"cmd/Makefile", "net/fastboot_udp.c", "include/fastboot.h", "include/net/fastboot_udp.h",
               "drivers/fastboot/fb_command.c", "drivers/net/zynq_gem.c"}
    board_patch = ZTURN_V2_PHY_PATCH if board.name == "zturn-v2" else None
    if board_patch:
        allowed.add("arch/arm/dts/zynq-zturn.dts")
    if set(changes) - allowed:
        raise SystemExit("Unrecorded U-Boot source changes")
    for patch in [PATCH, IDLE_PATCH] + ([board_patch] if board_patch else []):
        applied = subprocess.run(["git", "-C", SOURCE, "apply", "--reverse", "--check", patch], capture_output=True)
        if applied.returncode:
            run(["git", "-C", SOURCE, "apply", "--check", patch])
            run(["git", "-C", SOURCE, "apply", patch])
    copies = {"uboot.c": "cmd/shoop.c", "boot_state.c": "cmd/shoop_boot_state.c",
              "shoop_boot_state.h": "include/shoop_boot_state.h", "shoop_layout.h": "include/shoop_layout.h"}
    for source, target in copies.items():
        shutil.copyfile(DEPLOYMENT / source, SOURCE / target)
    (SOURCE / "include/shoop_board.h").write_text(board.header())
    (SOURCE / "include/shoop_ps_contract.h").write_text(
        f'#define SHOOP_PS_CONTRACT "{fpga_ps["contract"]}"\n' if fpga_ps else "")
    original = subprocess.check_output(["git", "-C", SOURCE, "show", "HEAD:cmd/Makefile"], text=True)
    (SOURCE / "cmd/Makefile").write_text(original + "\nobj-y += shoop.o shoop_boot_state.o\n")
    epoch = subprocess.check_output(["git", "-C", SOURCE, "show", "-s", "--format=%ct", "HEAD"], text=True).strip()
    environment = {**os.environ, "SOURCE_DATE_EPOCH": epoch}
    make = ["make", "-C", SOURCE, "O=" + str(BUILD), "CROSS_COMPILE=" + os.environ.get("ARM_CROSS_COMPILE", "arm-none-eabi-"), "DEVICE_TREE=" + board.boot["device_tree"]]
    run([*make, "xilinx_zynq_virt_defconfig"], env=environment)
    config = [SOURCE / "scripts/config", "--file", BUILD / ".config"]
    enable = ["LMB", "NET_LEGACY", "UDP_FUNCTION_FASTBOOT", "FASTBOOT_OEM_BOARD", "USE_BOOTCOMMAND", "ENV_IS_NOWHERE", "ENV_OVERWRITE", "CMD_ELF"]
    if board_patch:
        enable.append("PHY_MOTORCOMM")
    disable = ["NET_LWIP", "EFI_LOADER", "TOOLS_MKEFICAPSULE", "FASTBOOT_FLASH", "FASTBOOT_OEM_RUN", "FASTBOOT_UUU_SUPPORT",
               "FASTBOOT_CMD_OEM_CONSOLE", "USB_FUNCTION_FASTBOOT", "ENV_IS_IN_FAT", "ENV_IS_IN_SPI_FLASH",
               "ENV_IS_IN_NAND", "ENV_IS_IN_MMC", "ENV_IS_IN_EXT4", "USE_PREBOOT"]
    strings = {"DEFAULT_DEVICE_TREE": board.boot["device_tree"], "OF_LIST": board.boot["device_tree"], "SPL_OF_LIST": board.boot["device_tree"], "BOOTCOMMAND": "shoop boot"}
    if fpga_ps is not None:
        strings["XILINX_PS_INIT_FILE"] = str(generated / "ps7_init_gpl.c")
    for name in disable:
        run([*config, "--disable", name])
    for name in enable:
        run([*config, "--enable", name])
    for name, value in strings.items():
        run([*config, "--set-str", name, value])
    for name, value in {"FASTBOOT_BUF_ADDR": hex(board.stage), "FASTBOOT_BUF_SIZE": hex(board.memory["bundle_mib"] * boards.MIB), "SYS_LOAD_ADDR": hex(board.stage)}.items():
        run([*config, "--set-val", name, value])
    run([*config, "--set-val", "BOOTDELAY", "0"])
    run([*make, "olddefconfig"], env=environment)
    run([*make, "-j" + os.environ.get("CARGO_BUILD_JOBS", "5")], env=environment)
    manifest = {**board.manifest(), "source": revision, "device_tree": board.boot["device_tree"], "first_stage": "U-Boot Zynq SPL",
                "patch_sha256": hashlib.sha256(PATCH.read_bytes()).hexdigest(),
                "idle_patch_sha256": hashlib.sha256(IDLE_PATCH.read_bytes()).hexdigest(),
                "reference_ps_init": board.boot["ps_init"],
                "reference_ps_init_sha256": reference,
                "source_sha256": {source: hashlib.sha256((DEPLOYMENT / source).read_bytes()).hexdigest() for source in copies},
                "config_sha256": hashlib.sha256((BUILD / ".config").read_bytes()).hexdigest(),
                "files": {name: hashlib.sha256((BUILD / name).read_bytes()).hexdigest() for name in ["u-boot.bin", "u-boot-nodtb.bin", "u-boot.dtb", "spl/boot.bin", "spl/u-boot-spl"]}}
    if board_patch:
        manifest["board_patch_sha256"] = hashlib.sha256(board_patch.read_bytes()).hexdigest()
    if fpga_ps is not None:
        manifest["fpga_ps"] = fpga_ps
    (BUILD / "shoop-build.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(BUILD)


if __name__ == "__main__":
    main()
