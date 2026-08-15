#!/usr/bin/env python3
import argparse
import os
import pathlib
import shutil
import subprocess

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
target = (
    ROOT
    / "target"
    / "wasm32-unknown-unknown"
    / profile
    / "shoop_audio_worklet.wasm"
)

environment = os.environ.copy()
linker_key = "CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER"
if linker_key not in environment and shutil.which("lld") is None:
    rustup_home = pathlib.Path(environment.get("RUSTUP_HOME", pathlib.Path.home() / ".rustup"))
    candidates = sorted(rustup_home.glob("toolchains/*/lib/rustlib/*/bin/rust-lld"))
    if candidates:
        environment[linker_key] = str(candidates[-1])

command = [
    "cargo",
    "build",
    "--locked",
    "--target",
    "wasm32-unknown-unknown",
    "-p",
    "shoop_audio_worklet",
]
if profile == "release":
    command.append("--release")
subprocess.run(command, cwd=ROOT, env=environment, check=True)
output_dir = args.output_dir.resolve()
output_dir.mkdir(parents=True, exist_ok=True)
destination = output_dir / target.name
shutil.copy2(target, destination)
print(f"copied {profile} worklet: {target} -> {destination}")
