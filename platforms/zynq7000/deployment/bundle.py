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
import tempfile
from layout import LAYOUT, boards
import fpga_pair

ROOT = Path(__file__).resolve().parents[3]
BOOT = ROOT / "work/zynq/u-boot-build"
LOADER_FILES = ("control.dtb", "u-boot.bin", "boot.bin", "u-boot.img")


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


def create(elf, build, key_dir, output, loader_output=None, board=None, *, validate_firmware=True,
           fpga_manifest=None, factory_loader=None, boot_build=None):
    if (loader_output is None) == (factory_loader is None):
        raise ValueError("Select either a factory loader output or an existing factory loader")
    board = board or boards.load()
    boot = boot_build or board.boot_dir(ROOT)
    if factory_loader is None:
        loader = json.loads((boot / "shoop-build.json").read_text())
        ps_contract = loader.get("fpga_ps", {}).get("contract")
    else:
        if output.resolve().is_relative_to(factory_loader.resolve()):
            raise ValueError("Write the update outside the factory loader directory")
        loader = json.loads((factory_loader / "factory.json").read_text())
        ps_contract = loader.get("ps_contract")
        if loader.get("loader_files") != {name: fpga_pair.digest(factory_loader / name) for name in LOADER_FILES}:
            raise ValueError("Factory loader files differ from the provisioning manifest")
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
        if bool(firmware.get("fpga_renderer") or firmware.get("native_i2s")) != (fpga_manifest is not None):
            raise ValueError("Fabric-dependent firmware requires its matching FPGA pair")
    if not re.fullmatch(r"[A-Za-z0-9_.-]{1,40}", build):
        raise ValueError("Build identity must contain 1–40 ASCII letters, digits, dots, dashes or underscores")
    if not elf.read_bytes().startswith(b"\x7fELF\x01\x01"):
        raise ValueError("A little-endian ELF32 firmware is required")
    for name in ["deployment.key", "deployment.crt"]:
        if not (key_dir / name).is_file():
            raise ValueError("Explicit deployment.key and deployment.crt are required")
    output.parent.mkdir(parents=True, exist_ok=True)
    fabric_image = fabric_config = ""
    signed_images = '"firmware"'
    fabric_sha256 = None
    if fpga_manifest is not None:
        pair = fpga_pair.verify(fpga_manifest)
        artifacts = pair["artifacts"]
        if (not validate_firmware or
                not (pair.get("renderer") or pair.get("native_i2s")) or
                bool(firmware.get("native_i2s")) != bool(pair.get("native_i2s")) or
                firmware["fpga_contract"] != pair["contract"] or
                artifacts["firmware"]["sha256"] != firmware["elf_sha256"]):
            raise ValueError("FPGA pair and deployment firmware differ")
        xsa = fpga_manifest.parent / artifacts["zturn_v2.xsa"]["path"]
        if ps_contract != fpga_pair.ps_contract(board, fpga_pair.ps_init_sources(xsa)):
            raise ValueError("FPGA PS initialization differs from the factory loader contract")
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
        fabric_config = f'fpga = "fabric"; shoop,ps-contract = "{ps_contract}";'
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
    if factory_loader is None:
        loader_output.mkdir(parents=True, exist_ok=True)
        control = loader_output / "control.dtb"
        shutil.copyfile(boot / "u-boot.dtb", control)
        key_options = ["-K", control, "-r"]
    else:
        control = factory_loader / "control.dtb"
        key_options = []
    with tempfile.TemporaryDirectory(dir=output.parent) as temporary:
        signed_fit = Path(temporary) / output.name
        subprocess.run([mkimage, "-f", its, "-k", key_dir, *key_options, signed_fit], check=True, env=environment)
        if signed_fit.stat().st_size > LAYOUT["bundle_max_bytes"]:
            raise ValueError("Signed bundle exceeds the 32 MiB bound")
        subprocess.run([boot / "tools/fit_check_sign", "-f", signed_fit, "-k", control], check=True)
        signed_fit.replace(output)
    if factory_loader is None:
        nodtb = (boot / "u-boot-nodtb.bin").read_bytes()
        (loader_output / "u-boot.bin").write_bytes(nodtb + control.read_bytes())
        shutil.copyfile(boot / "spl/boot.bin", loader_output / "boot.bin")
        subprocess.run([mkimage, "-f", "auto", "-A", "arm", "-T", "firmware", "-C", "none", "-O", "u-boot",
                        "-a", "0x4000000", "-e", "0x4000000", "-E", "-b", control,
                        "-d", boot / "u-boot-nodtb.bin", loader_output / "u-boot.img"], check=True, env=environment)
        fpga_pair.write_json(loader_output / "board.json", board.manifest())
        loader_files = {name: fpga_pair.digest(loader_output / name) for name in LOADER_FILES}
        fpga_pair.write_json(loader_output / "factory.json", {
            **board.manifest(), "ps_contract": ps_contract, "loader_files": loader_files,
        })
    else:
        loader_files = loader["loader_files"]
    manifest = {**board.manifest(), "build": build,
                "elf_sha256": hashlib.sha256(elf.read_bytes()).hexdigest(),
                "bundle_sha256": hashlib.sha256(output.read_bytes()).hexdigest(),
                "bundle_bytes": output.stat().st_size,
                "loader_files": loader_files}
    if fabric_sha256 is not None:
        manifest["fpga_payload_sha256"] = fabric_sha256
        manifest["ps_contract"] = ps_contract
    output.with_suffix(".json").write_text(json.dumps(manifest, indent=2) + "\n")
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--elf", type=Path, required=True)
    parser.add_argument("--build-id", required=True)
    parser.add_argument("--key-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    loader = parser.add_mutually_exclusive_group(required=True)
    loader.add_argument("--loader-output", type=Path, help="Build the key-bearing factory loader here")
    loader.add_argument("--factory-loader", type=Path, help="Sign an update for this provisioned loader")
    parser.add_argument("--boot-build", type=Path, help="Use tools from a separate loader build directory")
    parser.add_argument("--fpga-pair", type=Path)
    boards.argument(parser)
    args = parser.parse_args()
    create(args.elf, args.build_id, args.key_dir, args.output, args.loader_output, boards.load(args.board),
           fpga_manifest=args.fpga_pair, factory_loader=args.factory_loader, boot_build=args.boot_build)


if __name__ == "__main__":
    main()
