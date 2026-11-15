import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]


class NativeSourceTreeMarkerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory()
        cls.addClassCleanup(cls.temporary.cleanup)
        cls.build_script = Path(cls.temporary.name) / "build-script"
        if os.name == "nt":
            cls.build_script = cls.build_script.with_suffix(".exe")
        subprocess.run(
            ["rustc", "--edition=2021", "-Dwarnings", str(ROOT / "src/rust/shoopdaloop/build.rs"),
             "-o", str(cls.build_script)],
            env={**os.environ, "CARGO_MANIFEST_DIR": str(ROOT / "src/rust/shoopdaloop"),
                 "CARGO_PKG_VERSION": "0.3.0"},
            check=True, capture_output=True, text=True,
        )

    def run_build_script(self, out, arch="x86_64"):
        out.mkdir(parents=True)
        return subprocess.run(
            [str(self.build_script)],
            env={**os.environ, "OUT_DIR": str(out), "CARGO_CFG_TARGET_ARCH": arch},
            capture_output=True, text=True,
        )

    def test_marker_resolves_source_root_for_both_cargo_layouts(self):
        for profile in ("debug", "release", "llvm-cov-target/debug", "aarch64-unknown-linux-gnu/debug"):
            for layout in ("shoopdaloop-hash/out", "shoopdaloop/hash/out"):
                with self.subTest(profile=profile, layout=layout), tempfile.TemporaryDirectory() as directory:
                    profile_dir = Path(directory) / profile
                    result = self.run_build_script(profile_dir / "build" / layout)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    marker = (profile_dir / "SHOOP_SRC_TREE").read_text().strip()
                    self.assertFalse(Path(marker).is_absolute())
                    self.assertEqual((profile_dir / marker).resolve(), ROOT)

    def test_unrecognized_layout_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            result = self.run_build_script(Path(directory) / "debug/not-build/hash/out")
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("unexpected Cargo OUT_DIR layout", result.stderr)

    def test_wasm_does_not_write_native_marker(self):
        with tempfile.TemporaryDirectory() as directory:
            profile_dir = Path(directory) / "debug"
            result = self.run_build_script(profile_dir / "build/shoopdaloop/hash/out", "wasm32")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertFalse((profile_dir / "SHOOP_SRC_TREE").exists())
