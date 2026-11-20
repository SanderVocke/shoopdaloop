"""Qualify FPGA update policy under QEMU with a synthetic fabric payload."""
import json
from pathlib import Path
import struct
import subprocess
import sys
import zipfile

import bundle
import card
from emulator import BootQemu, ROOT
import fpga_pair
from qualification import Qualification, boots, snapshot
from remote_profile import write_profile
from test_transport import FastbootFixture


def fixture_route(directory, board, sources, variant):
    (directory / "board").mkdir(parents=True)
    fpga_pair.write_json(directory / "board/board.json", board.data)
    with zipfile.ZipFile(directory / "zturn_v2.xsa", "w") as archive:
        for name, source in sources.items():
            archive.writestr(name, source)
        archive.writestr("fixture-fabric", variant)
    bitstream = b"\xff" * 4 + b"\xaa\x99\x55\x66" + variant.encode().ljust(248, b"\0")
    (directory / "zturn_v2.bit").write_bytes(
        b"\x00\x01\x0f\x00\x01a\x00\x01\x00b\x00\x01\x00c\x00\x01\x00d\x00\x01\x00e"
        + struct.pack(">I", len(bitstream)) + bitstream
    )
    (directory / "geometry.txt").write_text("looping\n")
    for placeholder in ("ps7_init.tcl", "ps-properties.txt"):
        (directory / placeholder).write_text("# QEMU test fixture\n")
    (directory / "fpga-abi.json").write_bytes(fpga_pair.RTL_ABI.read_bytes())
    fpga_pair.stamp(directory, "synthetic QEMU fixture")


