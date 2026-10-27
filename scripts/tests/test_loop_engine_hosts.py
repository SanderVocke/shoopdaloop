"""The browser failure gate requires Rust witnesses, not just matching error text."""
import copy
import sys
import struct
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from run_loop_engine_hosts import validate_semantic, verify_failure


class FailureGateTests(unittest.TestCase):
    def report(self, kind):
        return {
            "passed": False,
            "failure": "intentional loop " + ("processor" if kind == "processor" else "Rust heap") + " failure canary detected",
            "workletFailure": {"code": 10 if kind == "processor" else 0xffff0001,
                               "processor": 1 if kind == "processor" else 2,
                               "heap": [0, 0, 0] if kind == "processor" else [1, 7, 1]},
        }

    def test_matching_rust_witnesses_and_nonzero_exit_are_required(self):
        for kind in ("processor", "rust-heap"):
            report = self.report(kind)
            self.assertEqual(verify_failure(report, 1, kind)["witness"], report["workletFailure"])
            with self.assertRaises(RuntimeError):
                verify_failure(report, 0, kind)
            for field, value in (("passed", True), ("workletFailure", None), ("failure", "unrelated exception")):
                altered = copy.deepcopy(report)
                altered[field] = value
                with self.assertRaises(RuntimeError):
                    verify_failure(altered, 1, kind)

    def test_unrelated_fault_and_ineffective_heap_counts_cannot_pass(self):
        for kind in ("processor", "rust-heap"):
            for field, value in (("processor", 0), ("code", None), ("heap", []), ("heap", ["1", 7, 1])):
                altered = self.report(kind)
                altered["workletFailure"][field] = value
                with self.assertRaises(RuntimeError):
                    verify_failure(altered, 1, kind)
        for counts in ([0, 7, 1], [1, 0, 1], [1, 7, 0], [1, -1, 1], [True, 7, 1]):
            altered = self.report("rust-heap")
            altered["workletFailure"]["heap"] = counts
            with self.assertRaises(RuntimeError):
                verify_failure(altered, 1, "rust-heap")


class SemanticJournalTests(unittest.TestCase):
    def sample(self):
        data = bytearray(40 + 3 * 160)
        data[:8] = b"LSJ1\x01\x00\xa0\x00"
        struct.pack_into("<I", data, 8, 3)
        frame = (1 << 64) - 1
        for index, kind in enumerate((1, 2, 3)):
            at = 40 + index * 160
            data[at] = kind
            struct.pack_into("<Q", data, at + 8, frame)
            if index:
                struct.pack_into("<I", data, at + 20, 1)
        struct.pack_into("<II", data, 40 + 16, 1, 1)
        data[40 + 2 * 160 + 1] = 9
        return data

    def test_complete_full_width_records_and_all_truncations(self):
        data = self.sample()
        self.assertEqual(validate_semantic(data), {"boundaries": [str((1 << 64) - 1)],
            "decisions": 1, "events": 1, "records": 3, "loss": ["0", "0", "0"]})
        for length in range(len(data)):
            with self.assertRaises(RuntimeError):
                validate_semantic(data[:length])
        with self.assertRaises(RuntimeError):
            validate_semantic(data + b"\x00")

    def test_padding_flags_identities_counts_and_optional_fields_are_canonical(self):
        positions = [0, 4, 6, 12, 40 + 1, 40 + 4, 40 + 32,
                     200 + 3, 200 + 4, 200 + 72, 200 + 80, 200 + 96, 200 + 104,
                     360 + 2, 360 + 4, 360 + 24, 360 + 32, 360 + 40]
        for position in positions:
            data = self.sample()
            data[position] ^= 1
            with self.assertRaises(RuntimeError, msg=str(position)):
                validate_semantic(data)
        for at, value in ((8, 129), (56, 3), (60, 2), (220, 0), (380, 0)):
            data = self.sample()
            struct.pack_into("<I", data, at, value)
            with self.assertRaises(RuntimeError):
                validate_semantic(data)
        for at, value in ((200, 3), (201, 3), (202, 128), (360, 2), (361, 0), (361, 15)):
            data = self.sample()
            data[at] = value
            with self.assertRaises(RuntimeError):
                validate_semantic(data)

    def test_loss_must_be_explicitly_accepted_and_batches_must_be_ordered(self):
        for loss in ((1, 3, 0), (0, 0, 7)):
            data = self.sample()
            struct.pack_into("<QQQ", data, 16, *loss)
            with self.assertRaises(RuntimeError):
                validate_semantic(data)
            self.assertEqual(validate_semantic(data, complete=False)["loss"], list(map(str, loss)))
        data = self.sample()
        data.extend(data[40:])
        struct.pack_into("<I", data, 8, 6)
        with self.assertRaises(RuntimeError):
            validate_semantic(data)
        data = self.sample()
        struct.pack_into("<QQ", data, 16, 1, 0)
        with self.assertRaises(RuntimeError):
            validate_semantic(data, complete=False)


if __name__ == "__main__":
    unittest.main()
