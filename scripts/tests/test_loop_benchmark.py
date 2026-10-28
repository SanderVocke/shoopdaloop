"""Synthetic verifier inputs, not Rust execution or timing measurements."""
import csv
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

SPEC = importlib.util.spec_from_file_location("loop_benchmark", Path(__file__).parents[1] / "benchmark_loop_engine.py")
runner = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runner)


class LoopBenchmarkTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.raw = Path(self.tmp.name)
        self.records = []
        for name in sorted(runner.CASES):
            rows = [{"callback": i, "nanoseconds": t, "intervals": 1, "nodes": 3, "edges": 8,
                     "transitions": 2, "render_bytes": 1024, "maintenance_bytes": 64,
                     "render_steps": 1, "maintenance_steps": 1, "pages": 4,
                     "reserved_pages": 0, "free_versions": 8, "free_snapshots": 8,
                     "domain_pending": 0, "resource_pending": 1, "cascade_starts": 4}
                    for i, t in enumerate([100, 3000000])]
            with (self.raw / f"{name}.csv").open("w") as file:
                writer = csv.DictWriter(file, fieldnames=rows[0])
                writer.writeheader()
                writer.writerows(rows)
            self.records.append({"case": name, "callbacks": 2, "frames_per_callback": 128,
                                 "sample_rate": 48000, "warmup_callbacks": 128, "rust_heap_operations": 0,
                                 "deadline_ns": 2666666, "p50_ns": 100, "p99_ns": 100, "p999_ns": 100,
                                 "max_ns": 3000000, "deadline_misses": 1, "pages_at_boundary_max": 4,
                                 "intervals": 2, "nodes": 6, "transitions": 4, "render_bytes": 2048,
                                 "maintenance_bytes": 128, "uploaded_bytes": 64, "exported_bytes": 64,
                                 "pending_requests_after_drain": 0, "cascade_starts": 8,
                                 "configuration": {"transfer_bytes": 4096, "validation_steps": 256,
                                                   "resource_pages": 4096, "domain_credits": 63,
                                                   "command_slots": 64}})

    def verify(self):
        return runner.verify_records("\n".join(map(json.dumps, self.records)), self.raw, 2)

    def test_timeout_retains_partial_output_and_fails(self):
        log = self.raw / "timeout.log"
        error = runner.subprocess.TimeoutExpired(["benchmark"], 600, output=b"partial", stderr=b" diagnostic")
        with mock.patch.object(runner.subprocess, "run", side_effect=error):
            with self.assertRaisesRegex(RuntimeError, "timed out"):
                runner.run(["benchmark"], log, {})
        self.assertIn("partial diagnostic", log.read_text())

    def test_deadline_misses_are_retained_not_rejected(self):
        result = self.verify()
        self.assertEqual(len(result), 14)
        self.assertTrue(all(r["deadline_misses"] == 1 for r in result))
        self.assertTrue(all(r["slowest_callbacks"][0]["nanoseconds"] == 3000000 for r in result))

    def test_missing_and_duplicate_cases_are_refused(self):
        self.records[-1] = self.records[0]
        with self.assertRaisesRegex(RuntimeError, "missing or duplicate"):
            self.verify()

    def test_boolean_heap_counter_is_not_an_integer_witness(self):
        self.records[0]["rust_heap_operations"] = False
        with self.assertRaisesRegex(RuntimeError, "guard"):
            self.verify()

    def test_raw_timing_and_work_must_agree(self):
        for field in ("max_ns", "transitions", "deadline_misses", "pages_at_boundary_max"):
            with self.subTest(field=field):
                self.records[0][field] += 1
                with self.assertRaisesRegex(RuntimeError, "differs"):
                    self.verify()
                self.records[0][field] -= 1

    def test_truncated_raw_callbacks_are_refused(self):
        path = self.raw / f"{self.records[0]['case']}.csv"
        lines = path.read_text().splitlines()
        path.write_text("\n".join(lines[:-1])+"\n")
        with self.assertRaisesRegex(RuntimeError, "raw callbacks"):
            self.verify()

    def test_fabricated_upload_export_progress_is_refused(self):
        record = next(r for r in self.records if r["case"] == "upload-export")
        record["exported_bytes"] = 0
        with self.assertRaisesRegex(RuntimeError, "progress"):
            self.verify()

    def test_unmeasured_cascades_are_refused(self):
        record = next(r for r in self.records if r["case"] == "cascade-bursts")
        record["cascade_starts"] = 0
        path = self.raw / "cascade-bursts.csv"
        with path.open() as file:
            reader = csv.DictReader(file)
            rows = list(reader)
            fields = reader.fieldnames
        for row in rows:
            row["cascade_starts"] = 0
        with path.open("w") as file:
            writer = csv.DictWriter(file, fieldnames=fields)
            writer.writeheader()
            writer.writerows(rows)
        with self.assertRaisesRegex(RuntimeError, "cascade starts"):
            self.verify()

    def test_pending_ownership_credit_overflow_is_refused(self):
        path = self.raw / f"{self.records[0]['case']}.csv"
        path.write_text(path.read_text().replace(",8,8,0,1", ",8,8,64,1"))
        with self.assertRaisesRegex(RuntimeError, "contract"):
            self.verify()

    def test_unreturned_credits_are_refused(self):
        self.records[0]["pending_requests_after_drain"] = 1
        with self.assertRaisesRegex(RuntimeError, "guard"):
            self.verify()

    def test_raw_maintenance_overflow_is_refused(self):
        path = self.raw / f"{self.records[0]['case']}.csv"
        path.write_text(path.read_text().replace(",1024,64,1,1,4,", ",1024,64,1,257,4,"))
        with self.assertRaisesRegex(RuntimeError, "contract"):
            self.verify()


if __name__ == "__main__":
    unittest.main()
