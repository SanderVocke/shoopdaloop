#!/usr/bin/env python3
"""Verify packet redaction, checksums, split TCP data and truncated captures."""
from pathlib import Path
import struct
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from packet_capture import checksum, publish, redact_frame


def frame(payload, fragment=0):
    ethernet = bytes.fromhex("02000000000152550a0002020800")
    ip = bytearray(20); ip[0] = 0x45; ip[8:10] = b"\x40\x06"
    ip[12:20] = bytes([10, 0, 2, 2, 10, 0, 2, 15])
    struct.pack_into("!HHH", ip, 2, 40 + len(payload), 1, fragment)
    struct.pack_into("!H", ip, 10, checksum(ip))
    tcp = bytearray(20); tcp[12:14] = b"\x50\x18"
    struct.pack_into("!HHII", tcp, 0, 12345, 15401, 123, 456)
    return ethernet + ip + tcp + payload


def main():
    marker = bytes(range(32))
    packets = [frame(marker), frame(marker[:11]), frame(marker[11:]), frame(marker, fragment=0x2000)]
    for raw in packets:
        clean = redact_frame(raw)
        assert len(clean) == len(raw) and clean[:34] == raw[:34]
        assert clean[54:] == bytes(len(raw) - 54)
        if raw[20:22] == b"\0\0":
            pseudo = clean[26:34] + b"\0\6" + struct.pack("!H", len(clean) - 34)
            assert checksum(pseudo + clean[34:]) == 0
    for endian, magic in [("<", b"\xd4\xc3\xb2\xa1"), (">", b"\xa1\xb2\xc3\xd4")]:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary); public = directory / "public.pcap"; private = directory / "private/raw.pcap"
            raw = magic + struct.pack(endian + "HHIIII", 2, 4, 0, 0, 65535, 1)
            for packet in packets:
                raw += struct.pack(endian + "4I", 1, 2, len(packet), len(packet)) + packet
            public.write_bytes(raw + b"partial")
            result = publish(public, private)
            assert result["packets"] == 4 and result["truncated_tail_dropped"]
            assert private.read_bytes() == raw + b"partial"
            redacted = public.with_suffix(".redacted.pcap")
            assert marker not in redacted.read_bytes()
            assert len(redacted.read_bytes()) == len(raw)
            assert not public.exists()
    with tempfile.TemporaryDirectory() as temporary:
        directory = Path(temporary); public = directory / "bad.pcap"; private = directory / "private/bad.pcap"
        public.write_bytes(b"invalid capture")
        try:
            publish(public, private)
        except ValueError:
            pass
        else:
            raise AssertionError("Invalid capture was published")
        assert private.exists() and not public.exists()
    print("Packet redaction: split payloads, checksums, endian formats and truncated tails passed")


if __name__ == "__main__":
    main()
