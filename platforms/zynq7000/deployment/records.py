"""Factory encoding of the versioned boot records; firmware owns live writes."""
import hashlib
import ipaddress
import struct
import zlib
from layout import LAYOUT

MAGIC = 0x53484450
NO_SLOT = 0xffffffff


def record(kind, body):
    raw = struct.pack("<4I", MAGIC, LAYOUT["version"], kind, 0) + body
    if len(raw) > 508:
        raise ValueError("Boot record exceeds one sector")
    raw = raw.ljust(508, b"\0")
    return raw + struct.pack("<I", zlib.crc32(raw))


def valid(raw, kind):
    return len(raw) == 512 and struct.unpack_from("<4I", raw) == (MAGIC, LAYOUT["version"], kind, 0) and zlib.crc32(raw[:508]) == struct.unpack_from("<I", raw, 508)[0]


def configuration(credential, mac, ip, netmask, gateway, server):
    if len(credential) != 32 or not any(credential):
        raise ValueError("A nonzero 32-byte credential is required")
    mac_bytes = bytes.fromhex(mac.replace(":", ""))
    if len(mac_bytes) != 6 or not any(mac_bytes) or mac_bytes[0] & 1:
        raise ValueError("A nonzero unicast MAC address is required")
    ipaddress.IPv4Network((ip, netmask), strict=False)
    addresses = b"".join(ipaddress.IPv4Address(a).packed for a in [ip, netmask, gateway, server])
    return record(1, struct.pack("<6s2x", mac_bytes) + addresses + credential + LAYOUT["profile"].encode().ljust(32, b"\0"))


def selection(digest, mode=0):
    return record(2, struct.pack("<Q4I32s32s16sQII32s", 1, 0, NO_SLOT, 0, mode, digest, b"", b"", 0, 0, NO_SLOT, b""))


def counter(seed):
    if not 0 < seed < (1 << 64) - 1:
        raise ValueError("Nonce seed must be nonzero and below the counter limit")
    return record(3, struct.pack("<Q", seed))


def slot(bundle, build):
    encoded = build.encode("ascii")
    if not encoded or len(encoded) > 40 or b"\0" in encoded:
        raise ValueError("Build identity must contain 1–40 ASCII bytes")
    if not 0 < len(bundle) <= LAYOUT["bundle_max_bytes"]:
        raise ValueError("Bundle exceeds the deployment limit")
    return record(4, struct.pack("<I32s64s", len(bundle), hashlib.sha256(bundle).digest(), encoded))