def qualify_fpga_update(board, private, keys, confirmed_elf):
    platform = Path(__file__).resolve().parents[1]
    directory = private / "fpga-update"
    directory.mkdir()
    build = "fpga-update-trial"
    subprocess.run([sys.executable, platform / "build.py", "--board", board.name, "--app", "host",
                    "--deployment", "--fpga", "--build-id", build], cwd=ROOT, check=True)
    fpga_elf = board.build_dir(ROOT) / "host-deployment-fpga.elf"
    uboot = ROOT / "work/zynq/u-boot"
    sources = {"ps7_init_gpl.c": (uboot / board.boot["ps_init"]).read_bytes(),
               "ps7_init_gpl.h": (uboot / "arch/arm/mach-zynq/include/mach/ps7_init_gpl.h").read_bytes()}
    initial, next_route, incompatible = (directory / name for name in ("initial", "next", "incompatible"))
    fixture_route(initial, board, sources, "factory")
    fixture_route(next_route, board, sources, "update")
    fixture_route(incompatible, board, {**sources, "ps7_init_gpl.c": sources["ps7_init_gpl.c"] + b"\n"}, "other PS")
    assert fpga_pair.digest(initial / "zturn_v2.xsa") != fpga_pair.digest(next_route / "zturn_v2.xsa")
    boot_build = directory / "u-boot-build"
    subprocess.run([sys.executable, platform / "deployment/build_boot.py", "--board", board.name,
                    "--fpga-build", initial, "--directory", boot_build], cwd=ROOT, check=True)
    factory = directory / "loader"
    pair = directory / "pair.json"
    fpga_pair.pair(initial, fpga_elf, pair)
    factory_fit = directory / "factory.itb"
    bundle.create(fpga_elf, build, keys, factory_fit, factory, board, fpga_manifest=pair, boot_build=boot_build)
    factory_before = {file.name: fpga_pair.digest(file) for file in factory.iterdir()}
    confirmed_manifest = json.loads(confirmed_elf.with_suffix(".json").read_text())
    confirmed_build = confirmed_manifest["build_id"]
    confirmed_fit = directory / "confirmed.itb"
    bundle.create(confirmed_elf, confirmed_build, keys, confirmed_fit, board=board, factory_loader=factory)
    candidate = directory / "candidate.itb"
    fpga_pair.pair(incompatible, fpga_elf, pair)
    try:
        bundle.create(fpga_elf, build, keys, candidate, board=board, fpga_manifest=pair, factory_loader=factory)
    except ValueError as error:
        assert "PS initialization differs" in str(error), error
    else:
        raise AssertionError("Signed an FPGA update with changed PS initialization")
    assert not candidate.exists()
    fpga_pair.pair(next_route, fpga_elf, pair)
    manifest = bundle.create(fpga_elf, build, keys, candidate, board=board, fpga_manifest=pair, factory_loader=factory)
    assert factory_before == {file.name: fpga_pair.digest(file) for file in factory.iterdir()}

    wrong_its = directory / "wrong-ps.its"
    wrong_its.write_text(candidate.with_suffix(".its").read_text().replace(manifest["ps_contract"], "v1:" + "0" * 64))
    wrong_fit = wrong_its.with_suffix(".itb")
    subprocess.run([boot_build / "tools/mkimage", "-f", wrong_its, "-k", keys, wrong_fit],
                   cwd=ROOT, check=True)

    port, updater = 17420, 17574
    qualification = Qualification(directory, directory, directory / "evidence", port, updater, board)
    credential = (private / "credential").read_bytes()
    write_profile(qualification.profile, private / "credential", port)
    boot_files = {"BOOT    BIN": (factory / "boot.bin").read_bytes(), "U-BOOT  IMG": (factory / "u-boot.img").read_bytes()}

    def provision(name, mode=0, initial_fit=confirmed_fit, identity=confirmed_build):
        image = directory / (name + ".img")
        card.create(image, boot_files, initial_fit.read_bytes(), identity, credential, 10000, mode=mode, board=board)
        return image

    offline = provision("offline")
    with BootQemu(qualification.loader, offline, qualification.evidence / "offline", port_base=port,
                  updater_port=updater, board=board, link_down=True) as guest:
        guest.ready("SHOOP_HOST_READY", 90)
        guest.qmp("set_link", {"name": "lan", "up": True})
        assert f"build={confirmed_build}" in qualification.command("online-after-offline-boot", "status")

    abandoned = provision("abandoned", mode=1)
    before = snapshot(abandoned)
    with qualification.vm(abandoned, "abandoned") as guest:
        guest.ready("Listening for fastboot", 60)
        qualification.command("wrong-ps", "stage", wrong_fit, success=False)
        guest.ready("SHOOP_PS_CONTRACT_REFUSED", 10)
        qualification.command("stage-without-activation", "stage", candidate)
        guest.ready("SHOOP_INSTALL_PENDING slot=1", 10)
        guest.ready("Fastboot UDP idle timeout", 60)
        guest.ready("SHOOP_HOST_READY", 90)
        assert [entry["build"] for entry in boots(guest.serial.read_text())] == [confirmed_build]
    after = qualification.protected(abandoned, before, "abandoned")
    assert all(item["confirmed"] == 0 and item["pending"] == 0xffffffff for item in after["state"])

    trial = provision("trial")
    before = snapshot(trial)
    with qualification.vm(trial, "trial") as guest:
        guest.ready("SHOOP_HOST_READY", 90)
        result = qualification.command("deploy-fpga", "deploy", candidate, success=False)
        assert "candidate rolled back to " + confirmed_build in result, result
        guest.ready("SHOOP_FPGA_TRIAL_ROLLBACK", 10)
        guest.qmp("query-status")
        assert len([event for event in guest.events if event["event"] == "RESET"]) == 3
        serial = guest.serial.read_text()
        assert serial.count("SHOOP_FPGA_PROGRAM_REFUSED") == 1
        history = boots(serial)
        assert [entry["build"] for entry in history] == [confirmed_build, confirmed_build]
        assert history[0]["nonce"] < history[1]["nonce"]
    after = qualification.protected(trial, before, "trial")
    assert all(item["confirmed"] == 0 and item["pending"] == 0xffffffff for item in after["state"])

    broken = provision("broken", initial_fit=factory_fit, identity=build)
    with qualification.vm(broken, "broken") as guest:
        guest.ready("SHOOP_FPGA_PROGRAM_REFUSED", 60)
        guest.ready("Listening for fastboot", 60)
        with FastbootFixture(updater) as client:
            assert client.command("getvar:shoop-profile") == b"OKAY" + board.profile.encode()
        assert not boots(guest.serial.read_text())

    return {"synthetic_fabric": True, "boot_with_link_down": True, "compatible_xsa_update": True,
            "incompatible_ps_refused_by_signer_and_loader": True, "factory_loader_unchanged": True,
            "abandoned_install_returned_to_confirmed_slot": True, "fpga_program_failure_rolled_back": True,
            "updater_available_with_no_bootable_slot": True}
