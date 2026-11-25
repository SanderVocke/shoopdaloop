#!/usr/bin/env python3
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("factory_return", Path(__file__).parents[1] / "factory_return.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class Console:
    def __init__(self, chunks):
        self.chunks = iter(chunks)
        self.sent = []

    def receive(self):
        return next(self.chunks, b"")

    def send(self, data):
        self.sent.append(data)


class FactoryReturn(unittest.TestCase):
    def run_case(self, chunks, *, mode=3, boot=5, flags=0, after=5):
        self.console = Console(chunks)
        self.booted = False
        self.now = 0
        modes = iter((boot, after))

        def read(address):
            return next(modes) if address == 0xf800025c else {0xf8005000: mode, 0xf8000258: flags}[address]

        def pause(seconds):
            self.now += seconds

        def boot(console):
            self.assertIn(b"reset\r", console.sent)
            self.assertIn(b" ", console.sent)
            self.booted = True

        module.return_to_factory(self.console, read, boot, timeout=.1,
                                 clock=lambda: self.now, pause=pause)

    def test_full_reset_precedes_kernel_and_never_disables_watchdog(self):
        self.run_case([b"old Zynq> ", b"U-Boot 2018.01\nBOOTP broadcast 1", b"Hit any key to stop autoboot", b"Zynq> "])
        self.assertTrue(self.booted)
        self.assertEqual(self.console.sent, [b"reset\r", b"\x03", b" "])

    def test_countdown_is_sufficient_when_early_banner_was_lost(self):
        self.run_case([b"", b"Hit any key to stop auto", b"boot: 4", b"Zynq> "])
        self.assertTrue(self.booted)

    def test_stale_banner_and_prompt_do_not_authorize_boot(self):
        with self.assertRaises(TimeoutError):
            self.run_case([b"U-Boot 2018.01 Hit any key to stop autoboot Zynq> "])
        self.assertFalse(self.booted)
        self.assertEqual(self.console.sent, [b"reset\r"])

    def test_prompt_without_countdown_does_not_authorize_boot(self):
        with self.assertRaises(TimeoutError):
            self.run_case([b"", b"Zynq> "])
        self.assertFalse(self.booted)

    def test_prompt_before_countdown_is_not_a_fresh_prompt(self):
        with self.assertRaises(TimeoutError):
            self.run_case([b"", b"Zynq> ", b"Hit any key to stop autoboot"])
        self.assertFalse(self.booted)

    def test_countdown_without_prompt_does_not_boot(self):
        with self.assertRaises(TimeoutError):
            self.run_case([b"", b"Hit any key to stop autoboot"])
        self.assertFalse(self.booted)

    def test_disabled_or_interrupt_only_watchdog_refuses_reset(self):
        for mode in (0, 1, 2, 4, 5):
            with self.subTest(mode=mode), self.assertRaises(RuntimeError):
                self.run_case([], mode=mode)
            self.assertEqual(self.console.sent, [])

    def test_wrong_boot_source_refuses_reset(self):
        with self.assertRaises(RuntimeError):
            self.run_case([], boot=0)
        self.assertEqual(self.console.sent, [])

    def test_warm_pl_skip_flags_refuse_reset(self):
        for bit in range(24, 28):
            with self.subTest(bit=bit), self.assertRaises(RuntimeError):
                self.run_case([], flags=1 << bit)
            self.assertEqual(self.console.sent, [])

    def test_boot_mode_change_stops_at_prompt(self):
        with self.assertRaises(RuntimeError):
            self.run_case([b"", b"Hit any key to stop autoboot", b"Zynq> "], after=0)
        self.assertFalse(self.booted)


if __name__ == "__main__":
    unittest.main()
