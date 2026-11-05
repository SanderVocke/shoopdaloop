#!/usr/bin/env python3
"""Provision a regular-file SD image without mounting or block-device access."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import secrets
import struct
from layout import LAYOUT, boards
import records


def fat32(stream, files):
    start, sectors = LAYOUT["boot"]["lba"], LAYOUT["boot"]["sectors"]
    offset = start * 512
    fat_sectors, reserved = 1024, 32
    clusters = sectors - reserved - 2 * fat_sectors
    if not 65525 <= clusters < 0x0ffffff5 or (clusters + 2) * 4 > fat_sectors * 512:
        raise ValueError("Invalid FAT32 reference geometry")
    boot = bytearray(512)
    boot[:11] = b"\xeb\x58\x90SHOOPSD "
    struct.pack_into("<HBHBHHBHHHII", boot, 11, 512, 1, reserved, 2, 0, 0, 0xf8, 0, 63, 255, start, sectors)
    struct.pack_into("<IHHIHH", boot, 36, fat_sectors, 0, 0, 2, 1, 6)
    boot[64:67] = b"\x80\0\x29"
    struct.pack_into("<I", boot, 67, 0x53484f50)
    boot[71:90] = b"SHOOPBOOT  FAT32   "
    boot[510:512] = b"\x55\xaa"
    stream.seek(offset); stream.write(boot)
    stream.seek(offset + 6 * 512); stream.write(boot)
    fat = bytearray(fat_sectors * 512)
    struct.pack_into("<III", fat, 0, 0x0ffffff8, 0x0fffffff, 0x0fffffff)
    directory = bytearray(512)
    next_cluster = 3
    for index, (name, payload) in enumerate(sorted(files.items())):
        if len(name) != 11 or not payload:
            raise ValueError("Boot files need nonempty content and an 8.3 name")
        count = (len(payload) + 511) // 512
        first = next_cluster
        next_cluster += count
        if next_cluster > clusters + 2 or index >= 16:
            raise ValueError("Boot FAT partition exceeded")
        for cluster in range(first, next_cluster):
            struct.pack_into("<I", fat, cluster * 4, cluster + 1 if cluster + 1 < next_cluster else 0x0fffffff)
        position = offset + (reserved + 2 * fat_sectors + first - 2) * 512
        stream.seek(position); stream.write(payload)
        entry = index * 32
        directory[entry:entry + 11] = name.encode("ascii")
        directory[entry + 11] = 0x20
        struct.pack_into("<H", directory, entry + 20, first >> 16)
        struct.pack_into("<HHHI", directory, entry + 22, 0, 0x5a21, first & 0xffff, len(payload))
    info = bytearray(512)
    struct.pack_into("<I", info, 0, 0x41615252)
    struct.pack_into("<III", info, 484, 0x61417272, clusters - (next_cluster - 2), next_cluster)
    struct.pack_into("<I", info, 508, 0xaa550000)
    for sector in [1, 7]:
        stream.seek(offset + sector * 512); stream.write(info)
    for index in range(2):
        stream.seek(offset + (reserved + index * fat_sectors) * 512); stream.write(fat)
    stream.seek(offset + (reserved + 2 * fat_sectors) * 512); stream.write(directory)


def create(path, boot_files, bundle, build, credential, seed, mode=0, mac="02:00:00:00:00:01",
           ip="10.0.2.15", netmask="255.255.255.0", gateway="10.0.2.2", server="10.0.2.2", board=None):
    board = board or boards.load()
    config = records.configuration(credential, mac, ip, netmask, gateway, server, board.profile)
    digest = hashlib.sha256(bundle).digest()
    slot = records.slot(bundle, build)
    selection, counter = records.selection(digest, mode), records.counter(seed)
    descriptor = os.open(path, os.O_RDWR | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "r+b") as stream:
        stream.truncate(LAYOUT["card_bytes"])
        mbr = bytearray(512)
        for index, name in enumerate(["boot", "metadata", "slot_a", "slot_b"]):
            part = LAYOUT[name]
            struct.pack_into("<B3sB3sII", mbr, 446 + index * 16, 0x80 if index == 0 else 0,
                             b"\xfe\xff\xff", part["type"], b"\xfe\xff\xff", part["lba"], part["sectors"])
        mbr[510:512] = b"\x55\xaa"
        stream.write(mbr)
        fat32(stream, boot_files)
        for name, data in [("config", config), ("state", selection), ("counter", counter)]:
            for relative in LAYOUT[name + "_offsets"]:
                stream.seek((LAYOUT["metadata"]["lba"] + relative) * 512); stream.write(data)
        stream.seek(LAYOUT["slot_a"]["lba"] * 512); stream.write(slot); stream.write(bundle)
        stream.flush(); os.fsync(stream.fileno())
    return {"layout_version": LAYOUT["version"], "card_bytes": LAYOUT["card_bytes"], **board.manifest(),
            "build": build, "bundle_sha256": digest.hex(), "nonce_seed": seed,
            "boot_files": {name: hashlib.sha256(data).hexdigest() for name, data in boot_files.items()}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--boot-files", type=Path, required=True)
    parser.add_argument("--bundle", type=Path, required=True)
    parser.add_argument("--build-id", required=True)
    parser.add_argument("--credential-file", type=Path, required=True)
    parser.add_argument("--nonce-seed", type=int)
    parser.add_argument("--network-boot", action="store_true")
    parser.add_argument("--mac", default="02:00:00:00:00:01")
    parser.add_argument("--ip", default="10.0.2.15")
    parser.add_argument("--netmask", default="255.255.255.0")
    parser.add_argument("--gateway", default="10.0.2.2")
    parser.add_argument("--server", default="10.0.2.2")
    boards.argument(parser)
    args = parser.parse_args()
    board = boards.load(args.board)
    loader = json.loads((args.boot_files / "board.json").read_text())
    bundle_manifest = json.loads(args.bundle.with_suffix(".json").read_text())
    if loader != board.manifest() or any(bundle_manifest.get(k) != v for k, v in board.manifest().items()):
        parser.error("Loader and bundle must match the selected board")
    if (bundle_manifest.get("build") != args.build_id or
            bundle_manifest.get("bundle_sha256") != hashlib.sha256(args.bundle.read_bytes()).hexdigest()):
        parser.error("Bundle digest or build identity differs from its manifest")
    boot_files = {"BOOT    BIN": (args.boot_files / "boot.bin").read_bytes(),
                  "U-BOOT  IMG": (args.boot_files / "u-boot.img").read_bytes()}
    manifest = create(args.output, boot_files, args.bundle.read_bytes(), args.build_id,
                      args.credential_file.read_bytes(), args.nonce_seed if args.nonce_seed is not None else secrets.randbelow((1 << 64) - 2) + 1,
                      mode=2 if args.network_boot else 0, mac=args.mac, ip=args.ip, netmask=args.netmask,
                      gateway=args.gateway, server=args.server, board=board)
    args.output.with_suffix(".json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(args.output)


if __name__ == "__main__":
    main()
