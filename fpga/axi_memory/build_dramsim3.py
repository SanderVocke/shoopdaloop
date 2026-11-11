#!/usr/bin/env python3
"""Fetch the pinned MIT dependency, apply its idle hook, and build the ctypes ABI."""
import argparse
from pathlib import Path
import shutil
import subprocess

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]


def run(*args, cwd=None):
    subprocess.run(args, cwd=cwd, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT/"work/dramsim3")
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    lock = dict(line.split("=", 1) for line in (HERE/"dramsim3.lock").read_text().splitlines())
    source = output/"source"
    if not source.exists():
        run("git", "init", str(source))
        run("git", "remote", "add", "origin", lock["repository"], cwd=source)
        run("git", "fetch", "--depth", "1", "origin", lock["revision"], cwd=source)
        run("git", "checkout", "--detach", "FETCH_HEAD", cwd=source)
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=source, text=True).strip()
    if revision != lock["revision"]:
        raise SystemExit("cached DRAMsim3 revision differs from lock; use a fresh --output")
    if subprocess.check_output(["git", "status", "--porcelain"], cwd=source):
        raise SystemExit("cached DRAMsim3 source is modified; use a fresh --output")
    vendor = output/"vendor"
    if vendor.exists():
        shutil.rmtree(vendor)
    shutil.copytree(source, vendor, ignore=shutil.ignore_patterns(".git"))
    run("patch", "-p1", "-i", str(HERE/"native/drain.patch"), cwd=vendor)
    run("cmake", "-S", str(HERE/"native"), "-B", str(output/"build"),
        f"-DDRAMSIM3_SOURCE={vendor}", "-DCMAKE_BUILD_TYPE=Release", "-DCMAKE_POLICY_VERSION_MINIMUM=3.5")
    run("cmake", "--build", str(output/"build"), "--target", "shoop_dram", "-j", "4")
    print(output/"build/libshoop_dram.so")


if __name__ == "__main__":
    main()
