"""CI partition/collection checks use synthetic reports, not interpreter evidence."""
import argparse
import contextlib
import copy
import io
import json
from pathlib import Path
import tempfile
import sys
import unittest
from unittest.mock import patch

from scripts import run_rt_runtime_miri as runner


class MiriPartitionTests(unittest.TestCase):
    def test_sixteen_batches_cover_the_complete_inventory_once_without_changing_cases(self):
        full = runner.selected_cases()
        batches = [runner.inventory(shard=(index, 16)) for index in range(1, 17)]
        self.assertTrue(all(batch["cases"] for batch in batches))
        self.assertEqual(sum(len(batch["cases"]) for batch in batches), len(full))
        self.assertEqual(sum(case[3] for batch in batches for case in batch["cases"]), 304)
        keyed = lambda case: json.dumps(case)
        self.assertEqual(sorted(map(keyed, full)), sorted(keyed(case) for batch in batches for case in batch["cases"]))
        self.assertEqual(len({batch["full_inventory_sha256"] for batch in batches}), 1)
        for index, batch in enumerate(batches):
            self.assertEqual(batch["cases"], full[index::16])
        self.assertEqual(runner.inventory()["cases"], full)
        self.assertIsNone(runner.inventory()["shard"])

    def test_invalid_or_empty_partitions_are_refused(self):
        for value in ["0/2", "3/2", "1/0", "-1/2", "1", "one/two", "1/2/3"]:
            with self.subTest(value=value), self.assertRaises(argparse.ArgumentTypeError):
                runner.shard_spec(value)
        self.assertEqual(runner.shard_spec("2/16"), (2, 16))
        with self.assertRaisesRegex(ValueError, "empty batches"):
            runner.sharded_cases([("fixture",)], (1, 2))

    def test_list_cli_reports_a_batch_without_running_miri(self):
        output = io.StringIO()
        with patch.object(sys, "argv", ["runner", "--list", "--shard", "1/16"]), patch.object(
            runner, "run_case", side_effect=AssertionError("unexpected interpreter")
        ), contextlib.redirect_stdout(output):
            runner.main()
        report = json.loads(output.getvalue())
        self.assertEqual(report["shard"], {"index": 1, "total": 16})
        self.assertEqual(len(report["cases"]), 10)
        self.assertEqual(report["full_inventory_sha256"], runner.inventory()["full_inventory_sha256"])

    def test_package_partitions_cannot_claim_the_default_inventory(self):
        self.assertNotEqual(runner.inventory(runner.HOST_PACKAGE)["full_inventory_sha256"],
                            runner.inventory()["full_inventory_sha256"])


class MiriBatchCollectionTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.cases = [
            ("rt_runtime", "first", [], 2, 0),
            ("shoop_loops", "second", ["only", "--", "--exact"], 1, 1),
            (runner.HOST_PACKAGE, "lib", ["host", "--", "--exact"], 1, 2),
            ("rt_runtime", "last", [], 1, 0),
        ]
        self.selection = patch.object(runner, "selected_cases", return_value=self.cases)
        self.source = patch.object(runner, "source_commit", return_value="fixture-source")
        self.selection.start()
        self.source.start()
        self.paths = []
        for index in (1, 2):
            selected = runner.inventory(shard=(index, 2))
            directory = self.root / f"batch-{index}"
            directory.mkdir()
            records = []
            for offset, (package, target, arguments, minimum, seed) in enumerate(selected["cases"]):
                name = f"case-{offset}.log"
                (directory / name).write_text(f"test result: ok. {minimum} passed; 0 failed\n")
                records.append({"package": package, "target": target, "arguments": arguments,
                                "passed": minimum, "seed": seed, "log": name, "seconds": 0.1,
                                "miri_flags": runner.case_flags(seed, directory)})
            report = {**selected, "cases": records, "passed": True, "package_filter": None,
                      "source_commit": "fixture-source", "miri": "synthetic fixture, not Miri", "run_attempt": "1"}
            path = directory / "result.json"
            path.write_text(json.dumps(report))
            self.paths.append(path)

    def tearDown(self):
        self.source.stop()
        self.selection.stop()
        self.temporary.cleanup()

    def test_complete_set_checks_all_logs_counts_seeds_and_artifact_hashes(self):
        result = runner.verify_batches(self.root)
        self.assertTrue(result["passed"])
        self.assertEqual(result["commands"], 4)
        self.assertEqual(result["executions"], 5)
        self.assertEqual(len(result["artifact_sha256"]), 6)
        self.assertEqual(result["source_commit"], "fixture-source")
        self.assertEqual(set(result["batches"]), {1, 2})

    def test_collection_cli_writes_only_verified_complete_evidence(self):
        with patch.object(sys, "argv", ["runner", "--verify-batches", str(self.root)]), contextlib.redirect_stdout(io.StringIO()):
            runner.main()
        result = json.loads((self.root / "verified.json").read_text())
        self.assertEqual(result["executions"], 5)
        self.assertEqual(result["source_commit"], "fixture-source")
        with patch.object(sys, "argv", ["runner", "--verify-batches", str(self.root), "--package", "rt_runtime"]), contextlib.redirect_stderr(
            io.StringIO()
        ), self.assertRaises(SystemExit) as failure:
            runner.main()
        self.assertEqual(failure.exception.code, 2)

    def test_missing_duplicate_and_mixed_batch_sets_fail(self):
        original = self.paths[1].read_text()
        self.paths[1].unlink()
        with self.assertRaisesRegex(RuntimeError, "Incomplete Miri batch set"):
            runner.verify_batches(self.root)
        self.paths[1].write_text(self.paths[0].read_text())
        with self.assertRaisesRegex(RuntimeError, "Duplicate"):
            runner.verify_batches(self.root)
        report = json.loads(original)
        report["shard"]["total"] = 3
        self.paths[1].write_text(json.dumps(report))
        with self.assertRaisesRegex(RuntimeError, "Inconsistent"):
            runner.verify_batches(self.root)

    def test_source_scope_version_identity_and_manifest_substitution_fail(self):
        path = self.paths[0]
        original = json.loads(path.read_text())
        changes = [
            {"passed": False}, {"source_commit": "older-source"}, {"toolchain": "another-nightly"},
            {"package_filter": runner.HOST_PACKAGE}, {"full_inventory_sha256": "wrong"},
            {"manifests": {}}, {"miri": ""}, {"shard": {"index": True, "total": 2}}, {"measureme": True},
        ]
        for change in changes:
            with self.subTest(change=change):
                path.write_text(json.dumps({**original, **change}))
                with self.assertRaises(RuntimeError):
                    runner.verify_batches(self.root)
        path.write_text(json.dumps(original))
        report = json.loads(self.paths[1].read_text())
        report["miri"] = "different Miri version"
        self.paths[1].write_text(json.dumps(report))
        with self.assertRaisesRegex(RuntimeError, "inconsistent Miri version"):
            runner.verify_batches(self.root)

    def test_wrong_cases_counts_seeds_and_missing_or_escaping_logs_fail(self):
        path = self.paths[0]
        original = json.loads(path.read_text())
        for field, value in [("target", "wrong"), ("arguments", []), ("passed", True),
                             ("passed", 3), ("seed", True), ("seed", 99), ("log", "../escape.log"),
                             ("miri_flags", "-Zmiri-seed=0 -Zmiri-disable-stacked-borrows")]:
            report = copy.deepcopy(original)
            record = report["cases"][1] if field == "arguments" else report["cases"][0]
            record[field] = value
            with self.subTest(field=field, value=value):
                path.write_text(json.dumps(report))
                with self.assertRaises(RuntimeError):
                    runner.verify_batches(self.root)
        report = copy.deepcopy(original)
        report["cases"].pop()
        path.write_text(json.dumps(report))
        with self.assertRaisesRegex(RuntimeError, "Incomplete Miri batch inventory"):
            runner.verify_batches(self.root)
        path.write_text(json.dumps(original))
        log = path.parent / original["cases"][0]["log"]
        log.write_text("test result: ok. 0 passed; 0 failed\n")
        with self.assertRaisesRegex(RuntimeError, "log summaries"):
            runner.verify_batches(self.root)
        log.unlink()
        with self.assertRaises(FileNotFoundError):
            runner.verify_batches(self.root)


if __name__ == "__main__":
    unittest.main()
