import plistlib
import struct
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from check_macos_compatibility import CPU_TYPES, inspect_macho, verify_payloads

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "src/rust/shoopdaloop"))
import package_artifacts
from carla_runtime import normalize, parser as carla_parser


def macho(arch="x86_64", minimum=(10, 15, 0), dependencies=(), modern=True, file_type=2):
    encoded = (minimum[0] << 16) | (minimum[1] << 8) | minimum[2]
    commands = [struct.pack("<6I", 0x32, 24, 1, encoded, encoded, 0) if modern else struct.pack("<4I", 0x24, 16, encoded, encoded)]
    for dependency in dependencies:
        name = dependency.encode() + b"\0"
        commands.append(struct.pack("<6I", 0xC, 24 + len(name), 24, 0, 0, 0) + name)
    return struct.pack("<8I", 0xFEEDFACF, CPU_TYPES[arch], 0, file_type, len(commands), sum(map(len, commands)), 0, 0) + b"".join(commands)


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

    def test_executable_path_in_a_plugin_uses_its_host_directory(self):
        payloads = {"resources/host": macho(), "resources/QtCore": macho(file_type=6), "resources/lib/plugin.so": macho(file_type=8, dependencies=["@executable_path/QtCore"])}
        self.assertEqual(len(verify_payloads(payloads, "x86_64", "10.15")), 3)

    def test_loader_path_cannot_use_an_unrelated_host_directory(self):
        payloads = {"resources/host": macho(), "resources/QtCore": macho(file_type=6), "resources/lib/plugin.so": macho(file_type=8, dependencies=["@loader_path/QtCore"])}
        with self.assertRaisesRegex(ValueError, "unbundled dependency"):
            verify_payloads(payloads, "x86_64", "10.15")


class MacOSPackageMetadataTests(unittest.TestCase):
    def stage_plist(self, minimum=None):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / "shoopdaloop"
            binary.write_bytes(macho())
            stage = root / "stage"
            stage.mkdir()
            with patch.object(package_artifacts, "stage_carla_runtime"):
                args = ("macos", binary, root / "carla", stage)
                if minimum is None:
                    package_artifacts.create_native_stage(*args)
                else:
                    package_artifacts.create_native_stage(*args, minimum)
            return plistlib.loads((stage / "shoopdaloop/ShoopDaLoop.app/Contents/Info.plist").read_bytes())

    def test_catalina_metadata(self):
        self.assertEqual(self.stage_plist("10.15")["LSMinimumSystemVersion"], "10.15")

    def test_arm64_packaging_default_is_preserved(self):
        self.assertEqual(self.stage_plist()["LSMinimumSystemVersion"], "11.0")


class CatalinaRuntimeNormalizationTests(unittest.TestCase):
    def normalize_fixture(self, root, replace):
        upstream = root / "upstream"
        for name in ("lib/libcarla_native-plugin.dylib", "lib/libcrypto.3.dylib", "lib/liblzma.5.dylib", "carla-discovery-native", "carla-bridge-native", "resources/carla-plugin", "resources/carla-plugin-patchbay", "resources/lib/PyQt5/QtDesigner.so", "resources/lib/PyQt5/QtHelp.so"):
            path = upstream / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"upstream")
        dependencies = root / "dependencies"
        for name in ("lib/libcrypto.3.dylib", "lib/liblzma.5.dylib", "licenses/dependency-license.txt"):
            path = dependencies / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"rebuilt for Catalina")
        license = root / "COPYING"
        license.write_text("Carla license")
        output = root / "component"
        args = ["normalize", "--platform", "macos", "--search-root", str(upstream), "--license", str(license), "--output", str(output)]
        if replace:
            args.extend(["--macos-dependency-dir", str(dependencies)])
        normalize(carla_parser().parse_args(args))
        return output

    def test_replacements_cover_native_and_frozen_python_layouts(self):
        with tempfile.TemporaryDirectory() as directory:
            output = self.normalize_fixture(Path(directory), True)
            for library in ("libcrypto.3.dylib", "liblzma.5.dylib"):
                for parent in (output, output / "lib", output / "resources"):
                    self.assertEqual((parent / library).read_bytes(), b"rebuilt for Catalina")
            self.assertTrue((output / "licenses/dependency-license.txt").is_file())
            self.assertFalse((output / "resources/lib/PyQt5/QtDesigner.so").exists())
            self.assertFalse((output / "resources/lib/PyQt5/QtHelp.so").exists())

    def test_existing_runtime_normalization_is_unchanged_without_replacements(self):
        with tempfile.TemporaryDirectory() as directory:
            output = self.normalize_fixture(Path(directory), False)
            self.assertEqual((output / "lib/libcrypto.3.dylib").read_bytes(), b"upstream")
            self.assertTrue((output / "resources/lib/PyQt5/QtDesigner.so").is_file())


if __name__ == "__main__":
    unittest.main()
