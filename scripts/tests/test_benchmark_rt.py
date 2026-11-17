import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("benchmark_rt", Path(__file__).resolve().parents[1] / "benchmark_rt.py")
BENCH = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BENCH)


def result():
    fixture = dict(loops=64, channels=2, quantum=32, sample_rate=48_000,
                   fpga_clock_hz=150_000_000, callbacks=100, events=[dict(callback=0, count=64)], pattern="lifecycle")
    rows = [dict(callback=i, frame=3200 + i * 32, event="record-all" if i == 0 else None,
                 control_ns=10, recording=64, playing=0, wraps=0,
                 measurement=dict(elapsed_ns=999999999, fabric_cycles=150 * (i + 1), storage_bytes=128)) for i in range(100)]
    return dict(backend="fpga-rtl", status="completed", fixture=fixture,
                fixture_sha256=hashlib.sha256(json.dumps(fixture, separators=(",", ":")).encode()).hexdigest(),
                callbacks=rows, events=[dict(label="record-all", callback=0, commands=64,
                execution_frame=3200, groups=2, same_frame=True)], coverage=dict(complete_samples=True))


class BenchmarkReportTests(unittest.TestCase):
    def test_fpga_cycles_are_authoritative_and_percentiles_use_observation_counts(self):
        run = result()
        BENCH.validate(run)
        s = BENCH.summarize(run, "run.json")
        self.assertEqual(s["statistics"]["median_us"], 50.5)
        self.assertEqual(s["statistics"]["p99_us"], 99)
        self.assertEqual(s["statistics"]["max_us"], 100)
        self.assertIsNone(s["groups"]["record-all"]["p99_us"])
        self.assertEqual(s["worst_callback"]["callback"], 99)

    def test_incomplete_or_reduced_runs_cannot_pass_validation(self):
        for change in (lambda r: r["callbacks"].pop(),
                       lambda r: r["fixture"].update(channels=1),
                       lambda r: r["events"][0].update(execution_frame=3232),
                       lambda r: r["events"][0].update(same_frame=False),
                       lambda r: r["callbacks"][4].update(frame=3200),
                       lambda r: r["callbacks"][0]["measurement"].update(fabric_cycles=None)):
            run = copy.deepcopy(result()); change(run)
            with self.assertRaises(ValueError):
                BENCH.validate(run)

    def test_report_regenerates_pdf_and_exports_without_a_backend(self):
        with tempfile.TemporaryDirectory() as name:
            output = Path(name)
            (output / "run.json").write_text(json.dumps(result()))
            (output / "manifest.json").write_text(json.dumps(dict(files=["run.json"], revision="test")))
            self.assertTrue(BENCH.report(output))
            self.assertTrue((output / "report.pdf").read_bytes().startswith(b"%PDF"))
            self.assertEqual(len((output / "callbacks.csv").read_text().splitlines()), 101)
            self.assertEqual(json.loads((output / "summary.json").read_text())["runs"][0]["statistics"]["max_us"], 100)


if __name__ == "__main__":
    unittest.main()
