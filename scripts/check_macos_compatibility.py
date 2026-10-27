#!/usr/bin/env python3
"""Check the selected architecture of every Mach-O in a portable macOS bundle."""

from __future__ import annotations

import argparse
import posixpath
import struct
from pathlib import Path


CPU_TYPES = {"x86_64": 0x01000007, "arm64": 0x0100000C}
MACHO_MAGICS = {b"\xcf\xfa\xed\xfe", b"\xce\xfa\xed\xfe", b"\xca\xfe\xba\xbe", b"\xca\xfe\xba\xbf"}


def version(value: str) -> tuple[int, int, int]:
    parts = tuple(int(part) for part in value.split("."))
    if not 2 <= len(parts) <= 3 or any(part < 0 or part > 255 for part in parts):
        raise ValueError(f"invalid macOS version: {value}")
    return (*parts, *([0] * (3 - len(parts))))


def inspect_macho(data: bytes, arch: str) -> tuple[tuple[int, int, int], list[str]] | None:
    if data[:4] not in MACHO_MAGICS:
        return None
    cpu = CPU_TYPES[arch]
    if data[:4] in {b"\xca\xfe\xba\xbe", b"\xca\xfe\xba\xbf"}:
        count = struct.unpack_from(">I", data, 4)[0]
        wide = data[:4] == b"\xca\xfe\xba\xbf"
        stride = 32 if wide else 20
        for index in range(count):
            entry = 8 + index * stride
            if struct.unpack_from(">I", data, entry)[0] == cpu:
                offset, size = struct.unpack_from(">QQ" if wide else ">II", data, entry + 8)
                if offset + size > len(data):
                    raise ValueError("truncated universal Mach-O slice")
                data = data[offset:offset + size]
                break
        else:
            raise ValueError(f"Mach-O has no {arch} slice")
    if data[:4] != b"\xcf\xfa\xed\xfe" or struct.unpack_from("<I", data, 4)[0] != cpu:
        raise ValueError(f"Mach-O has no {arch} slice")
    count, size = struct.unpack_from("<II", data, 16)
    end = 32 + size
    if end > len(data):
        raise ValueError("truncated Mach-O load commands")
    position = 32
    minimum = None
    dependencies = []
    for _ in range(count):
        if position + 8 > end:
            raise ValueError("truncated Mach-O load command")
        command, size = struct.unpack_from("<II", data, position)
        if size < 8 or position + size > end:
            raise ValueError("invalid Mach-O load command size")
        if command in {0x24, 0x32}:
            if size < (24 if command == 0x32 else 16):
                raise ValueError("truncated Mach-O minimum version")
            if command == 0x32 and struct.unpack_from("<I", data, position + 8)[0] != 1:
                raise ValueError("Mach-O does not target macOS")
            encoded = struct.unpack_from("<I", data, position + (12 if command == 0x32 else 8))[0]
            minimum = (encoded >> 16, (encoded >> 8) & 255, encoded & 255)
        elif command in {0xC, 0x80000018, 0x8000001F, 0x80000023, 0x20}:
            if size < 24:
                raise ValueError("truncated Mach-O dylib command")
            offset = struct.unpack_from("<I", data, position + 8)[0]
            if offset < 24 or offset >= size:
                raise ValueError("invalid Mach-O dependency name")
            dependencies.append(data[position + offset:position + size].split(b"\0", 1)[0].decode("utf-8"))
        position += size
    if minimum is None:
        raise ValueError("Mach-O has no macOS minimum version")
    return minimum, dependencies


def verify_payloads(payloads: dict[str, bytes], arch: str, minimum_os: str) -> list[str]:
    maximum = version(minimum_os)
    report = []
    errors = []
    for name, data in sorted(payloads.items()):
        try:
            info = inspect_macho(data, arch)
            if info is None:
                continue
            minimum, dependencies = info
            report.append(f"{name}: {arch}, minimum macOS {'.'.join(map(str, minimum))}")
            if minimum > maximum:
                raise ValueError(f"requires macOS {'.'.join(map(str, minimum))}, newer than {minimum_os}")
            for dependency in dependencies:
                if dependency.startswith(("/System/Library/", "/usr/lib/")):
                    continue
                if dependency.startswith(("@loader_path/", "@executable_path/")):
                    target = posixpath.normpath(posixpath.join(posixpath.dirname(name), dependency.split("/", 1)[1]))
                    if target in payloads:
                        continue
                elif dependency.startswith("@rpath/"):
                    suffix = dependency.split("/", 1)[1]
                    if any(path.endswith("/" + suffix) for path in payloads):
                        continue
                raise ValueError(f"unbundled dependency: {dependency}")
        except (ValueError, struct.error) as error:
            errors.append(f"{name}: {error}")
    if errors:
        raise ValueError("macOS compatibility audit failed:\n" + "\n".join(errors))
    if not report:
        raise ValueError("macOS compatibility audit found no Mach-O files")
    return report


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--arch", choices=CPU_TYPES, required=True)
    parser.add_argument("--minimum-os", required=True)
    args = parser.parse_args()
    payloads = {str(path.relative_to(args.root)): path.read_bytes() for path in args.root.rglob("*") if path.is_file()}
    print("\n".join(verify_payloads(payloads, args.arch, args.minimum_os)))


if __name__ == "__main__":
    main()
