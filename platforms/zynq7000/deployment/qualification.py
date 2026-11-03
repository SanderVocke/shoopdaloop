"""Guest-only boot/update transitions and deterministic SD/network fault cases."""
import contextlib
import hashlib
import json
import os
from pathlib import Path
import re
import struct
import subprocess
import time

import card
from emulator import BootQemu, ROOT
from fault_disk import FaultDisk
from layout import LAYOUT
import records
from test_transport import FastbootFixture, LossyUdp


def boots(text):
    return [{"slot": int(s), "trial": int(t), "nonce": int(n), "build": b}
            for s, t, n, b in re.findall(r"SHOOP_BOOT_IMAGE slot=(\d+) trial=(\d+) nonce=(\d+) build=([\w.-]+)", text)]


def snapshot(path):
    result = {}
    with path.open("rb") as disk:
        digest = hashlib.sha256(disk.read(512))
        disk.seek(LAYOUT["boot"]["lba"] * 512)
        for _ in range(LAYOUT["boot"]["sectors"] // 2048):
            digest.update(disk.read(2048 * 512))
        for relative in LAYOUT["config_offsets"]:
            disk.seek((LAYOUT["metadata"]["lba"] + relative) * 512)
            digest.update(disk.read(512))
        result["protected_sha256"] = digest.hexdigest()
        for name, kind in [("state", 2), ("counter", 3)]:
            result[name] = []
            for relative in LAYOUT[name + "_offsets"]:
                disk.seek((LAYOUT["metadata"]["lba"] + relative) * 512)
                raw = disk.read(512)
                public = {"valid": records.valid(raw, kind), "sha256": hashlib.sha256(raw).hexdigest()}
                if public["valid"]:
                    if kind == 2:
                        public.update(zip(["generation", "confirmed", "pending", "attempts", "mode"], struct.unpack_from("<Q4I", raw, 16)))
                    else:
                        public["nonce"] = struct.unpack_from("<Q", raw, 16)[0]
                result[name].append(public)
    return result


class Qualification:
    def __init__(self, artifacts, private, evidence, port_base=16400, updater_port=16554):
        self.artifacts, self.private, self.evidence = map(Path, (artifacts, private, evidence))
        self.evidence.mkdir(parents=True, exist_ok=True)
        (self.private / "cards").mkdir(parents=True, exist_ok=True)
        self.port_base, self.updater_port = port_base, updater_port
        self.loader = self.artifacts / "loader/u-boot.bin"
        self.profile = self.private / "profile.json"
        self.cli = ROOT / "target/debug/shoop-deploy"
        self.results = []
        self.seed = 100000

    def factory(self, name, mode=0):
        path = self.private / "cards" / (name + ".img")
        self.seed += 1000
        loader = self.artifacts / "loader"
        card.create(path, {"BOOT    BIN": (loader / "boot.bin").read_bytes(), "U-BOOT  IMG": (loader / "u-boot.img").read_bytes()},
                    (self.artifacts / "firmware-a.itb").read_bytes(), "firmware-a", (self.private / "credential").read_bytes(), self.seed, mode=mode)
        return path

    def vm(self, image, label, block_url=None, tftp=None):
        return BootQemu(self.loader, image, self.evidence / label, port_base=self.port_base,
                        updater_port=self.updater_port, block_url=block_url, tftp=tftp)

    def cli_command(self, *args, updater=None, journal=None):
        return [self.cli, "--profile", self.profile, "--updater", f"127.0.0.1:{updater or self.updater_port}",
                "--journal", journal or self.evidence / "operation.json", "--timeout", "240", *args]

    def command(self, label, *args, success=True, updater=None, journal=None):
        with (self.evidence / (label + ".log")).open("w") as log:
            result = subprocess.run(self.cli_command(*args, updater=updater, journal=journal), stdout=log, stderr=log, timeout=260)
        content = (self.evidence / (label + ".log")).read_text()
        if (result.returncode == 0) != success:
            raise RuntimeError(label + ": " + content[-12000:])
        return content

    def healthy(self, image, label, allowed=("firmware-a", "firmware-b")):
        with self.vm(image, label) as vm:
            vm.ready("SHOOP_HOST_READY", 150)
            text = vm.serial.read_text()
            images = boots(text)
            assert len(images) == 1 and images[0]["build"] in allowed, images
            assert "SHOOP_DEPLOY_CONFIRMED" in text
        return images[0]

    def record(self, name, **details):
        self.results.append({"case": name, "passed": True, **details})
        (self.evidence / "qualification.json").write_text(json.dumps(self.results, indent=2) + "\n")
        print("Passed deployment case:", name, flush=True)

    def protected(self, image, before, label):
        after = snapshot(image)
        assert before["protected_sha256"] == after["protected_sha256"], "Boot/configuration region changed"
        (self.evidence / (label + "-metadata.json")).write_text(json.dumps({"before": before, "after": after}, indent=2) + "\n")
        return after

    def roundtrip(self):
        image = self.factory("roundtrip")
        before = snapshot(image)
        with self.vm(image, "roundtrip/live") as vm:
            vm.ready("SHOOP_HOST_READY", 150)
            self.command("roundtrip-status", "status")
            for build in ["firmware-b", "firmware-a"]:
                output = self.command("roundtrip-" + build, "deploy", self.artifacts / (build + ".itb"))
                assert "confirmed build=" + build in output
            text = vm.serial.read_text()
            images = boots(text)
            assert [b["build"] for b in images] == ["firmware-a", "firmware-b", "firmware-a"]
            assert [b["nonce"] for b in images] == sorted({b["nonce"] for b in images})
            assert text.count("SHOOP_DUMMY_MIDI_RESET packets=32") == 2
            assert text.count("SHOOP_HOST_CLEANUP_COMPLETE") == 2
        cold = self.healthy(image, "roundtrip/cold", ("firmware-a",))
        assert cold["nonce"] > images[-1]["nonce"]
        self.protected(image, before, "roundtrip")
        self.record("roundtrip", boots=images, cold=cold)

    def watchdogs(self):
        for fault, expected in [("before-ready", 2), ("render", 3), ("control", 3), ("broker", 3), ("handoff-version", 2)]:
            name = "watchdog-" + fault
            image = self.factory(name, mode=1)
            before = snapshot(image)
            with self.vm(image, name) as vm:
                vm.ready("Listening for fastboot", 60)
                self.command(name, "stage", self.artifacts / ("fault-" + fault + ".itb"), "--reboot")
                position = vm.ready("SHOOP_BUILD firmware-a", 300)
                vm.ready("SHOOP_HOST_READY", 150, after=position)
                vm.qmp("query-status")
                images = boots(vm.serial.read_text())
                resets = [e for e in vm.events if e["event"] == "WATCHDOG"]
                assert len(resets) == expected and len(images) == expected + 1, (resets, images)
                assert images[-1]["build"] == "firmware-a"
                assert [b["nonce"] for b in images] == sorted({b["nonce"] for b in images})
            cold = self.healthy(image, name + "-cold", ("firmware-a",))
            assert cold["nonce"] > images[-1]["nonce"]
            self.protected(image, before, name)
            self.record(name, watchdog_resets=len(resets), boots=images, cold=cold)

    def disk_faults(self, seed=20261103):
        import random
        metadata = LAYOUT["metadata"]["lba"]
        rules = [
            ("payload", {"lba": LAYOUT["slot_b"]["lba"] + 1}, "install"),
            ("descriptor", {"lba": LAYOUT["slot_b"]["lba"]}, "install"),
        ]
        for copy, relative in enumerate(LAYOUT["state_offsets"]):
            rules += [(f"publish-{copy}", {"lba": metadata + relative, "record_kind": 2, "pending": 1, "attempts": 2}, "install"),
                      (f"attempt-{copy}", {"lba": metadata + relative, "record_kind": 2, "pending": 1, "attempts": 1}, "boot"),
                      (f"confirm-{copy}", {"lba": metadata + relative, "record_kind": 2, "confirmed": 1, "pending": records.NO_SLOT}, "boot")]
        for copy, relative in enumerate(LAYOUT["counter_offsets"]):
            rules.append((f"nonce-{copy}", {"lba": metadata + relative, "record_kind": 3}, "boot"))
        # Every critical record boundary gets a torn write. Additional seeded
        # omissions/cuts exercise the same real controller path and are replayable.
        cases = [(name, match, phase, "tear") for name, match, phase in rules]
        cases.append(("readback", {"lba": LAYOUT["slot_b"]["lba"] + 1}, "install", "readback"))
        randomizer = random.Random(seed)
        cases += [(name + "-" + mode, match, phase, mode) for mode in ["omit", "cut"]
                  for name, match, phase in randomizer.sample(rules, 2)]
        for name, match, phase, mode in cases:
            name = "disk-" + name
            image = self.factory(name, mode=1)
            before = snapshot(image)
            def rule(event):
                return mode if all(event.get(k) == v for k, v in match.items()) else None
            with FaultDisk(image, self.evidence / name / "backing", rule) as backing:
                with self.vm(image, name + "/live", block_url=backing.url) as vm:
                    vm.ready("Listening for fastboot", 60)
                    if mode == "cut" and phase == "install":
                        with (self.evidence / (name + ".log")).open("w") as log:
                            client = subprocess.Popen(self.cli_command("stage", self.artifacts / "firmware-b.itb"), stdout=log, stderr=log)
                            try:
                                assert backing.hit.wait(150), "Storage cut was not reached"
                            finally:
                                client.kill(); client.wait(timeout=5)
                    else:
                        self.command(name, "stage", self.artifacts / "firmware-b.itb", success=phase != "install")
                        if phase == "boot":
                            with FastbootFixture(self.updater_port) as client:
                                assert client.command("reboot").startswith(b"OKAY")
                            assert backing.hit.wait(150), "Boot metadata fault was not reached"
                    assert backing.hit.is_set(), "Disk fault case did not inject its intended guest write"
                    # Closing this VM is the power cut for blocked/torn writes.
                    text = vm.serial.read_text()
                    used = [b["nonce"] for b in boots(text)]
                    if mode == "cut":
                        vm.power_cut()
            after = snapshot(image)
            cold = self.healthy(image, name + "/cold")
            assert not used or cold["nonce"] > max(used)
            if phase == "install" and not name.startswith("disk-publish-1"):
                assert cold["build"] == "firmware-a", cold
            self.protected(image, before, name)
            self.record(name, mode=mode, seed=seed, match=match, after_fault=after, cold=cold)

    def metadata_damage(self):
        variants = [("state-one", "state", [0], False), ("counter-one", "counter", [0], False),
                    ("config-one", "config", [0], False), ("state-both", "state", [0, 1], True),
                    ("counter-both", "counter", [0, 1], True), ("config-both", "config", [0, 1], True),
                    ("state-version", "state", [0, 1], True), ("config-version", "config", [0, 1], True),
                    ("counter-wrap", "counter", [0, 1], True), ("images-both", "images", [], True)]
        for name, region, copies, recovery in variants:
            image = self.factory(name)
            with image.open("r+b") as disk:
                if region == "images":
                    for slot in ["slot_a", "slot_b"]:
                        disk.seek(LAYOUT[slot]["lba"] * 512); disk.write(bytes(512))
                for copy in copies:
                    offset = (LAYOUT["metadata"]["lba"] + LAYOUT[region + "_offsets"][copy]) * 512
                    disk.seek(offset)
                    if name.endswith("version"):
                        raw = bytearray(disk.read(512)); struct.pack_into("<I", raw, 4, LAYOUT["version"] + 1)
                        import zlib
                        struct.pack_into("<I", raw, 508, zlib.crc32(raw[:508])); disk.seek(offset)
                    elif name == "counter-wrap":
                        raw = records.record(3, struct.pack("<Q", (1 << 64) - 1))
                    else:
                        raw = bytes(512)
                    disk.write(raw)
                disk.flush(); os.fsync(disk.fileno())
            before = snapshot(image)
            if recovery:
                with self.vm(image, name) as vm:
                    vm.ready("Listening for fastboot", 90)
                    assert not boots(vm.serial.read_text()), "Unusable identity/state reached the app"
                    if region in {"state", "images"}:
                        self.command(name, "stage", self.artifacts / "firmware-b.itb", "--reboot")
                        vm.ready("SHOOP_HOST_READY", 150)
                        assert boots(vm.serial.read_text())[-1]["build"] == "firmware-b"
                    elif region == "config":
                        self.command(name, "stage", self.artifacts / "firmware-b.itb", success=False)
                        assert not boots(vm.serial.read_text())
            else:
                self.healthy(image, name, ("firmware-a",))
            self.protected(image, before, name)
            self.record(name, recovery=recovery)

    def network_faults(self):
        # A fresh INIT must discard the previous client's abandoned DOWNLOAD.
        image = self.factory("abandoned-download", mode=1)
        before = snapshot(image)
        with self.vm(image, "abandoned-download") as vm:
            vm.ready("Listening for fastboot", 60)
            with FastbootFixture(self.updater_port) as client:
                assert client.command("download:00001000") == b"DATA00001000"
                assert client.exchange(3, bytes(1020)) == b""
            with LossyUdp(self.updater_port, seed=20261103) as proxy:
                self.command("lossy-resume", "stage", self.artifacts / "firmware-b.itb", "--reboot", updater=proxy.port)
                assert proxy.dropped > 0 and proxy.install_reply_dropped
                dropped = proxy.dropped
            vm.ready("SHOOP_HOST_READY", 150)
            assert boots(vm.serial.read_text())[-1]["build"] == "firmware-b"
        self.protected(image, before, "abandoned-download")
        self.record("abandoned-download-and-lost-replies", seed=20261103, dropped=dropped)
        # Stop the workstation during an authenticated transfer, then run exactly
        # the same command/journal. The guest card remains exclusively owned by QEMU.
        image = self.factory("cli-resume")
        before = snapshot(image)
        with self.vm(image, "cli-resume") as vm:
            vm.ready("SHOOP_HOST_READY", 150)
            journal = self.evidence / "resume-operation.json"
            with (self.evidence / "cli-interrupted.log").open("w") as log:
                client = subprocess.Popen(self.cli_command("deploy", self.artifacts / "firmware-b.itb", journal=journal), stdout=log, stderr=log)
                try:
                    vm.ready("Starting download", 90)
                finally:
                    client.kill(); client.wait(timeout=5)
            result = self.command("cli-resumed", "deploy", self.artifacts / "firmware-b.itb", journal=journal)
            assert "confirmed build=firmware-b" in result
        self.protected(image, before, "cli-resume")
        self.record("interrupted-cli-resumes-authenticated-operation")

    def refusals(self):
        image = self.factory("refusals", mode=1)
        before = snapshot(image)
        with self.vm(image, "refusals") as vm:
            vm.ready("Listening for fastboot", 60)
            with FastbootFixture(self.updater_port) as client:
                for command in ["flash:boot", "erase:boot", "oem run:mmc erase 0 100", "oem board:erase", "download:02000001"]:
                    assert client.command(command).startswith(b"FAIL"), command
            for name in ["bad-signature", "bad-profile", "bad-range", "bad-length"]:
                self.command(name, "stage", self.artifacts / (name + ".itb"), success=False)
                assert not boots(vm.serial.read_text())
        cold = self.healthy(image, "refusals-cold", ("firmware-a",))
        self.protected(image, before, "refusals")
        self.record("protected-writes-and-invalid-bundles-refused", cold=cold)

    def cleanup_refusal(self):
        image = self.factory("cleanup-refusal")
        # A signed test image injects one cleanup refusal. It is an ordinary
        # autonomous healthy image until the authenticated request arrives.
        fixture = (self.artifacts / "fault-cleanup.itb").read_bytes()
        with image.open("r+b") as disk:
            for relative in LAYOUT["state_offsets"]:
                disk.seek((LAYOUT["metadata"]["lba"] + relative) * 512)
                disk.write(records.selection(hashlib.sha256(fixture).digest()))
            disk.seek(LAYOUT["slot_a"]["lba"] * 512)
            disk.write(records.slot(fixture, "fault-cleanup")); disk.write(fixture)
            disk.flush(); os.fsync(disk.fileno())
        before = snapshot(image)
        journal = self.evidence / "cleanup-operation.json"
        with self.vm(image, "cleanup-refusal") as vm:
            vm.ready("SHOOP_HOST_READY", 150)
            refused = self.command("cleanup-refused", "deploy", self.artifacts / "firmware-b.itb", success=False, journal=journal)
            assert "CleanupFailed" in refused
            vm.qmp("query-status")
            assert len(boots(vm.serial.read_text())) == 1
            assert not any(e["event"] in {"RESET", "WATCHDOG"} for e in vm.events)
            result = self.command("cleanup-retried", "deploy", self.artifacts / "firmware-b.itb", journal=journal)
            assert "confirmed build=firmware-b" in result
        self.protected(image, before, "cleanup-refusal")
        self.record("cleanup-refuses-reset-and-allows-fresh-operation-retry")

    def no_server(self):
        image = self.factory("no-server", mode=2)
        before = snapshot(image)
        started = time.monotonic()
        with self.vm(image, "no-server") as vm:
            vm.ready("SHOOP_HOST_READY", 120)
            text = vm.serial.read_text()
            assert "SHOOP_NETWORK_BOOT_REFUSED" in text
            assert boots(text)[-1]["build"] == "firmware-a"
        self.protected(image, before, "no-server")
        self.record("missing-tftp-server-bounded-local-fallback", seconds=round(time.monotonic() - started, 2))
