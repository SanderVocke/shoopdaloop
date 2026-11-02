#!/usr/bin/env python3
"""Build the pinned AMD BSP and application without Vitis."""
from concurrent.futures import ThreadPoolExecutor
import argparse
import hashlib
import json
import os
from pathlib import Path
import struct
import subprocess

ROOT = Path(__file__).resolve().parents[2]
PLATFORM = Path(__file__).resolve().parent
SOURCE = ROOT / "work/zynq/embeddedsw"
BUILD = ROOT / "work/zynq/build"
PINS = json.loads((PLATFORM / "pins.json").read_text())
REVISION = PINS["embeddedsw"]
SPARSE = ["lib/bsp/standalone", "ThirdParty/bsp/freertos10_xilinx",
          "ThirdParty/sw_services/lwip220", *[
              "XilinxProcessorIPLib/drivers/" + name for name in
              ("emacps", "scugic", "scutimer", "ttcps", "uartps", "common")]]


def run(args, **kwargs):
    return subprocess.run([str(a) for a in args], check=True, **kwargs)


def build():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fetch", action="store_true")
    args = parser.parse_args()
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
    patched = BUILD / "patched"
    original = drivers / "emacps/src/xemacps_intr.c"
    patched_source = patched / original.relative_to(SOURCE)
    patched_source.parent.mkdir(parents=True, exist_ok=True)
    patched_source.write_bytes(original.read_bytes())
    patch = PLATFORM / "patches/emacps-read-clear.patch"
    run(["patch", "--batch", "--forward", "-p1", "-d", patched, "-i", patch], stdout=subprocess.DEVNULL)
    sources = [patched_source if p == original else p for p in sources]
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
    sources += [PLATFORM / "platform.c", PLATFORM / "echo.c"]
    cc = os.environ.get("ARM_CC", "arm-none-eabi-gcc")
    version = subprocess.check_output([cc, "-dumpfullversion"], text=True).strip()
    if version != PINS["gcc"]:
        raise SystemExit(f"Expected Arm GCC {PINS['gcc']}, found {version}")
    flags = ["-mcpu=cortex-a9", "-marm", "-mfpu=vfpv3", "-mfloat-abi=hard", "-O2", "-g",
             "-ffunction-sections", "-fdata-sections", "-D__FREERTOS__", "-DPLATFORM_ZYNQ",
             "-DPROCESSOR_LITTLE_ENDIAN", "-D_DEFAULT_SOURCE"]
    flags += ["-include", str(PLATFORM / "include/xparameters.h")]
    flags += ["-I" + str(p) for p in includes]
    BUILD.mkdir(parents=True, exist_ok=True)

    def compile_one(source):
        obj = BUILD / (hashlib.sha256(str(source).encode()).hexdigest()[:12] + ".o")
        warnings = ["-Wall", "-Wextra", "-Werror"] if source.parent == PLATFORM else []
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
    elf = BUILD / "echo.elf"
    run([cc, *flags[:4], "-nostartfiles", "-Wl,--gc-sections", "-Wl,-Map=" + str(elf.with_suffix(".map")),
         "-T", PLATFORM / "link.ld", "-o", elf, *objects,
         "-Wl,--start-group", "-lc", "-lm", "-lgcc", "-lnosys", "-Wl,--end-group"])
    image = elf.read_bytes()
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
            0x08000000 <= address < address + size <= 0x28000000
        ):
            raise SystemExit(f"Invalid load region {address:#x}+{size:#x}")
        regions.append([address, address + size])
    for first, second in zip(sorted(regions), sorted(regions)[1:]):
        if first[1] > second[0]:
            raise SystemExit("Overlapping ELF load regions")
    (BUILD / "build.json").write_text(json.dumps({"embeddedsw": REVISION,
        "compiler": subprocess.check_output([cc, "--version"], text=True).splitlines()[0],
        "sources": len(sources), "patch_sha256": hashlib.sha256(patch.read_bytes()).hexdigest(),
        "ram_bytes": 1 << 30, "load_regions": regions, "elf": str(elf.relative_to(ROOT))}, indent=2) + "\n")
    print(elf)


if __name__ == "__main__":
    build()
