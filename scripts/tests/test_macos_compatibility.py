import struct
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from check_macos_compatibility import CPU_TYPES, inspect_macho, verify_payloads


def macho(arch="x86_64", minimum=(10, 15, 0), dependencies=(), modern=True):
    encoded = (minimum[0] << 16) | (minimum[1] << 8) | minimum[2]
    commands = [struct.pack("<6I", 0x32, 24, 1, encoded, encoded, 0) if modern else struct.pack("<4I", 0x24, 16, encoded, encoded)]
    for dependency in dependencies:
        name = dependency.encode() + b"\0"
        commands.append(struct.pack("<6I", 0xC, 24 + len(name), 24, 0, 0, 0) + name)
    return struct.pack("<8I", 0xFEEDFACF, CPU_TYPES[arch], 0, 2, len(commands), sum(map(len, commands)), 0, 0) + b"".join(commands)


def universal(*slices):
    position = 8 + len(slices) * 20
    entries = []
    for arch, data in slices:
        entries.append(struct.pack(">5I", CPU_TYPES[arch], 0, position, len(data), 0))
        position += len(data)
    return struct.pack(">2I", 0xCAFEBABE, len(slices)) + b"".join(entries) + b"".join(data for _, data in slices)


class MacOSCompatibilityTests(unittest.TestCase):
    def test_checks_selected_universal_slice(self):
        binary = universal(("arm64", macho("arm64", (11, 0, 0))), ("x86_64", macho()))
        self.assertEqual(inspect_macho(binary, "x86_64"), ((10, 15, 0), []))
        with self.assertRaisesRegex(ValueError, "newer than 10.15"):
            verify_payloads({"app": binary}, "arm64", "10.15")

    def test_rejects_missing_architecture_and_truncated_headers(self):
        with self.assertRaisesRegex(ValueError, "no x86_64 slice"):
            inspect_macho(macho("arm64"), "x86_64")
        with self.assertRaisesRegex(ValueError, "macOS compatibility audit failed"):
            verify_payloads({"broken": b"\xcf\xfa\xed\xfe"}, "x86_64", "10.15")

    def test_checks_old_minimum_version_command(self):
        self.assertEqual(inspect_macho(macho(minimum=(10, 9, 0), modern=False), "x86_64"), ((10, 9, 0), []))

    def test_rejects_newer_helper_even_when_application_is_compatible(self):
        with self.assertRaisesRegex(ValueError, "helper: requires macOS 11.0.0"):
            verify_payloads({"app": macho(), "helper": macho(minimum=(11, 0, 0))}, "x86_64", "10.15")

    def test_rejects_homebrew_dependency(self):
        with self.assertRaisesRegex(ValueError, "unbundled dependency: /usr/local/"):
            verify_payloads({"app": macho(dependencies=["/usr/local/opt/lib/libfoo.dylib"])}, "x86_64", "10.15")

    def test_accepts_system_and_bundled_dependencies(self):
        payloads = {"bin/app": macho(dependencies=["/usr/lib/libSystem.B.dylib", "@loader_path/../lib/libfoo.dylib"]), "lib/libfoo.dylib": macho(), "readme": b"hello"}
        self.assertEqual(len(verify_payloads(payloads, "x86_64", "10.15")), 2)

    def test_rejects_missing_bundled_dependency(self):
        with self.assertRaisesRegex(ValueError, "unbundled dependency"):
            verify_payloads({"app": macho(dependencies=["@rpath/QtCore.framework/QtCore"])}, "x86_64", "10.15")


if __name__ == "__main__":
    unittest.main()
