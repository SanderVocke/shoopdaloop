#!/usr/bin/env python3
"""Run strict native JACK acceptance with an isolated dummy server and bounded teardown."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", choices=["debug", "release"], default="debug")
    parser.add_argument("--output", type=Path, default=ROOT / "target/native-jack")
    args = parser.parse_args()
    if not shutil.which("jackd"):
        raise RuntimeError("strict JACK acceptance requires jackd with its dummy driver")
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=True)
    preview_config = args.output / "preview-driver.json"
    preview_config.write_text(json.dumps({"Jack": {
        "client_name": "ShoopPreviewAcceptance",
        "audio_inputs": [], "audio_outputs": ["audio_out_1", "audio_out_2"],
        "midi_inputs": [], "midi_outputs": [], "connections": [],
    }}))
    cargo_args = ["--release"] if args.profile == "release" else []
    env = os.environ.copy()
    env.pop("SHOOP_ALLOW_MISSING_BACKENDS", None)
    env["JACK_DEFAULT_SERVER"] = f"shoop-acceptance-{os.getpid()}"
    env["SHOOP_JACK_TEST_BUFFER_CHANGES"] = "1"
    env["SHOOP_JACK_TEST_SERVER_LOSS"] = "1"
    env["RUST_BACKTRACE"] = "1"
    env["SHOOP_PREVIEW_DRIVER_CONFIG"] = str(preview_config)
    env["SHOOP_PREVIEW_LOG_DIR"] = str(args.output)
    commands = [
        ["cargo", "test", "--locked", "-p", "shoop_rt_host_native", "--no-default-features", "--features", "jack", "--test", "jack", "native_jack_server_readiness", *cargo_args],
        ["cargo", "test", "--locked", "-p", "shoop_rt_host_native", "--no-default-features", "--features", "jack", "--lib", *cargo_args],
        ["cargo", "test", "--locked", "-p", "shoop_rt_host_native", "--no-default-features", "--features", "jack", "--test", "jack", *cargo_args, "--", "--test-threads=1"],
        ["cargo", "test", "--locked", "-p", "shoop_host", "--test", "jack", *cargo_args, "--", "--test-threads=1"],
        ["cargo", "test", "--locked", "-p", "shoop_host", "--test", "recovery", "native_master_preview_device_acceptance", *cargo_args, "--", "--ignored", "--exact", "--test-threads=1"],
        ["cargo", "test", "--locked", "-p", "shoop_rt_host_process", "native_jack", *cargo_args, "--", "--test-threads=1"],
        ["cargo", "test", "--locked", "-p", "shoopdaloop", "native_jack_application_start", *cargo_args, "--", "--test-threads=1"],
    ]
    with (args.output / "server.log").open("w") as log:
        server = subprocess.Popen(["jackd", "--no-realtime", "--name", env["JACK_DEFAULT_SERVER"], "-d", "dummy", "-r", "48000", "-p", "128"], env=env, stdout=log, stderr=subprocess.STDOUT)
        try:
            deadline = time.monotonic() + 10
            while True:
                if server.poll() is not None:
                    raise RuntimeError(f"JACK startup failed; see {args.output / 'server.log'}")
                if "JACK server starting" in (args.output / "server.log").read_text():
                    # The first command probes the server through the production
                    # client API before the audio/MIDI acceptance scenarios run.
                    break
                if time.monotonic() >= deadline:
                    raise RuntimeError("JACK startup timed out")
                time.sleep(0.05)
            for index, command in enumerate(commands):
                print("+", " ".join(command), flush=True)
                with (args.output / f"acceptance-{index}.log").open("w") as output:
                    result = subprocess.run(command, cwd=ROOT, env=env, stdout=output, stderr=subprocess.STDOUT, timeout=1200)
                print((args.output / f"acceptance-{index}.log").read_text(), end="", flush=True)
                if result.returncode:
                    return result.returncode
                if server.poll() is not None:
                    raise RuntimeError("JACK server exited during acceptance")
        finally:
            if server.poll() is None:
                server.terminate()
                try:
                    server.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    server.kill()
                    server.wait(timeout=5)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
