#!/usr/bin/env python3
import argparse
import os
import pathlib
import shutil
import subprocess
import tempfile

PACKAGE = pathlib.Path(__file__).resolve().parent
ROOT = PACKAGE.parents[2]
GENERATED = PACKAGE / "generated"

parser = argparse.ArgumentParser()
parser.add_argument("--profile", choices=("debug", "release"))
parser.add_argument(
    "--output-dir",
    type=pathlib.Path,
    default=GENERATED,
    help="artifact destination (defaults to the Trunk generated directory)",
)
args = parser.parse_args()
profile = args.profile or os.environ.get("TRUNK_PROFILE", "release")
if profile not in ("debug", "release"):
    raise RuntimeError(f"unsupported Trunk profile: {profile}")

environment = os.environ.copy()
linker_key = "CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER"
if linker_key not in environment and shutil.which("lld") is None:
    rustup_home = pathlib.Path(environment.get("RUSTUP_HOME", pathlib.Path.home() / ".rustup"))
    candidates = sorted(rustup_home.glob("toolchains/*/lib/rustlib/*/bin/rust-lld"))
    if candidates:
        environment[linker_key] = str(candidates[-1])

output_dir = args.output_dir.resolve()
output_dir.mkdir(parents=True, exist_ok=True)
(output_dir / "shoop_audio_worklet.wasm").unlink(missing_ok=True)
command = [
    "cargo", "build", "--locked", "--target", "wasm32-unknown-unknown",
    "-p", "shoop_rt_host_worklet",
]
if profile == "release":
    command.append("--release")
subprocess.run(command, cwd=ROOT, env=environment, check=True)
target_root = pathlib.Path(environment.get("CARGO_TARGET_DIR", ROOT / "target"))
if not target_root.is_absolute():
    target_root = ROOT / target_root
target = target_root / "wasm32-unknown-unknown" / profile / "shoop_rt_host_worklet.wasm"
shutil.copy2(target, output_dir / target.name)

(output_dir / "shoop_persistence_codec.wasm").unlink(missing_ok=True)
with tempfile.TemporaryDirectory(prefix="shoop-codec-") as temporary:
    subprocess.run(
        [
            "wasm-pack", "build", str(ROOT / "src/rust/shoop_persistence_codec"),
            "--target", "web", "--no-opt", "--no-typescript", "--no-pack",
            "--out-dir", temporary, "--out-name", "shoop_persistence_codec",
            "--dev" if profile == "debug" else "--release",
            "--", "--locked", "--features", "worker-exports",
        ],
        cwd=ROOT, env=environment, check=True,
    )
    for name in ("shoop_persistence_codec.js", "shoop_persistence_codec_bg.wasm"):
        shutil.copy2(pathlib.Path(temporary) / name, output_dir / name)
