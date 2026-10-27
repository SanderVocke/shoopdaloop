"""The browser failure gate requires Rust witnesses, not just matching error text."""
import copy
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from run_loop_engine_hosts import verify_failure


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


if __name__ == "__main__":
    unittest.main()
