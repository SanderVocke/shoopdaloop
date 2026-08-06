#!/usr/bin/env python3
import os
import pathlib
import shutil
import subprocess

PACKAGE = pathlib.Path(__file__).resolve().parent
ROOT = PACKAGE.parents[2]
TARGET = ROOT / "target" / "wasm32-unknown-unknown" / "release" / "shoop_audio_worklet.wasm"
GENERATED = PACKAGE / "generated"

environment = os.environ.copy()
linker_key = "CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER"
if linker_key not in environment and shutil.which("lld") is None:
    rustup_home = pathlib.Path(environment.get("RUSTUP_HOME", pathlib.Path.home() / ".rustup"))
    candidates = sorted(rustup_home.glob("toolchains/*/lib/rustlib/*/bin/rust-lld"))
    if candidates:
        environment[linker_key] = str(candidates[-1])

subprocess.run(
    [
        "cargo",
        "build",
        "--locked",
        "--release",
        "--target",
        "wasm32-unknown-unknown",
        "-p",
        "shoop_audio_worklet",
    ],
    cwd=ROOT,
    env=environment,
    check=True,
)
GENERATED.mkdir(exist_ok=True)
shutil.copy2(TARGET, GENERATED / TARGET.name)
