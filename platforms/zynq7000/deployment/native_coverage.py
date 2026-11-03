"""Instrument the workstation CLI while keeping all Arm firmware builds unchanged."""
import os
from pathlib import Path
import shlex
import subprocess

ROOT = Path(__file__).resolve().parents[3]


def binary(directory):
    return directory.resolve() / "private/native-coverage/debug/shoop-deploy"


def environment(directory):
    target = directory.resolve() / "private/native-coverage"
    configured = {**os.environ, "CARGO_TARGET_DIR": str(target)}
    output = subprocess.check_output(["cargo", "llvm-cov", "show-env", "--sh"], cwd=ROOT, env=configured, text=True)
    for line in output.splitlines():
        if not line.startswith("export "):
            continue
        name, separator, value = line[7:].partition("=")
        if not separator:
            raise RuntimeError("Invalid cargo-llvm-cov environment")
        words = shlex.split(value)
        if len(words) != 1:
            raise RuntimeError("Invalid cargo-llvm-cov environment value")
        configured[name] = words[0]
    return configured


def build(directory):
    subprocess.run(["cargo", "build", "--locked", "-p", "shoop_host", "--no-default-features", "--bin", "shoop-deploy"],
                   cwd=ROOT, env=environment(directory), check=True)
    if not binary(directory).is_file():
        raise RuntimeError("Instrumented CLI was not built at its expected path")


def report(directory):
    configured = environment(directory)
    for kind, suffix in [("--lcov", "lcov"), ("--json", "json")]:
        subprocess.run(["cargo", "llvm-cov", "report", kind,
                        "--output-path", str(directory.resolve() / "evidence" / ("native-cli-coverage." + suffix))],
                       cwd=ROOT, env=configured, check=True)


def configure_runtime(directory):
    configured = environment(directory)
    # Export only runtime settings; instrumentation/wrapper flags stay confined
    # to the explicit native build above, including during mixed Arm/native CI.
    os.environ["LLVM_PROFILE_FILE"] = configured["LLVM_PROFILE_FILE"]
    os.environ["SHOOP_DEPLOY_BIN"] = str(binary(directory))
