#!/usr/bin/env python3
"""Sign a bounded firmware FIT and embed its verification key in the loader."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
from layout import LAYOUT, boards
import fpga_pair

ROOT = Path(__file__).resolve().parents[3]
BOOT = ROOT / "work/zynq/u-boot-build"


def bitstream_data(bit):
    def field(data, offset):
        end = offset + 2 + int.from_bytes(data[offset:offset + 2], "big")
        if offset + 2 > len(data) or end > len(data):
            raise ValueError("Truncated Vivado bitstream metadata")
        return end

    offset = field(bit, 0)
    if bit[offset:offset + 3] != b"\x00\x01a":
        raise ValueError("Missing Vivado bitstream design field")
    offset = field(bit, offset + 3)
    for tag in b"bcd":
        if bit[offset:offset + 1] != bytes([tag]):
            raise ValueError("Unexpected Vivado bitstream metadata")
        offset = field(bit, offset + 1)
    size = int.from_bytes(bit[offset + 1:offset + 5], "big")
    payload = bit[offset + 5:]
    if (bit[offset:offset + 1] != b"e" or size < 128 or size != len(payload) or size % 4
            or b"\xaa\x99\x55\x66" not in payload[:256]):
        raise ValueError("Invalid Vivado configuration payload")
    return payload


def create(elf, build, key_dir, output, loader_output, board=None, *, validate_firmware=True, fpga_manifest=None):
    board = board or boards.load()
    boot = board.boot_dir(ROOT)
    loader = json.loads((boot / "shoop-build.json").read_text())
    if loader.get("board_sha256") != board.manifest()["board_sha256"]:
        raise ValueError("Loader does not match the selected board contract")
    if validate_firmware:
        firmware_manifest = elf.with_suffix(".json")
        if not firmware_manifest.exists():
            firmware_manifest = elf.with_name(elf.stem + "-build.json")
        firmware = json.loads(firmware_manifest.read_text())
        if (firmware.get("board_sha256") != board.manifest()["board_sha256"] or
                firmware.get("elf_sha256") != hashlib.sha256(elf.read_bytes()).hexdigest() or
                firmware.get("build_id") != build or not firmware.get("deployment")):
            raise ValueError("Firmware identity, digest or deployment contract differs")
        if bool(firmware.get("fpga_renderer")) != (fpga_manifest is not None):
            raise ValueError("An FPGA renderer bundle requires its matching FPGA pair")
    if not re.fullmatch(r"[A-Za-z0-9_.-]{1,40}", build):
        raise ValueError("Build identity must contain 1–40 ASCII letters, digits, dots, dashes or underscores")
    if not elf.read_bytes().startswith(b"\x7fELF\x01\x01"):
        raise ValueError("A little-endian ELF32 firmware is required")
    for name in ["deployment.key", "deployment.crt"]:
        if not (key_dir / name).is_file():
            raise ValueError("Explicit deployment.key and deployment.crt are required")
    output.parent.mkdir(parents=True, exist_ok=True)
    loader_output.mkdir(parents=True, exist_ok=True)
    control = loader_output / "control.dtb"
    shutil.copyfile(boot / "u-boot.dtb", control)
    fabric_image = fabric_config = ""
    signed_images = '"firmware"'
    fabric_sha256 = None
    if fpga_manifest is not None:
        pair = fpga_pair.verify(fpga_manifest)
        artifacts = pair["artifacts"]
        if (not validate_firmware or not pair.get("renderer") or firmware["fpga_contract"] != pair["contract"] or
                artifacts["firmware"]["sha256"] != firmware["elf_sha256"]):
            raise ValueError("FPGA pair and deployment firmware differ")
        raw = output.with_suffix(".fpga.bin")
        bit = fpga_manifest.parent / artifacts["zturn_v2.bit"]["path"]
        payload = bitstream_data(bit.read_bytes())
        raw.write_bytes(payload)
        fabric_sha256 = hashlib.sha256(payload).hexdigest()
        fabric_image = f'''
        fabric {{
            description = "Shoop Z-turn V2 programmable logic";
            data = /incbin/({json.dumps(str(raw.resolve()))});
            type = "fpga"; arch = "arm"; compression = "none";
            hash {{ algo = "sha256"; }};
        }};'''
        fabric_config = 'fpga = "fabric";'
        signed_images += ', "fpga"'
    its = output.with_suffix(".its")
    its.write_text(f'''/dts-v1/;
/ {{
    description = "Shoop Zynq firmware";
    #address-cells = <1>;
    images {{
        firmware {{
            description = "FreeRTOS Shoop host";
            data = /incbin/({json.dumps(str(elf.resolve()))});
            type = "firmware"; arch = "arm"; os = "u-boot"; compression = "none";
            hash {{ algo = "sha256"; }};
        }};{fabric_image}
    }};
    configurations {{
        default = "conf";
        conf {{
            firmware = "firmware"; {fabric_config}
            shoop,profile = "{board.profile}";
            shoop,build = "{build}";
            signature {{
                algo = "sha256,rsa2048"; key-name-hint = "deployment";
                sign-images = {signed_images};
            }};
        }};
    }};
}};
''')
    environment = {**os.environ, "SOURCE_DATE_EPOCH": "0"}
    mkimage = boot / "tools/mkimage"
    subprocess.run([mkimage, "-f", its, "-k", key_dir, "-K", control, "-r", output], check=True, env=environment)
    if output.stat().st_size > LAYOUT["bundle_max_bytes"]:
        raise ValueError("Signed bundle exceeds the 32 MiB bound")
    subprocess.run([boot / "tools/fit_check_sign", "-f", output, "-k", control], check=True)
    nodtb = (boot / "u-boot-nodtb.bin").read_bytes()
    (loader_output / "u-boot.bin").write_bytes(nodtb + control.read_bytes())
    shutil.copyfile(boot / "spl/boot.bin", loader_output / "boot.bin")
    subprocess.run([mkimage, "-f", "auto", "-A", "arm", "-T", "firmware", "-C", "none", "-O", "u-boot",
                    "-a", "0x4000000", "-e", "0x4000000", "-E", "-b", control,
                    "-d", boot / "u-boot-nodtb.bin", loader_output / "u-boot.img"], check=True, env=environment)
    (loader_output / "board.json").write_text(json.dumps(board.manifest(), indent=2) + "\n")
    manifest = {**board.manifest(), "build": build,
                "elf_sha256": hashlib.sha256(elf.read_bytes()).hexdigest(),
                "bundle_sha256": hashlib.sha256(output.read_bytes()).hexdigest(),
                "bundle_bytes": output.stat().st_size,
                "loader_files": {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in
                    [control, loader_output / "u-boot.bin", loader_output / "boot.bin", loader_output / "u-boot.img"]}}
    if fabric_sha256 is not None:
        manifest["fpga_payload_sha256"] = fabric_sha256
    output.with_suffix(".json").write_text(json.dumps(manifest, indent=2) + "\n")
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--elf", type=Path, required=True)
    parser.add_argument("--build-id", required=True)
    parser.add_argument("--key-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--loader-output", type=Path, required=True)
    parser.add_argument("--fpga-pair", type=Path)
    boards.argument(parser)
    args = parser.parse_args()
    create(args.elf, args.build_id, args.key_dir, args.output, args.loader_output, boards.load(args.board),
           fpga_manifest=args.fpga_pair)


if __name__ == "__main__":
    main()
