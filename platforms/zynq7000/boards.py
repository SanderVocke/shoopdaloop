"""Validated Zynq-7000 board contracts shared by firmware and deployment tools."""
import argparse
import hashlib
import json
import re
from pathlib import Path

PLATFORM = Path(__file__).resolve().parent
DEFAULT = "qemu"
MIB = 1024 * 1024


def names():
    return sorted(p.stem for p in (PLATFORM / "boards").glob("*.json"))


def argument(parser):
    parser.add_argument("--board", choices=names(), default=DEFAULT)


class Board:
    def __init__(self, data, fpga=False):
        self.data = data
        self.name = data["name"]
        self.profile = data["profile"]
        self.memory = data["memory"]
        self.bsp = data["bsp"]
        self.boot = data["boot"]
        self.firmware = data["firmware"]
        if fpga and "fpga" not in data:
            raise ValueError("Board has no FPGA arena profile")
        self.fpga = data["fpga"] if fpga else None
        self.validate()

    @property
    def ram(self):
        return self.memory["ram_mib"] * MIB

    @property
    def platform_end(self):
        return self.memory["platform_mib"] * MIB

    @property
    def rust_end(self):
        return self.platform_end + self.memory["rust_mib"] * MIB

    @property
    def arena_bytes(self):
        """PL-owned DDR at the top of the Rust region, outside the firmware heap."""
        return self.fpga["arena_mib"] * MIB if self.fpga else 0

    @property
    def heap_end(self):
        return self.rust_end - self.arena_bytes

    @property
    def recording_mib(self):
        return self.fpga["recording_mib"] if self.fpga else self.firmware["recording_mib"]

    @property
    def stage(self):
        return self.rust_end

    @property
    def loader_min(self):
        return self.stage + 2 * self.memory["bundle_mib"] * MIB

    def validate(self):
        if self.data["family"] != "zynq7000":
            raise ValueError("Only the Zynq-7000 Cortex-A9 platform is supported")
        if not re.fullmatch(r"[A-Za-z0-9_.-]{1,31}", self.profile):
            raise ValueError("Deployment profile must fit the 32-byte boot record")
        if any(type(v) is not int or v <= 0 for v in self.memory.values()):
            raise ValueError("Memory budgets must be positive integer MiB values")
        if self.memory["platform_mib"] < 64 or self.ram > 1024 * MIB:
            raise ValueError("Invalid Zynq-7000 platform/DDR window")
        if self.memory["bundle_mib"] != 32:
            raise ValueError("The deployment protocol requires a 32 MiB bundle bound")
        if self.loader_min + self.memory["loader_mib"] * MIB > self.ram:
            raise ValueError("Firmware, two staging buffers and loader exceed board DDR")
        fw = self.firmware
        if any(type(fw[k]) is not int or fw[k] <= 0 for k in ("recording_mib", "media_mib", "runtime_reserve_mib")):
            raise ValueError("Firmware budgets must be positive integer MiB values")
        if fw["runtime_reserve_mib"] < 240 or self.memory["loader_mib"] < 64:
            raise ValueError("Runtime and loader reserves are below the supported minimum")
        if self.fpga is not None and (
                any(type(self.fpga.get(k)) is not int or self.fpga[k] <= 0 for k in ("arena_mib", "recording_mib"))
                or self.rust_end > 512 * MIB):
            raise ValueError("FPGA arena budgets must be positive MiB values within HP0-reachable DDR")
        # Reserve covers engine state, transport, metadata and allocator headroom.
        if (self.recording_mib + 3 * fw["media_mib"] + fw["runtime_reserve_mib"]
                > (self.heap_end - self.platform_end) // MIB):
            raise ValueError("Recording, media replacement overlap and runtime reserve exceed Rust arena")
        if fw["audio"] != "dummy" or fw["midi"] not in ("dummy", "usb-optional"):
            raise ValueError("Unsupported audio/MIDI implementation")
        if self.bsp["gem_base"] not in (0xe000b000, 0xe000c000) or self.bsp["uart_base"] not in (0xe0000000, 0xe0001000):
            raise ValueError("Unsupported PS peripheral selection")
        if self.bsp["sd_base"] not in (0xe0100000, 0xe0101000):
            raise ValueError("Unsupported SD controller")
        for field in ("cpu_hz", "watchdog_hz", "sd_hz", "gem_irq"):
            if type(self.bsp[field]) is not int or self.bsp[field] <= 0:
                raise ValueError("Invalid PS clock or interrupt")
        if (set(self.bsp["gem_divisors"]) != {"1000", "100", "10"} or
                any(len(pair) != 2 or any(type(v) is not int or not 1 <= v <= 63 for v in pair)
                    for pair in self.bsp["gem_divisors"].values())):
            raise ValueError("GEM clock divisors must be pairs in 1..63 for each link speed")
        if type(self.bsp["phy_address"]) is not int or not 0 <= self.bsp["phy_address"] <= 31:
            raise ValueError("PHY address must be in 0..31")
        tree = self.boot["device_tree"]
        if (not re.fullmatch(r"zynq-[a-z0-9-]+", tree) or
                self.boot["ps_init"] != f"board/xilinx/zynq/{tree}/ps7_init_gpl.c" or
                not re.fullmatch(r"[0-9a-f]{64}", self.boot["ps_init_sha256"])):
            raise ValueError("Board requires a pinned U-Boot device tree and PS initialization")
        if fw["midi"] == "usb-optional":
            hook = fw["usb_reset"]
            if (not isinstance(hook, str) or not re.fullmatch(r"[a-z0-9_]+", hook) or
                    not (PLATFORM / "boards" / (hook + "_usb.c")).is_file()):
                raise ValueError("USB MIDI requires an implemented board reset hook")

    def build_dir(self, root):
        suffix = "-fpga" if self.fpga else ""
        return root / "work/zynq" / ("build" + suffix if self.name == DEFAULT else f"boards/{self.name}/build{suffix}")

    def boot_dir(self, root):
        return root / "work/zynq" / ("u-boot-build" if self.name == DEFAULT else f"boards/{self.name}/u-boot-build")

    def header(self):
        b = self.bsp
        values = {
            "RAM_BYTES": self.ram, "PLATFORM_START": MIB, "PLATFORM_END": self.platform_end,
            "RUST_END": self.heap_end, "STAGE_ADDRESS": self.stage, "LOADER_MIN": self.loader_min,
            "CPU_HZ": b["cpu_hz"], "WATCHDOG_HZ": b["watchdog_hz"],
            "GEM_BASE": b["gem_base"], "GEM_IRQ": b["gem_irq"],
            "UART_BASE": b["uart_base"], "SD_BASE": b["sd_base"], "SD_HZ": b["sd_hz"],
            "FIXTURE_UART_BASE": 0xe0000000 if b["uart_base"] == 0xe0001000 else 0xe0001000,
            "SD_DETECT": b["sd_detect"], "SD_WP": b["sd_wp"], "SD_BANK": b["sd_bank"],
        }
        for speed, divs in b["gem_divisors"].items():
            for i, value in enumerate(divs):
                values[f"GEM_{speed}_DIV{i}"] = value
        lines = ["/* Generated board contract. */", "#ifndef SHOOP_BOARD_H", "#define SHOOP_BOARD_H"]
        if self.fpga:
            values["FPGA_ARENA_BASE"] = self.heap_end
            values["FPGA_ARENA_BYTES"] = self.arena_bytes
        lines += [f"#define SHOOP_{key} {value:#x}u" for key, value in values.items()]
        lines += [f"#define SHOOP_BOARD_PROFILE {json.dumps(self.profile)}", "#endif", ""]
        return "\n".join(lines)

    def linker(self):
        template = (PLATFORM / "link.ld").read_text()
        return (template.replace("@PLATFORM_LENGTH@", hex(self.platform_end - MIB))
                .replace("@RUST_START@", hex(self.platform_end))
                .replace("@RUST_LENGTH@", hex(self.heap_end - self.platform_end))
                .replace("@RUST_END@", hex(self.heap_end)))

    def generate(self, directory):
        directory.mkdir(parents=True, exist_ok=True)
        (directory / "shoop_board.h").write_text(self.header())
        (directory / "link.ld").write_text(self.linker())
        (directory / "board.json").write_text(json.dumps(self.data, indent=2) + "\n")

    def manifest(self):
        manifest = {"board": self.name, "profile": self.profile,
                    "board_sha256": hashlib.sha256(json.dumps(self.data, sort_keys=True).encode()).hexdigest()}
        if self.fpga:
            manifest["fpga_arena"] = {"base": self.heap_end, "bytes": self.arena_bytes}
        return manifest


def load(name=DEFAULT, fpga=False):
    if name not in names():
        raise ValueError(f"Unknown board: {name}")
    data = json.loads((PLATFORM / "boards" / (name + ".json")).read_text())
    if data["name"] != name:
        raise ValueError("Board filename and name differ")
    return Board(data, fpga)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    argument(parser)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    load(args.board).generate(args.output)
