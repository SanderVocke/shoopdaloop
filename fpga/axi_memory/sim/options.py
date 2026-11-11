"""Shared simulator configuration for endpoint and pipeline runners."""
import hashlib
import os
from pathlib import Path
import subprocess
from axi_memory import AxiMemory, FixedTiming

HERE = Path(__file__).resolve().parents[1]
ROOT = HERE.parents[1]


def add_options(parser, *, legacy=False):
    choices = ["instant", "fixed", "dramsim3"]+(["legacy"] if legacy else [])
    parser.add_argument("--memory-model", choices=choices, default="instant")
    parser.add_argument("--memory-cycles", type=int, default=7)
    parser.add_argument("--dram-library", type=Path, default=ROOT/"work/dramsim3/build/libshoop_dram.so")
    parser.add_argument("--dram-config", type=Path, default=HERE/"configs/zynq7010-ddr3-1066-x16.ini")
    parser.add_argument("--fabric-hz", type=int, default=100_000_000)
    parser.add_argument("--physical-base", type=lambda value: int(value, 0), default=0x01000000)
    parser.add_argument("--axi-data-width", type=int, choices=(32, 64), default=32)


def environment(args):
    if args.memory_cycles < 0 or not 1_000_000 <= args.fabric_hz <= 1_000_000_000:
        raise ValueError("use nonnegative memory cycles and a 1 MHz to 1 GHz fabric clock")
    if not 0 <= args.physical_base <= 0xfffffffc or args.physical_base % 8:
        raise ValueError("physical base must be an eight-byte aligned 32-bit address")
    if args.memory_model == "dramsim3" and (not args.dram_library.is_file() or not args.dram_config.is_file()):
        raise ValueError("DRAMsim3 library/profile missing; run fpga/axi_memory/build_dramsim3.py first")
    return {"AXI_MEMORY_MODEL": args.memory_model, "AXI_MEMORY_CYCLES": str(args.memory_cycles),
            "AXI_DRAM_LIBRARY": str(args.dram_library.resolve()), "AXI_DRAM_CONFIG": str(args.dram_config.resolve()),
            "AXI_FABRIC_HZ": str(args.fabric_hz), "AXI_PHYSICAL_BASE": str(args.physical_base)}


def create_memory(dut, words, *, seed=0, word_only=False, output=None):
    mode = os.environ.get("AXI_MEMORY_MODEL", "instant")
    if mode == "dramsim3":
        from dram_timing import DramTiming
        timing = DramTiming(library=os.environ["AXI_DRAM_LIBRARY"], config=os.environ["AXI_DRAM_CONFIG"],
                            fabric_hz=int(os.environ["AXI_FABRIC_HZ"]), output=Path(output or ".")/f"dram-{seed}")
    else:
        timing = FixedTiming(int(os.environ.get("AXI_MEMORY_CYCLES", "7")) if mode == "fixed" else 0)
    memory = AxiMemory(dut, words, timing=timing, seed=seed,
                       base=int(os.environ.get("AXI_PHYSICAL_BASE", str(0x01000000))), word_only=word_only)
    if hasattr(timing, "capacity") and memory.base+len(words)*4 > timing.capacity:
        memory.close()
        raise ValueError("physical arena exceeds configured DDR capacity")
    return memory


def metadata():
    profile = Path(os.environ.get("AXI_DRAM_CONFIG", HERE/"configs/zynq7010-ddr3-1066-x16.ini"))
    library = Path(os.environ.get("AXI_DRAM_LIBRARY", "missing"))
    digest = lambda path: hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else None
    return dict(model=os.environ.get("AXI_MEMORY_MODEL", "instant"),
                fabric_hz=int(os.environ.get("AXI_FABRIC_HZ", "100000000")),
                physical_base=int(os.environ.get("AXI_PHYSICAL_BASE", str(0x01000000))),
                fixed_cycles=int(os.environ.get("AXI_MEMORY_CYCLES", "7")),
                profile=str(profile), profile_sha256=digest(profile), library_sha256=digest(library),
                model_revision=dict(line.split("=", 1) for line in (HERE/"dramsim3.lock").read_text().splitlines())["revision"],
                revision=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip())
