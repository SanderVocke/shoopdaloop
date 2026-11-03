"""Test-only NBD backing store: fault guest SD writes without bypassing SDHCI.

All reads/writes originate from QEMU's SD controller. Rules can omit or tear a
particular guest write, or pause it so the harness can simulate power loss.
Logs contain offsets, hashes and decoded public state, never record payloads.
"""
import errno
import hashlib
import json
import os
from pathlib import Path
import socket
import struct
import threading
import tempfile

NBD_MAGIC = 0x4e42444d41474943
OPT_MAGIC = 0x49484156454f5054
REPLY_MAGIC = 0x3e889045565a9
REQUEST_MAGIC = 0x25609513
SIMPLE_REPLY = 0x67446698
MAX_REQUEST = 32 * 1024 * 1024


def receive(connection, size):
    result = bytearray()
    while len(result) < size:
        data = connection.recv(size - len(result))
        if not data:
            raise EOFError()
        result.extend(data)
    return bytes(result)


class FaultDisk:
    def __init__(self, card, directory, rule=None):
        self.card = Path(card)
        self.directory = Path(directory).resolve()
        self.directory.mkdir(parents=True, exist_ok=True)
        self._sockets = tempfile.TemporaryDirectory(prefix="shoop-sd-")
        self.path = Path(self._sockets.name) / "disk.sock"
        self.path.unlink(missing_ok=True)
        self.url = "nbd+unix:///disk?socket=" + str(self.path)
        self.rule = rule
        self.hit = threading.Event()
        self.stop = threading.Event()
        self.error = None
        self.events = []
        self.readback = None
        self.connection = None
        self.listener = socket.socket(socket.AF_UNIX)
        self.listener.bind(str(self.path))
        self.listener.listen(1)
        self.listener.settimeout(.1)
        self.thread = threading.Thread(target=self._serve, daemon=True)
        self.thread.start()

    def _option(self, connection, option, kind, data=b""):
        connection.sendall(struct.pack("!QIII", REPLY_MAGIC, option, kind, len(data)) + data)

    def _negotiate(self, connection, size):
        connection.sendall(struct.pack("!QQH", NBD_MAGIC, OPT_MAGIC, 3))
        flags = struct.unpack("!I", receive(connection, 4))[0]
        if flags & ~3 or not flags & 1:
            raise ValueError("Unsupported NBD client flags")
        while True:
            magic, option, length = struct.unpack("!QII", receive(connection, 16))
            if magic != OPT_MAGIC or length > 4096:
                raise ValueError("Invalid NBD option")
            data = receive(connection, length)
            if option == 1:  # EXPORT_NAME
                connection.sendall(struct.pack("!QH", size, 1 | 4 | 8))
                if not flags & 2:
                    connection.sendall(bytes(124))
                return
            if option == 7:  # GO: export size and block constraints
                self._option(connection, option, 3, struct.pack("!HQH", 0, size, 1 | 4 | 8))
                self._option(connection, option, 3, struct.pack("!HIII", 3, 512, 4096, MAX_REQUEST))
                self._option(connection, option, 1)
                return
            if option == 2:  # ABORT
                self._option(connection, option, 1)
                raise EOFError()
            # Keep simple replies: structured replies / metadata contexts are unnecessary.
            self._option(connection, option, 0x80000001)

    def _serve(self):
        try:
            while not self.stop.is_set():
                try:
                    self.connection, _ = self.listener.accept()
                    break
                except socket.timeout:
                    pass
            if self.connection is None:
                return
            with self.connection as connection, self.card.open("r+b", buffering=0) as disk:
                size = os.fstat(disk.fileno()).st_size
                self._negotiate(connection, size)
                while not self.stop.is_set():
                    magic, flags, command, handle, offset, length = struct.unpack("!IHHQQI", receive(connection, 28))
                    if magic != REQUEST_MAGIC or length > MAX_REQUEST:
                        raise ValueError("Invalid NBD request")
                    data = receive(connection, length) if command == 1 else b""
                    if command == 2:  # DISC has no reply
                        break
                    error, response = 0, b""
                    if offset > size or length > size - offset:
                        error = errno.EINVAL
                    elif command == 0:
                        response = os.pread(disk.fileno(), length, offset)
                        if len(response) != length:
                            error, response = errno.EIO, b""
                        elif self.readback is not None and offset <= self.readback < offset + length:
                            corrupt = bytearray(response); corrupt[self.readback - offset] ^= 1
                            response = bytes(corrupt); self.readback = None
                            self.events.append({"offset": offset, "bytes": length, "result": "corrupted-readback"})
                    elif command == 1:
                        event = {"offset": offset, "bytes": length, "lba": offset // 512,
                                 "sha256": hashlib.sha256(data).hexdigest(), "result": "committed"}
                        if length == 512 and data[:4] == struct.pack("<I", 0x53484450):
                            event["record_kind"] = struct.unpack_from("<I", data, 8)[0]
                            if event["record_kind"] == 2:
                                event.update(zip(["generation", "confirmed", "pending", "attempts", "mode"], struct.unpack_from("<Q4I", data, 16)))
                            if event["record_kind"] == 3:
                                event["nonce"] = struct.unpack_from("<Q", data, 16)[0]
                        action = self.rule(event) if self.rule is not None and not self.hit.is_set() else None
                        if action:
                            if action not in {"omit", "tear", "cut", "readback"}:
                                raise ValueError("Unknown disk fault")
                            event["result"] = action
                            event["committed_bytes"] = length if action == "readback" else min(199, length) if action != "omit" else 0
                            if action == "readback":
                                os.pwrite(disk.fileno(), data, offset)
                                self.readback = offset
                            elif action != "omit":
                                os.pwrite(disk.fileno(), data[:199], offset)
                            os.fsync(disk.fileno())
                            self.events.append(event)
                            self.hit.set()
                            if action == "cut":
                                self.stop.wait()
                                return
                            # ACK the specified faulty storage behavior. Guest readback
                            # must catch it; an NBD error alone would test a different path.
                        else:
                            if os.pwrite(disk.fileno(), data, offset) != length:
                                raise IOError("Short backing write")
                            if flags & 1:
                                os.fsync(disk.fileno())
                            self.events.append(event)
                    elif command == 3:
                        os.fsync(disk.fileno())
                    else:
                        error = errno.ENOTSUP
                    connection.sendall(struct.pack("!IIQ", SIMPLE_REPLY, error, handle) + response)
        except (EOFError, BrokenPipeError, ConnectionResetError):
            pass
        except BaseException as error:
            if not self.stop.is_set():
                self.error = error

    def close(self):
        self.stop.set()
        if self.connection is not None:
            try:
                self.connection.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
        self.listener.close()
        self.thread.join(timeout=3)
        self.path.unlink(missing_ok=True)
        self._sockets.cleanup()
        (self.directory / "disk-writes.json").write_text(json.dumps({"writes": self.events, "injected": self.hit.is_set(), "error": str(self.error) if self.error else None}, indent=2) + "\n")
        if self.thread.is_alive():
            raise RuntimeError("NBD backing thread failed to stop")
        if self.error is not None:
            raise RuntimeError("NBD backing server failed") from self.error

    def __enter__(self):
        return self

    def __exit__(self, *args):
        self.close()
