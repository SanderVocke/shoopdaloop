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
from layout import LAYOUT

ROOT = Path(__file__).resolve().parents[3]
BOOT = ROOT / "work/zynq/u-boot-build"


def create(elf, build, key_dir, output, loader_output):
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
    shutil.copyfile(BOOT / "u-boot.dtb", control)
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
        }};
    }};
    configurations {{
        default = "conf";
        conf {{
            firmware = "firmware";
            shoop,profile = "{LAYOUT['profile']}";
            shoop,build = "{build}";
            signature {{
                algo = "sha256,rsa2048"; key-name-hint = "deployment";
                sign-images = "firmware";
            }};
        }};
    }};
}};
''')
    environment = {**os.environ, "SOURCE_DATE_EPOCH": "0"}
    mkimage = BOOT / "tools/mkimage"
    subprocess.run([mkimage, "-f", its, "-k", key_dir, "-K", control, "-r", output], check=True, env=environment)
    if output.stat().st_size > LAYOUT["bundle_max_bytes"]:
        raise ValueError("Signed bundle exceeds the 32 MiB bound")
    subprocess.run([BOOT / "tools/fit_check_sign", "-f", output, "-k", control], check=True)
    nodtb = (BOOT / "u-boot-nodtb.bin").read_bytes()
    (loader_output / "u-boot.bin").write_bytes(nodtb + control.read_bytes())
    shutil.copyfile(BOOT / "spl/boot.bin", loader_output / "boot.bin")
    subprocess.run([mkimage, "-f", "auto", "-A", "arm", "-T", "firmware", "-C", "none", "-O", "u-boot",
                    "-a", "0x4000000", "-e", "0x4000000", "-E", "-b", control,
                    "-d", BOOT / "u-boot-nodtb.bin", loader_output / "u-boot.img"], check=True, env=environment)
    manifest = {"profile": LAYOUT["profile"], "build": build,
                "elf_sha256": hashlib.sha256(elf.read_bytes()).hexdigest(),
                "bundle_sha256": hashlib.sha256(output.read_bytes()).hexdigest(),
                "bundle_bytes": output.stat().st_size,
                "loader_files": {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in
                    [control, loader_output / "u-boot.bin", loader_output / "boot.bin", loader_output / "u-boot.img"]}}
    output.with_suffix(".json").write_text(json.dumps(manifest, indent=2) + "\n")
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--elf", type=Path, required=True)
    parser.add_argument("--build-id", required=True)
    parser.add_argument("--key-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--loader-output", type=Path, required=True)
    args = parser.parse_args()
    create(args.elf, args.build_id, args.key_dir, args.output, args.loader_output)


if __name__ == "__main__":
    main()
