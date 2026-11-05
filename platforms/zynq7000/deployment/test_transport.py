"""Test fixtures for hostile/incomplete Fastboot packets and lost UDP replies."""
import socket
import struct
import threading
import time


class FastbootFixture:
    """Intentionally exposes raw commands for negative tests only."""
    def __init__(self, port, timeout=30):
        self.socket = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.socket.connect(("127.0.0.1", port))
        self.socket.settimeout(.2)
        self.deadline = time.monotonic() + timeout
        self.sequence = 0
        query = self.exchange(1, b"")
        self.sequence = struct.unpack("!H", query)[0]
        assert self.exchange(2, b"") == b"\0\1\4\0"

    def exchange(self, kind, payload):
        packet = struct.pack("!BBH", kind, 0, self.sequence) + payload
        while time.monotonic() < self.deadline:
            self.socket.send(packet)
            try:
                reply = self.socket.recv(1025)
            except socket.timeout:
                continue
            if len(reply) < 4 or reply[:4] != packet[:4]:
                continue
            if kind != 1:
                self.sequence = (self.sequence + 1) & 65535
            return reply[4:]
        raise TimeoutError("Test Fastboot peer did not respond")

    def command(self, command):
        assert self.exchange(3, command.encode()) == b""
        return self.exchange(3, b"")

    def close(self):
        self.socket.close()

    def __enter__(self):
        return self

    def __exit__(self, *args):
        self.close()


class LossyUdp:
    def __init__(self, port, seed=1):
        import random
        self.random = random.Random(seed)
        self.target = ("127.0.0.1", port)
        self.socket = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.socket.bind(("127.0.0.1", 0))
        self.socket.settimeout(.1)
        self.port = self.socket.getsockname()[1]
        self.stop = threading.Event()
        self.client = None
        self.dropped = self.replies = 0
        self.install_reply_dropped = False
        self.reboot = False
        self.thread = threading.Thread(target=self._run, daemon=True)
        self.thread.start()

    def _run(self):
        while not self.stop.is_set():
            try:
                data, peer = self.socket.recvfrom(2048)
            except socket.timeout:
                continue
            except OSError:
                return
            if peer == self.target:
                self.replies += 1
                # Lose a verified-install reply as well as seeded transfer ACKs.
                install = b"OKAYcandidate verified" in data and not self.install_reply_dropped
                if not self.reboot and (install or self.random.randrange(100) == 0):
                    self.install_reply_dropped |= install
                    self.dropped += 1
                    continue
                if self.client:
                    self.socket.sendto(data, self.client)
            else:
                self.client = peer
                if data[4:] == b"reboot":
                    self.reboot = True
                self.socket.sendto(data, self.target)

    def close(self):
        self.stop.set()
        self.thread.join(timeout=2)
        self.socket.close()
        assert not self.thread.is_alive()

    def __enter__(self):
        return self

    def __exit__(self, *args):
        self.close()
