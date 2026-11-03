#!/usr/bin/env python3
"""Build the pinned AMD BSP and application without Vitis."""
from concurrent.futures import ThreadPoolExecutor
import argparse
import hashlib
import json
import os
import re
from pathlib import Path
import struct
import subprocess

ROOT = Path(__file__).resolve().parents[2]
PLATFORM = Path(__file__).resolve().parent
SOURCE = ROOT / "work/zynq/embeddedsw"
TINYUSB = ROOT / "work/zynq/tinyusb"
BUILD = ROOT / "work/zynq/build"
PINS = json.loads((PLATFORM / "pins.json").read_text())
REVISION = PINS["embeddedsw"]
SPARSE = ["lib/bsp/standalone", "ThirdParty/bsp/freertos10_xilinx",
          "ThirdParty/sw_services/lwip220", *[
              "XilinxProcessorIPLib/drivers/" + name for name in
              ("emacps", "scugic", "scutimer", "ttcps", "uartps", "common", "sdps")]]


def run(args, **kwargs):
    return subprocess.run([str(a) for a in args], check=True, **kwargs)


def build():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fetch", action="store_true")
    parser.add_argument("--app", choices=["echo", "host"], default="echo")
    parser.add_argument("--credential-file", type=Path)
    parser.add_argument("--fixture", action="store_true")
    parser.add_argument("--usb", action="store_true", help="Use the TinyUSB USB0 MIDI host")
    parser.add_argument("--deployment", action="store_true")
    parser.add_argument("--build-id", default="direct-dev")
    parser.add_argument("--deployment-test-fault", choices=["before-ready", "render", "control", "broker", "handoff-version", "cleanup"])
    args = parser.parse_args()
    if not re.fullmatch(r"[A-Za-z0-9_.-]{1,40}", args.build_id):
        parser.error("--build-id requires 1–40 ASCII letters, digits, dots, dashes or underscores")
    if (args.deployment or args.deployment_test_fault) and args.app != "host":
        parser.error("Deployment is only supported by the host app")
    if args.deployment_test_fault and not args.deployment:
        parser.error("Deployment test faults require --deployment")
    if args.usb and args.app != "host":
        parser.error("USB MIDI requires the host app")
    BUILD.mkdir(parents=True, exist_ok=True)
    if args.fetch:
        SOURCE.parent.mkdir(parents=True, exist_ok=True)
        if not (SOURCE / ".git").exists():
            run(["git", "init", SOURCE])
            run(["git", "-C", SOURCE, "remote", "add", "origin", "https://github.com/Xilinx/embeddedsw.git"])
        run(["git", "-C", SOURCE, "sparse-checkout", "set", *SPARSE])
        run(["git", "-C", SOURCE, "fetch", "--depth=1", "--filter=blob:none", "origin", REVISION])
        run(["git", "-C", SOURCE, "checkout", "--detach", REVISION])
    actual = subprocess.check_output(["git", "-C", SOURCE, "rev-parse", "HEAD"], text=True).strip()
    if actual != REVISION:
        raise SystemExit("AMD source revision differs; run --fetch")
    if subprocess.check_output(["git", "-C", SOURCE, "diff", "--name-only"], text=True).strip():
        raise SystemExit("AMD sources have unrecorded modifications")
    bsp = SOURCE / "lib/bsp/standalone/src"
    kernel = SOURCE / "ThirdParty/bsp/freertos10_xilinx/src/Source"
    lwip = SOURCE / "ThirdParty/sw_services/lwip220/src/lwip-2.2.0"
    port = lwip / "contrib/ports/xilinx"
    drivers = SOURCE / "XilinxProcessorIPLib/drivers"
    includes = [PLATFORM / "include", bsp / "common", bsp / "arm/common",
                bsp / "arm/common/gcc", bsp / "arm/cortexa9", bsp / "arm/cortexa9/gcc",
                kernel / "include", kernel / "portable/GCC/ARM_CA9",
                lwip / "src/include", port / "include", port / "netif"]
    sources = []
    for name in ("emacps", "scugic", "scutimer", "uartps"):
        directory = drivers / name / "src"
        includes.append(directory)
        sources.extend(p for p in directory.glob("*.c") if not p.name.endswith("_g.c"))
    includes.append(drivers / "common/src")
    sources += [bsp / "common" / f for f in ("xil_assert.c", "xil_printf.c", "xil_sutil.c", "xplatform_info.c", "xil_sleepcommon.c")]
    sources += [bsp / "arm/common" / f for f in ("xil_exception.c", "vectors.c", "xil_spinlock.c")]
    sources += [bsp / "arm/cortexa9" / f for f in ("xil_cache.c", "xil_mmu.c", "xtime_l.c", "sleep.c", "usleep.c")]
    sources += list((bsp / "arm/cortexa9/gcc").glob("*.S"))
    sources = [p for p in sources if p.name != "asm_vectors.S"]
    sources += [kernel / f for f in ("tasks.c", "queue.c", "list.c", "timers.c", "event_groups.c", "stream_buffer.c")]
    sources += list((kernel / "portable/GCC/ARM_CA9").glob("*.c"))
    sources += list((kernel / "portable/GCC/ARM_CA9").glob("*.S"))
    sources += [kernel / "portable/MemMang/heap_4.c"]
    sources += list((lwip / "src/core").glob("*.c"))
    sources += list((lwip / "src/core/ipv4").glob("*.c"))
    sources += list((lwip / "src/api").glob("*.c"))
    sources += [lwip / "src/netif/ethernet.c", port / "sys_arch.c", port / "sys_arch_raw.c"]
    sources += [port / "netif" / f for f in ("xadapter.c", "xpqueue.c", "xemacpsif.c", "xemacpsif_dma.c", "xemacpsif_hw.c", "xemacpsif_physpeed.c")]
    sources += [PLATFORM / "platform.c", PLATFORM / (args.app + ".c")]
    if args.usb:
        if args.fetch:
            if not (TINYUSB / ".git").exists():
                run(["git", "init", TINYUSB])
                run(["git", "-C", TINYUSB, "remote", "add", "origin", "https://github.com/hathach/tinyusb.git"])
            run(["git", "-C", TINYUSB, "fetch", "--depth=1", "origin", PINS["tinyusb"]])
            run(["git", "-C", TINYUSB, "checkout", "--detach", PINS["tinyusb"]])
        revision = subprocess.check_output(["git", "-C", TINYUSB, "rev-parse", "HEAD"], text=True).strip()
        dirty = subprocess.check_output(["git", "-C", TINYUSB, "status", "--porcelain"], text=True).strip()
        if revision != PINS["tinyusb"] or dirty:
            raise SystemExit("TinyUSB source differs from its pin or has unrecorded modifications")
        includes += [PLATFORM / "usb", TINYUSB / "src", TINYUSB / "src/portable/ehci"]
        sources += [TINYUSB / "src" / name for name in (
            "tusb.c", "common/tusb_fifo.c", "host/usbh.c", "host/hub.c",
            "class/midi/midi_host.c", "portable/ehci/ehci.c")]
        sources += [PLATFORM / "usb/hcd_zynq.c", PLATFORM / "usb/host.c"]
    if args.deployment:
        deployment = PLATFORM / "deployment"
        includes += [deployment, drivers / "sdps/src"]
        sources += [deployment / "runtime.c", deployment / "boot_state.c", bsp / "common/xil_util.c"]
        sources += [p for p in (drivers / "sdps/src").glob("*.c") if not p.name.endswith("_g.c")]
    if args.app == "host":
        sources.append(PLATFORM / "sockets.c")
    if args.fixture:
        sources.append(PLATFORM / "fixture.c")
    patches = {}
    for original, patch_name in [
        (drivers / "emacps/src/xemacps_intr.c", "emacps-read-clear.patch"),
        (kernel / "portable/GCC/ARM_CA9/portZynq7000.c", "freertos-fpu-irq.patch"),
        *([(TINYUSB / "src/portable/ehci/ehci.c", "tinyusb-zynq-ehci.patch")] if args.usb else []),
    ]:
        patched = BUILD / "patched"
        patch_root = patched / "tinyusb" if original.is_relative_to(TINYUSB) else patched
        origin_root = TINYUSB if original.is_relative_to(TINYUSB) else SOURCE
        patched_source = patch_root / original.relative_to(origin_root)
        patched_source.parent.mkdir(parents=True, exist_ok=True)
        patched_source.write_bytes(original.read_bytes())
        patch = PLATFORM / "patches" / patch_name
        run(["patch", "--batch", "--forward", "-p1", "-d", patch_root, "-i", patch], stdout=subprocess.DEVNULL)
        sources = [patched_source if p == original else p for p in sources]
        patches[patch_name] = hashlib.sha256(patch.read_bytes()).hexdigest()
    rust_library = None
    if args.app == "host":
        if not args.deployment and (args.credential_file is None or len(args.credential_file.read_bytes()) != 32):
            raise SystemExit("Host requires --credential-file containing exactly 32 bytes")
        credential = BUILD / "credential.c"
        definition = "const char shoop_build_identity[] = " + json.dumps(args.build_id) + ";\n"
        if not args.deployment:
            definition += "const unsigned char shoop_credential[32] = {" + ",".join(str(b) for b in args.credential_file.read_bytes()) + "};\n"
        credential.write_text(definition)
        sources.append(credential)
        manifest = PLATFORM / "firmware/Cargo.toml"
        rust_flags = "-D warnings -C target-cpu=cortex-a9"
        cargo = ["cargo", "+" + PINS["rust"], "build", "--locked", "--release", "--target", "armv7a-none-eabihf", "--manifest-path", manifest]
        if args.fixture:
            cargo += ["--features", "fixture"]
        run(cargo, env={**os.environ, "RUSTFLAGS": rust_flags})
        rust_library = PLATFORM / "firmware/target/armv7a-none-eabihf/release/libshoop_zynq_firmware.a"
    cc = os.environ.get("ARM_CC", "arm-none-eabi-gcc")
    version = subprocess.check_output([cc, "-dumpfullversion"], text=True).strip()
    if version != PINS["gcc"]:
        raise SystemExit(f"Expected Arm GCC {PINS['gcc']}, found {version}")
    flags = ["-mcpu=cortex-a9", "-marm", "-mfpu=vfpv3", "-mfloat-abi=hard", "-O2", "-g",
             "-ffunction-sections", "-fdata-sections", "-D__FREERTOS__", "-DPLATFORM_ZYNQ",
             "-DPROCESSOR_LITTLE_ENDIAN", "-D_DEFAULT_SOURCE"]
    flags += ["-include", str(PLATFORM / "include/xparameters.h")]
    if args.fixture:
        flags += ["-DSHOOP_FIXTURE"]
    if args.usb:
        flags += ["-DSHOOP_USB_MIDI"]
    if args.deployment:
        fault = [None, "before-ready", "render", "control", "broker", "handoff-version", "cleanup"].index(args.deployment_test_fault)
        flags += ["-DSHOOP_DEPLOYMENT", "-DSHOOP_DEPLOY_FAULT=" + str(fault)]
    flags += ["-I" + str(p) for p in includes]
    BUILD.mkdir(parents=True, exist_ok=True)

    def compile_one(source):
        obj = BUILD / (hashlib.sha256(str(source).encode()).hexdigest()[:12] + ".o")
        warnings = ["-Wall", "-Wextra", "-Werror"] if source.parent in {PLATFORM, PLATFORM / "deployment", PLATFORM / "usb"} else []
        result = subprocess.run([cc, *flags, *warnings, "-c", str(source), "-o", str(obj)], capture_output=True, text=True)
        if result.returncode:
            return None, str(source.relative_to(ROOT)) + "\n" + result.stderr
        return obj, None

    with ThreadPoolExecutor(max_workers=int(os.environ.get("CARGO_BUILD_JOBS", "5"))) as pool:
        results = list(pool.map(compile_one, sources))
    errors = [error for _, error in results if error]
    if errors:
        raise SystemExit("\n".join(errors))
    objects = [obj for obj, _ in results]
    elf = BUILD / (args.app + ("-usb" if args.usb else "") + ("-fixture" if args.fixture else "") + ("-deployment" if args.deployment else "") +
                   ("-test" if args.deployment_test_fault else "") + ".elf")
    run([cc, *flags[:4], "-nostartfiles", "-Wl,--gc-sections", "-Wl,-z,noexecstack", "-Wl,-Map=" + str(elf.with_suffix(".map")),
         "-T", PLATFORM / "link.ld", "-o", elf, *objects, *([rust_library] if rust_library else []),
         "-Wl,--start-group", "-lc", "-lm", "-lgcc", "-lnosys", "-Wl,--end-group"])
    image = elf.read_bytes()
    nm = subprocess.check_output([cc, "-print-prog-name=nm"], text=True).strip()
    symbols = subprocess.check_output([nm, "-g", elf], text=True)
    if not args.fixture and ("shoop_fixture_" in symbols or b"SHOOP_FPU_PASS" in image):
        raise SystemExit("Fixture hooks leaked into production firmware")
    if args.deployment and "shoop_credential" in symbols:
        raise SystemExit("Generic firmware contains a compiled credential")
    if not args.deployment_test_fault and b"SHOOP_DEPLOY_TEST_" in image:
        raise SystemExit("Deployment fault hooks leaked into production firmware")
    if image[:6] != b"\x7fELF\x01\x01":
        raise SystemExit("Expected little-endian ELF32")
    phoff = struct.unpack_from("<I", image, 28)[0]
    phsize, phcount = struct.unpack_from("<HH", image, 42)
    regions = []
    for index in range(phcount):
        kind, _, address, physical, stored, size, _, _ = struct.unpack_from("<8I", image, phoff + phsize * index)
        if kind != 1:
            continue
        if address != physical or stored > size or not (
            0x00100000 <= address < address + size <= 0x08000000 or
            0x08000000 <= address < address + size <= 0x38000000
        ):
            raise SystemExit(f"Invalid load region {address:#x}+{size:#x}")
        regions.append([address, address + size])
    for first, second in zip(sorted(regions), sorted(regions)[1:]):
        if first[1] > second[0]:
            raise SystemExit("Overlapping ELF load regions")
    elf.with_suffix(".json").write_text(json.dumps({"embeddedsw": REVISION,
        "tinyusb": PINS["tinyusb"] if args.usb else None, "usb_midi": args.usb,
        "compiler": subprocess.check_output([cc, "--version"], text=True).splitlines()[0],
        "sources": len(sources), "patch_sha256": patches,
        "elf_sha256": hashlib.sha256(image).hexdigest(),
        "git_head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "rust_target": "armv7a-none-eabihf" if rust_library else None,
        "fixture": args.fixture, "deployment": args.deployment, "build_id": args.build_id,
        "deployment_test_fault": args.deployment_test_fault,
        "ram_bytes": 1 << 30, "load_regions": regions, "elf": str(elf.relative_to(ROOT))}, indent=2) + "\n")
    print(elf)


if __name__ == "__main__":
    build()
