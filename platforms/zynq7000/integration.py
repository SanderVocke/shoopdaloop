#!/usr/bin/env python3
"""Run the native application client against the full FreeRTOS firmware."""
import json
import argparse
import os
from pathlib import Path
import subprocess
import sys
from qemu import Qemu, ROOT
from remote_profile import write_profile


def main():
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--fixture", action="store_true")
    mode.add_argument("--ui", action="store_true")
    mode.add_argument("--recovery", action="store_true")
    parser.add_argument("--small-fixture", action="store_true", help="development run; omits the 40 MiB and full recording-pool gates")
    parser.add_argument("--port-base", type=int, default=15400)
    parser.add_argument("--soak-cycles", type=int, default=100)
    args = parser.parse_args()
    if not 100 <= args.soak_cycles <= 100_000:
        parser.error("--soak-cycles must be between 100 and 100000")
    directory = ROOT / "work/zynq" / ("fixture-test" if args.fixture else "ui-test" if args.ui else "recovery-test" if args.recovery else "integration")
    directory.mkdir(parents=True, exist_ok=True)
    (directory / "result.json").write_text(json.dumps({"passed": False, "state": "building"}) + "\n")
    credential = directory / "test.key"
    descriptor = os.open(credential, os.O_CREAT | os.O_TRUNC | os.O_WRONLY, 0o600)
    with os.fdopen(descriptor, "wb") as stream:
        stream.write(os.urandom(32))
    credential.chmod(0o600)
    profile = directory / "profile.json"
    write_profile(profile, credential, args.port_base)
    build = [sys.executable, "platforms/zynq7000/build.py", "--app", "host", "--credential-file", str(credential)]
    if args.fixture:
        build += ["--fixture"]
    subprocess.run(build, cwd=ROOT, check=True)
    if args.ui:
        build_client = ["cargo", "test", "--locked", "-p", "shoopdaloop", "--no-default-features", "--bin", "shoopdaloop", "--no-run", "--message-format=json"]
        result = subprocess.run(build_client, cwd=ROOT, check=True, capture_output=True, text=True)
        executable = next(json.loads(line)["executable"] for line in result.stdout.splitlines()
                          if json.loads(line).get("executable"))
        client_command = [executable, "--exact", "native_recovery_tests::zynq_firmware_ui_workflow", "--ignored", "--nocapture"]
    else:
        subprocess.run(["cargo", "build", "--locked", "-p", "shoop_host", "--no-default-features", "--example", "zynq_probe"], cwd=ROOT, check=True)
        client_command = [ROOT / "target/debug/examples/zynq_probe", profile]
    elf = ROOT / "work/zynq/build" / ("host-fixture.elf" if args.fixture else "host.elf")
    boots = 1 if args.fixture or args.ui or args.recovery else 2
    previous_nonce = None
    for boot in range(boots):
        run_directory = directory if boot == 0 else directory / "restart"
        run_directory.mkdir(parents=True, exist_ok=True)
        (run_directory / "result.json").write_text(json.dumps({"passed": False, "state": "running"}) + "\n")
        nonce = (2 if previous_nonce != 2 else 3) if boot else None
        with Qemu(elf, run_directory, [(args.port_base, 15400), (args.port_base+1, 15401)], driver=args.fixture, boot_nonce=nonce) as emulator:
            previous_nonce = emulator.boot_nonce
            emulator.ready("SHOOP_HOST_READY", timeout=60)
            environment = dict(os.environ)
            environment["SHOOP_ZYNQ_PROFILE"] = str(profile)
            if boot == 0:
                environment["SHOOP_ZYNQ_AUTHORITY_OUTPUT"] = str(directory / "authority.json")
            else:
                environment["SHOOP_ZYNQ_PREVIOUS_AUTHORITY"] = str(directory / "authority.json")
            if args.fixture:
                environment["SHOOP_ZYNQ_DRIVER_SOCKET"] = str(emulator.driver_path)
                environment["SHOOP_ZYNQ_SOAK_CYCLES"] = str(args.soak_cycles)
            if args.small_fixture:
                if not args.fixture:
                    raise ValueError("--small-fixture requires --fixture")
                environment["SHOOP_ZYNQ_SMALL_FIXTURE"] = "1"
            if args.recovery:
                environment["SHOOP_ZYNQ_RECOVERY"] = "1"
            with open(run_directory / "client.log", "w") as log:
                result = subprocess.run(client_command, cwd=ROOT, env=environment, stdout=log, stderr=log,
                                        timeout=1200 + 2*(args.soak_cycles-100) if args.fixture else 360)
            if result.returncode:
                raise RuntimeError((run_directory / "client.log").read_text()[-12000:])
            emulator.ready("SHOOP_HOST_STOPPED", timeout=15)
            emulator.ready("SHOOP_HOST_RETURNED", timeout=15)
            serial = emulator.serial.read_text()
            if "SHOOP_HOST_CLEANUP_COMPLETE" not in serial:
                raise RuntimeError("Host cleanup was not confirmed")
            if "SHOOP_DUMMY_MIDI_RESET packets=32" not in serial:
                raise RuntimeError("Dummy sink did not acknowledge the complete MIDI reset")
            if args.fixture:
                for marker in ["SHOOP_FPU_PASS", "SHOOP_ALLOC_ATOMIC_PASS"]:
                    if marker not in serial:
                        raise RuntimeError("Missing runtime check: " + marker)
            import re
            memory = [{name: int(value) for name, value in re.findall(r"(\w+)=(\d+)", line)}
                      for line in serial.splitlines() if line.startswith("SHOOP_MEMORY ")]
            assert memory and max(sample["peak"] for sample in memory) < 768*1024*1024
            assert memory[-1]["used"] == 0, "Rust owners survived host teardown"
            c_memory = [{name: int(value) for name, value in re.findall(r"(\w+)=(\d+)", line)}
                        for line in serial.splitlines() if line.startswith("SHOOP_C_MEMORY ")]
            assert c_memory and max(sample["newlib_committed"] for sample in c_memory) <= 16*1024*1024
            assert max(sample["lwip_peak"] for sample in c_memory) <= 4*1024*1024
            assert max(sample["tcp_pcbs"] for sample in c_memory) <= 64
            assert c_memory[-1]["netconns"] == 0 and c_memory[-1]["sockets"] == 0
            stacks = [{"name": name, "free_words": int(words)} for name, words in
                      re.findall(r"SHOOP_STACK name=(.*?) free_words=(\d+)", serial)]
            assert stacks and min(task["free_words"] for task in stacks) > 64
            assert min(sample["c_min_free"] for sample in memory) > 1024*1024
            assert min(sample["control_stack_free"] for sample in memory) > 1024
            assert min(sample["render_stack_free"] for sample in memory) > 1024
            uptime = [sample["uptime_ms"] for sample in memory]
            assert uptime == sorted(uptime), "monotonic timer moved backwards"
            if args.recovery:
                assert uptime[-1] > 13_000, "recovery did not exercise the timer low-word carry"
            (run_directory / "result.json").write_text(json.dumps({
                "kind": "fixture" if args.fixture else "ui" if args.ui else "recovery" if args.recovery else "production",
                "passed": True,
                "large_media_gate": args.fixture and not args.small_fixture,
                "full_pool_gate": args.fixture and not args.small_fixture,
                "ram_bytes": 1 << 30, "boot_nonce": emulator.boot_nonce, "memory": memory, "c_memory": c_memory, "stacks": stacks}, indent=2) + "\n")
            print((run_directory / "client.log").read_text())



if __name__ == "__main__":
    main()
