"""Publish packet timing/headers while keeping trusted-LAN auth payloads private."""
import os
from pathlib import Path
import struct


def checksum(data):
    if len(data) & 1:
        data += b"\0"
    total = sum(struct.unpack("!" + "H" * (len(data) // 2), data))
    while total >> 16:
        total = (total & 65535) + (total >> 16)
    return (~total) & 65535


def redact_frame(raw):
    packet = bytearray(raw)
    if len(packet) < 14:
        return bytes(len(packet))
    kind, offset = struct.unpack_from("!H", packet, 12)[0], 14
    for _ in range(2):
        if kind not in {0x8100, 0x88a8}:
            break
        if len(packet) < offset + 4:
            packet[offset:] = bytes(len(packet) - offset)
            return bytes(packet)
        kind = struct.unpack_from("!H", packet, offset + 2)[0]; offset += 4
    if kind == 0x0806:  # ARP carries public link/network addresses.
        if len(packet) >= offset + 28 and packet[offset + 4:offset + 6] == b"\6\4":
            packet[offset + 28:] = bytes(len(packet) - offset - 28)
        else:
            packet[offset:] = bytes(len(packet) - offset)
        return bytes(packet)
    if kind != 0x0800 or len(packet) < offset + 20:
        packet[offset:] = bytes(len(packet) - offset)
        return bytes(packet)
    header = (packet[offset] & 15) * 4
    size = struct.unpack_from("!H", packet, offset + 2)[0]
    if header < 20 or size < header or offset + header > len(packet):
        packet[offset:] = bytes(len(packet) - offset)
        return bytes(packet)
    end = min(offset + size, len(packet))
    packet[end:] = bytes(len(packet) - end)
    start = offset + header
    # UDP contains public signed bundles/Fastboot status in this deployment.
    if packet[offset + 9] == 17:
        return bytes(packet)
    fragments = struct.unpack_from("!H", packet, offset + 6)[0] & 0x3fff
    if packet[offset + 9] != 6 or fragments or end < start + 20:
        packet[start:end] = bytes(end - start)
        return bytes(packet)
    tcp_header = (packet[start + 12] >> 4) * 4
    if tcp_header < 20 or start + tcp_header > end:
        packet[start:end] = bytes(end - start)
        return bytes(packet)
    packet[start + tcp_header:end] = bytes(end - start - tcp_header)
    if offset + size <= len(packet):
        packet[start + 16:start + 18] = b"\0\0"
        pseudo = bytes(packet[offset + 12:offset + 20]) + b"\0\6" + struct.pack("!H", size - header)
        struct.pack_into("!H", packet, start + 16, checksum(pseudo + bytes(packet[start:end])))
    return bytes(packet)


def publish(path, private):
    """Retain the raw file privately; publish complete records with redacted payloads."""
    private.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    original = private
    suffix = 0
    while private.exists():
        suffix += 1
        private = original.with_name(original.name + f".{suffix}")
    path.rename(private); private.chmod(0o600)
    destination = path.with_suffix(".redacted.pcap")
    temporary = destination.with_suffix(".pcap.redacting")
    packets, truncated = 0, False
    try:
        with private.open("rb") as source, temporary.open("xb") as output:
            header = source.read(24)
            if len(header) != 24:
                raise ValueError("Truncated PCAP header")
            if header[:4] in {b"\xd4\xc3\xb2\xa1", b"\x4d\x3c\xb2\xa1"}:
                endian = "<"
            elif header[:4] in {b"\xa1\xb2\xc3\xd4", b"\xa1\xb2\x3c\x4d"}:
                endian = ">"
            else:
                raise ValueError("Unsupported capture format")
            if struct.unpack_from(endian + "I", header, 20)[0] != 1:
                raise ValueError("Expected Ethernet capture")
            output.write(header)
            while record := source.read(16):
                if len(record) != 16:
                    truncated = True; break
                length = struct.unpack_from(endian + "I", record, 8)[0]
                if length > 1024 * 1024:
                    raise ValueError("Capture record exceeds the frame bound")
                raw = source.read(length)
                if len(raw) != length:
                    truncated = True; break
                output.write(record); output.write(redact_frame(raw)); packets += 1
            output.flush(); os.fsync(output.fileno())
        temporary.replace(destination)
    finally:
        temporary.unlink(missing_ok=True)
    return {"packets": packets, "tcp_payload": "redacted", "truncated_tail_dropped": truncated}
