#!/usr/bin/env python3
"""Bounded, unprivileged Zynq firmware execution and diagnostic capture."""
import json
import os
from pathlib import Path
import socket
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]


class Qemu:
    def __init__(self, elf, directory, forwards):
        self.elf = Path(elf).resolve()
        self.directory = Path(directory).resolve()
        self.directory.mkdir(parents=True, exist_ok=True)
        self.serial = self.directory / "serial.log"
        self.serial.write_text("")
        self.qmp_path = self.directory / "qmp.sock"
        if self.qmp_path.exists():
            self.qmp_path.unlink()
        network = "user,id=lan" + "".join(f",hostfwd=tcp:127.0.0.1:{host}-10.0.2.15:{guest}" for host, guest in forwards)
        self.command = [os.environ.get("QEMU", "qemu-system-arm"),
            "-M", "xilinx-zynq-a9", "-accel", "tcg", "-smp", "1", "-m", "1G",
            "-display", "none", "-monitor", "none", "-serial", "null",
            "-serial", "file:" + str(self.serial), "-qmp", "unix:" + str(self.qmp_path) + ",server=on,wait=off",
            "-device", f"loader,file={self.elf},cpu-num=0", "-netdev", network, "-net", "nic,netdev=lan",
            "-object", f"filter-dump,id=trace,netdev=lan,file={self.directory / 'ethernet.pcap'}"]
        (self.directory / "command.json").write_text(json.dumps(self.command, indent=2) + "\n")
        self.log = open(self.directory / "qemu.log", "w")
        version = subprocess.check_output([self.command[0], "--version"], text=True).splitlines()[0]
        expected = json.loads((Path(__file__).parent / "pins.json").read_text())["qemu"]
        if not version.startswith("QEMU emulator version " + expected + " "):
            raise RuntimeError(f"Expected QEMU {expected}, found {version}")
        (self.directory / "version.txt").write_text(version + "\n")
        self.process = subprocess.Popen(self.command, stdout=self.log, stderr=self.log)
        self.connection = None
        self.messages = None

    def qmp(self, command, arguments=None):
        if self.connection is None:
            self.connection = socket.socket(socket.AF_UNIX)
            self.connection.settimeout(5)
            self.connection.connect(str(self.qmp_path))
            self.messages = self.connection.makefile("rwb", buffering=0)
            json.loads(self.messages.readline())
            self.qmp("qmp_capabilities")
        request = {"execute": command}
        if arguments is not None:
            request["arguments"] = arguments
        self.messages.write(json.dumps(request).encode() + b"\n")
        while True:
            line = self.messages.readline()
            if not line:
                raise RuntimeError("QMP closed")
            response = json.loads(line)
            if "error" in response:
                raise RuntimeError(response["error"])
            if "return" in response:
                return response["return"]

    def ready(self, marker, timeout=45):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            text = self.serial.read_text(errors="replace") if self.serial.exists() else ""
            if "FATAL" in text or self.process.poll() is not None:
                raise RuntimeError("Firmware failed: " + text)
            if marker in text:
                memory = self.qmp("query-memory-size-summary")
                cpus = self.qmp("query-cpus-fast")
                assert memory["base-memory"] == 1 << 30, memory
                assert len(cpus) == 1, cpus
                (self.directory / "machine.json").write_text(json.dumps({"memory": memory, "cpus": cpus}, indent=2) + "\n")
                return
            time.sleep(.05)
        raise TimeoutError("Firmware readiness timed out: " + text)

    def close(self):
        try:
            if self.process.poll() is None:
                registers = self.qmp("human-monitor-command", {"command-line": "info registers"})
                (self.directory / "registers.txt").write_text(registers)
                for name, command in [("gem", "xp /64wx 0xe000b000"), ("gic", "x /64wx 0xf8f01100"), ("bd", "xp /32wx 0x01700000"), ("irq", "info irq")]:
                    (self.directory / (name + ".txt")).write_text(self.qmp("human-monitor-command", {"command-line": command}))
        finally:
            if self.messages:
                self.messages.close()
            if self.connection:
                self.connection.close()
            if self.process.poll() is None:
                self.process.terminate()
                try:
                    self.process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    self.process.kill()
                    self.process.wait(timeout=5)
            self.log.close()
            self.qmp_path.unlink(missing_ok=True)

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()


def echo_test():
    directory = ROOT / "work/zynq/echo-test"
    with Qemu(ROOT / "work/zynq/build/echo.elf", directory, [(15400, 15400)]) as emulator:
        emulator.ready("SHOOP_ECHO_READY")
        deadline = time.monotonic() + 60
        total = 0
        for _ in range(3):
            with socket.create_connection(("127.0.0.1", 15400), timeout=10) as peer:
                for size in [1, 7, 127, 2048, 131072] * 8:
                    if time.monotonic() >= deadline:
                        raise TimeoutError("Echo exchange deadline expired")
                    payload = bytes(i % 251 for i in range(size))
                    for offset in range(0, size, 997):
                        peer.sendall(payload[offset:offset + 997])
                    received = bytearray()
                    while len(received) < size:
                        block = peer.recv(size - len(received))
                        if not block:
                            raise RuntimeError("Unexpected echo EOF")
                        received.extend(block)
                    assert received == payload, "Echo content mismatch"
                    total += size
        (directory / "result.json").write_text(json.dumps({"gate": 1, "connections": 3, "bytes": total, "passed": True}) + "\n")
        print(f"Gate 1 passed: {total} exact bytes over 3 connections")


if __name__ == "__main__":
    echo_test()
