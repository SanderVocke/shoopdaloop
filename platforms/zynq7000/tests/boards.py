#!/usr/bin/env python3
"""Exercise board isolation, memory admission and the shared C boot record ABI."""
import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

PLATFORM = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PLATFORM))
sys.path.insert(0, str(PLATFORM / "deployment"))
import boards
import bundle
import records
from layout import header


class Contracts(unittest.TestCase):
    def test_fpga_trace_rejects_memory_overcommit_before_building(self):
        result = subprocess.run([sys.executable, PLATFORM / "build.py", "--app", "host", "--board", "zedboard",
                                 "--fpga", "--fpga-trace", "2000000"], capture_output=True, text=True)
        self.assertEqual(result.returncode, 2)
        self.assertIn("remaining Rust memory budget", result.stderr)

    def test_fpga_arena_is_outside_the_heap_and_builds_are_isolated(self):
        board = boards.load("zedboard", fpga=True)
        self.assertEqual((board.heap_end, board.arena_bytes), (0x17800000, 8 * boards.MIB))
        self.assertNotEqual(board.build_dir(Path("/tmp")), boards.load("zedboard").build_dir(Path("/tmp")))
        self.assertIn("#define SHOOP_RUST_END 0x17800000u", board.header())
        self.assertIn("#define SHOOP_FPGA_ARENA_BASE 0x17800000u", board.header())
        with self.assertRaisesRegex(ValueError, "no FPGA arena"):
            boards.load("qemu", fpga=True)
        spec = importlib.util.spec_from_file_location("zedboard_host", PLATFORM.parents[1] / "fpga/zedboard/configure_host.py")
        host = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(host)
        with tempfile.TemporaryDirectory() as temporary:
            configured = host.configure(temporary)
            self.assertEqual(configured["arena_base"], board.heap_end)
            self.assertLessEqual(configured["arena_bytes"], board.arena_bytes)
            self.assertIn(f".PHYSICAL_BASE(32'h{board.heap_end:08x})", configured["top"].read_text())

    def test_reference_layout_and_zedboard_fit(self):
        reference, zed = boards.load(), boards.load("zedboard")
        self.assertEqual((reference.platform_end, reference.rust_end, reference.stage, reference.loader_min),
                         (0x08000000, 0x38000000, 0x38000000, 0x3c000000))
        self.assertEqual((zed.ram, zed.platform_end, zed.rust_end, zed.loader_min),
                         (0x20000000, 0x04000000, 0x18000000, 0x1c000000))
        self.assertNotEqual(reference.profile, zed.profile)
        self.assertNotEqual(reference.build_dir(Path("/tmp")), zed.build_dir(Path("/tmp")))
        self.assertNotEqual(reference.boot_dir(Path("/tmp")), zed.boot_dir(Path("/tmp")))
        self.assertEqual(header(), (PLATFORM / "deployment/shoop_layout.h").read_text())

    def test_reject_overcommit_and_incompatible_platforms(self):
        for section, key, value in [("memory", "rust_mib", 768),
                                    ("memory", "loader_mib", 128),
                                    ("firmware", "recording_mib", 128),
                                    ("firmware", "media_mib", 64),
                                    ("firmware", "audio", "codec"),
                                    ("boot", "ps_init", "../unreviewed.c")]:
            with self.subTest(section=section, key=key):
                data = copy.deepcopy(boards.load("zedboard").data)
                data[section][key] = value
                with self.assertRaises(ValueError):
                    boards.Board(data)
        data = copy.deepcopy(boards.load().data)
        data["family"] = "zynqmp"
        with self.assertRaises(ValueError):
            boards.Board(data)

    def test_boot_records_for_each_board(self):
        for name in boards.names():
            with self.subTest(board=name), tempfile.TemporaryDirectory() as temporary:
                output = Path(temporary)
                board = boards.load(name)
                board.generate(output)
                executable = output / "vectors"
                subprocess.run(["cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-I", output,
                                "-I", PLATFORM / "deployment", PLATFORM / "deployment/tests/vectors.c",
                                PLATFORM / "deployment/boot_state.c", "-o", executable], check=True)
                encoded = subprocess.check_output([executable])[:512]
                expected = records.configuration(bytes(range(32)), "02:00:00:00:00:01", "10.0.2.15",
                                                 "255.255.255.0", "10.0.2.2", "10.0.2.2", board.profile)
                self.assertEqual(encoded, expected)
                other = boards.load("zedboard" if name == boards.DEFAULT else boards.DEFAULT)
                foreign = records.configuration(bytes(range(32)), "02:00:00:00:00:01", "10.0.2.15",
                                                "255.255.255.0", "10.0.2.2", "10.0.2.2", other.profile)
                (output / "foreign.bin").write_bytes(foreign)
                # The same production validator must refuse a CRC-valid foreign configuration.
                (output / "check.c").write_text(
                    '#include "shoop_boot_state.h"\n#include <stdio.h>\n#include <string.h>\n'
                    'static int read_config(void *c, uint32_t lba, void *out) { '
                    '(void)lba; memcpy(out, c, 512); return 0; }\n'
                    'int main(void) { struct shoop_config c; '
                    'if (fread(&c, 1, sizeof c, stdin) != sizeof c) return 2; '
                    'struct shoop_io io = { &c, read_config, NULL }; struct shoop_config out; '
                    'return shoop_config_load(&io, &out) == SHOOP_INVALID ? 0 : 1; }\n')
                subprocess.run(["cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-I", output,
                                "-I", PLATFORM / "deployment", output / "check.c",
                                PLATFORM / "deployment/boot_state.c", "-o", output / "check"], check=True)
                result = subprocess.run([output / "check"], input=foreign)
                self.assertEqual(result.returncode, 0)
                state_test = output / "boot-state"
                subprocess.run(["cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-I", output,
                                "-I", PLATFORM / "deployment", PLATFORM / "deployment/tests/boot_state.c",
                                PLATFORM / "deployment/boot_state.c", "-o", state_test], check=True)
                subprocess.run([state_test], check=True)

    def test_bundle_rejects_foreign_loader_before_signing(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            boot = boards.load("zedboard").boot_dir(root)
            boot.mkdir(parents=True)
            (boot / "shoop-build.json").write_text(json.dumps(boards.load().manifest()))
            original = bundle.ROOT
            try:
                bundle.ROOT = root
                with self.assertRaisesRegex(ValueError, "Loader does not match"):
                    bundle.create(root / "host.elf", "test", root, root / "host.itb", root / "loader",
                                  boards.load("zedboard"))
            finally:
                bundle.ROOT = original


if __name__ == "__main__":
    unittest.main()
