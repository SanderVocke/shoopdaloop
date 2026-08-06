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
GENERATED.mkdir(exist_ok=True)
shutil.copy2(target, GENERATED / target.name)
print(f"copied {profile} worklet: {target} -> {GENERATED / target.name}")
