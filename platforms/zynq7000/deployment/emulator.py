"""Bounded bootloader execution with a persistent SD card and Ethernet."""
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import threading
import time

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from qemu import Qemu, ROOT


class BootQemu(Qemu):
    def __init__(self, loader, card, directory, port_base=15400, updater_port=15554, tftp=None, qemu=None, first_stage=False):
        self.directory = Path(directory).resolve()
        self.directory.mkdir(parents=True, exist_ok=True)
        self.serial = self.directory / "serial.log"
        self.qmp_path = self.directory / "qmp.sock"
        self.driver_path = self.directory / "unused-driver.sock"
        self.console_path = self.directory / "console.sock"
        for path in [self.qmp_path, self.console_path]:
            path.unlink(missing_ok=True)
        binary = Path(qemu or ROOT / "work/zynq/qemu-build/qemu-system-arm").resolve()
        manifest_path = binary.parent / "shoop-build.json"
        manifest = json.loads(manifest_path.read_text())
        patch = ROOT / "platforms/zynq7000/patches/qemu-cadence-watchdog.patch"
        if manifest["patch_sha256"] != hashlib.sha256(patch.read_bytes()).hexdigest() or manifest["binary_sha256"] != hashlib.sha256(binary.read_bytes()).hexdigest():
            raise RuntimeError("Deployment QEMU does not match its watchdog model manifest")
        (self.directory / "emulator.json").write_text(json.dumps(manifest, indent=2) + "\n")
        network = f"user,id=lan,hostfwd=tcp:127.0.0.1:{port_base}-10.0.2.15:15400,hostfwd=tcp:127.0.0.1:{port_base+1}-10.0.2.15:15401,hostfwd=udp:127.0.0.1:{updater_port}-10.0.2.15:5554"
        if tftp is not None:
            network += ",tftp=" + str(Path(tftp).resolve())
        entry = f"loader,file={Path(loader).resolve()},cpu-num=0"
        if not first_stage:
            entry += ",addr=0x04000000"
        self.command = [str(binary), "-M", "xilinx-zynq-a9,boot-mode=sd", "-accel", "tcg", "-smp", "1", "-m", "1G",
                        "-display", "none", "-monitor", "none", "-serial", "null",
                        "-serial", f"unix:{self.console_path},server=on,wait=on",
                        "-qmp", f"unix:{self.qmp_path},server=on,wait=off", "-watchdog-action", "reset",
                        "-device", entry, "-drive", f"file={Path(card).resolve()},if=sd,format=raw,cache=directsync",
                        "-netdev", network, "-net", "nic,netdev=lan",
                        "-object", f"filter-dump,id=trace,netdev=lan,file={self.directory / 'ethernet.pcap'}"]
        (self.directory / "command.json").write_text(json.dumps(self.command, indent=2) + "\n")
        self.log = (self.directory / "qemu.log").open("w")
        self.connection = self.messages = None
        self.events = []
        self._stop = threading.Event()
        self._serial_socket = None
        self._reader = None
        self.process = subprocess.Popen(self.command, stdout=self.log, stderr=self.log)
        try:
            deadline = time.monotonic() + 10
            while not self.console_path.exists():
                if self.process.poll() is not None or time.monotonic() >= deadline:
                    raise RuntimeError("QEMU boot console did not start")
                time.sleep(.02)
            self._serial_socket = socket.socket(socket.AF_UNIX)
            self._serial_socket.connect(str(self.console_path))
            self._serial_socket.settimeout(.1)
            self._reader = threading.Thread(target=self._capture, daemon=True)
            self._reader.start()
        except BaseException:
            self.close()
            raise

    def _capture(self):
        with self.serial.open("wb") as stream:
            while not self._stop.is_set():
                try:
                    data = self._serial_socket.recv(65536)
                except socket.timeout:
                    continue
                except OSError:
                    break
                if not data:
                    break
                stream.write(data)
                stream.flush()

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
            response = json.loads(self.messages.readline())
            if "event" in response:
                self.events.append(response)
            if "error" in response:
                raise RuntimeError(response["error"])
            if "return" in response:
                return response["return"]

    def ready(self, marker, timeout=60, after=0):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            content = self.serial.read_text(errors="replace") if self.serial.exists() else ""
            if marker in content[after:]:
                memory, cpus = self.qmp("query-memory-size-summary"), self.qmp("query-cpus-fast")
                if memory["base-memory"] != 1 << 30 or len(cpus) != 1:
                    raise RuntimeError("Deployment VM violates the one-core/1 GiB contract")
                (self.directory / "machine.json").write_text(json.dumps({"memory": memory, "cpus": cpus}, indent=2) + "\n")
                return content.index(marker, after)
            if self.process.poll() is not None:
                raise RuntimeError("Deployment VM exited:\n" + content[-12000:])
            time.sleep(.05)
        raise TimeoutError("Boot marker missing: " + marker + "\n" + content[-12000:])

    def close(self):
        try:
            super().close()
        finally:
            self._stop.set()
            if self._serial_socket is not None:
                self._serial_socket.close()
            if self._reader is not None:
                self._reader.join(timeout=2)
            self.console_path.unlink(missing_ok=True)
            (self.directory / "events.json").write_text(json.dumps(self.events, indent=2) + "\n")
