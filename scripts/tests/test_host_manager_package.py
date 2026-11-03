import os
import plistlib
from pathlib import Path
import shutil
import sys
import tarfile
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "src/rust/shoopdaloop"))
import package_artifacts


class HostManagerPackageTests(unittest.TestCase):
    def test_macos_app_entry_survives_archive_relocation_and_shares_companions(self):
        if os.name == "nt":
            self.skipTest("macOS archive symlinks are exercised on Unix runners")
        with tempfile.TemporaryDirectory(prefix="Shoop bundle Δ ") as directory:
            root = Path(directory)
            build = root / "build"
            build.mkdir()
            for name in ("shoopdaloop", "shoop_host", "shoop_host_manager"):
                (build / name).write_bytes(name.encode())
            stage = root / "stage"
            stage.mkdir()
            package_artifacts.create_native_stage("macos", build / "shoopdaloop", stage)
            bundle = root / "package.tar.gz"
            package_artifacts.write_tar(stage / "shoopdaloop", bundle)
            shutil.rmtree(stage)
            relocated = root / "relocated bundle"
            relocated.mkdir()
            with tarfile.open(bundle) as archive:
                archive.extractall(relocated, filter="data")
            contents = relocated / "shoopdaloop/ShoopDaLoop Host Manager.app/Contents"
            metadata = plistlib.loads((contents / "Info.plist").read_bytes())
            entry = contents / "MacOS" / metadata["CFBundleExecutable"]
            self.assertEqual(metadata["CFBundleIdentifier"], "org.shoopdaloop.host-manager")
            self.assertEqual(entry.read_bytes(), b"shoop_host_manager")
            for name in ("shoop_host", "shoopdaloop"):
                self.assertEqual(entry.resolve().with_name(name).read_bytes(), name.encode())

    def test_linux_stage_includes_manager_and_installable_desktop_entry(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ("shoopdaloop", "shoop_host", "shoop_host_manager"):
                (root / name).write_bytes(name.encode())
            stage = root / "stage"
            stage.mkdir()
            package_artifacts.create_native_stage("linux", root / "shoopdaloop", stage)
            payload = stage / "shoopdaloop"
            self.assertEqual((payload / "shoop_host_manager").read_bytes(), b"shoop_host_manager")
            self.assertIn("Exec=shoop_host_manager\n", (payload / "shoop_host_manager.desktop").read_text())
            self.assertTrue((payload / "icon.png").is_file())
