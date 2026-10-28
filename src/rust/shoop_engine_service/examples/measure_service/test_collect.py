import csv
import gzip
import io
import json
from pathlib import Path
import tempfile
import unittest

import collect


def csv_rows(engine="service", mode="idle", samples=16, warmup=1):
    result = []
    for index in range(samples):
        frame = (warmup + index) * 64
        row = {name: 0 for name in collect.COLUMNS}
        row.update(engine=engine, workload=mode, index=index, frame=frame,
                   callback_ns=100 + index, control_ns=20,
                   meter_ns=40 * (64 if mode == "poll" else 1),
                   meter_reads=64 if mode == "poll" else 1,
                   control_attempts=2 if mode == "preset" else int(mode in ("poll", "export")),
                   refusals=int(mode == "preset"))
        if mode == "export":
            row["control_attempts"] = int(frame >= 128)
            row["copied_bytes"] = 256 if index >= 3 else 0
        if engine == "service" and mode in ("steady", "preset", "poll") and frame % 4096 == 0:
            row["midi_packets"] = 64
        result.append(row)
    return result


def encode(rows):
    output = io.StringIO()
    writer = csv.DictWriter(output, fieldnames=collect.COLUMNS)
    writer.writeheader()
    writer.writerows(rows)
    return output.getvalue()


def metadata(engine, mode, samples=16, warmup=1):
    return (f"meta engine={engine} mode={mode} frames=64 loop_frames=4096 sample_rate=48000 "
            f"samples={samples} warmup={warmup} debug=false clock_median_ns=20 rss_constructed_kib=Some(100)\n"
            "meta rss_after_warmup_kib=Some(104)\n"
            "meta elapsed_wall_ns=10000 rss_live_kib=Some(108)\n"
            "meta rss_after_cleanup_kib=Some(102)\n")


def fixture_bundle(directory):
    summary = {}
    manifest = {"schema": 1, "complete": True,
                "source": {"status_before": "", "head": "a" * 40, "head_after": "a" * 40},
                "configuration": {"samples": 16, "warmup": 1, "repeats": 1,
                                  "frames": 64, "nominal_rate": 48000, "loop_frames": 4096},
                "runs": []}
    for engine, mode in collect.CASES:
        raw = encode(csv_rows(engine, mode)).encode()
        meta = metadata(engine, mode).encode()
        stem = f"{engine}-{mode}-r1"
        raw_bytes = gzip.compress(raw, mtime=0)
        (directory / f"{stem}.csv.gz").write_bytes(raw_bytes)
        (directory / f"{stem}.meta.txt").write_bytes(meta)
        rows = collect.parse_rows(raw.decode(), engine, mode, 16, 1)
        observations = collect.parse_metadata(meta.decode(), engine, mode, 16, 1)
        values = collect.summarize(rows, [observations])
        summary[f"{engine}/{mode}"] = values
        manifest["runs"].append({"engine": engine, "workload": mode, "repetition": 1, "exit": 0,
                                 "command": ["binary", engine, mode, "16", "1"],
                                 "raw": f"{stem}.csv.gz", "raw_sha256": collect.digest(raw_bytes),
                                 "metadata": f"{stem}.meta.txt", "metadata_sha256": collect.digest(meta),
                                 "observations": observations, "summary": values})
    (directory / "summary.json").write_text(json.dumps(summary))
    manifest["summary_sha256"] = collect.digest((directory / "summary.json").read_bytes())
    (directory / "manifest.json").write_text(json.dumps(manifest))
    return manifest


class DistributionTests(unittest.TestCase):
    def test_nearest_rank_retains_the_tail_and_even_median(self):
        self.assertEqual(collect.distribution(list(range(1, 101))),
                         {"n": 100, "median": 50.5, "p95": 95, "p99": 99, "max": 100})
        self.assertEqual(collect.distribution([1, 2, 1_000_000])["max"], 1_000_000)
        self.assertEqual(collect.distribution([1, 2])["p95"], 2)

    def test_rejects_missing_negative_or_nonfinite_values(self):
        for values in ([], [-1], [float("nan")], [float("inf")]):
            with self.subTest(values=values), self.assertRaises(ValueError):
                collect.distribution(values)


class ParserTests(unittest.TestCase):
    def test_all_declared_cases_and_a_real_wrap_cut(self):
        for engine, mode in collect.CASES:
            with self.subTest(engine=engine, mode=mode):
                collect.parse_rows(encode(csv_rows(engine, mode)), engine, mode, 16, 1)
                collect.parse_metadata(metadata(engine, mode), engine, mode, 16, 1)
        rows = collect.parse_rows(encode(csv_rows(mode="steady", warmup=64)), "service", "steady", 16, 64)
        self.assertEqual(rows[0]["midi_packets"], 64)

    def test_wrong_frame_sample_count_timing_work_or_oracle_is_rejected(self):
        for key, value in (("frame", 63), ("index", 1), ("callback_ns", -1),
                           ("control_attempts", 1), ("refusals", 1), ("meter_reads", 64),
                           ("midi_packets", 1), ("copied_bytes", 256)):
            rows = csv_rows()
            rows[0][key] = value
            with self.subTest(key=key), self.assertRaises(ValueError):
                collect.parse_rows(encode(rows), "service", "idle", 16, 1)
        with self.assertRaises(ValueError):
            collect.parse_rows(encode(csv_rows()[:-1]), "service", "idle", 16, 1)

    def test_debug_wrong_configuration_duplicate_and_missing_metadata_rejected(self):
        original = metadata("service", "idle")
        for changed in (original.replace("debug=false", "debug=true"),
                        original.replace("frames=64", "frames=32"),
                        original + "meta elapsed_wall_ns=1\n",
                        original.replace("elapsed_wall_ns=10000", "nothing=1")):
            with self.assertRaises(ValueError):
                collect.parse_metadata(changed, "service", "idle", 16, 1)

    def test_rates_and_batch_mean_are_computed_from_real_work(self):
        rows = collect.parse_rows(encode(csv_rows(mode="export")), "service", "export", 16, 1)
        values = collect.summarize(rows, [collect.parse_metadata(metadata("service", "export"), "service", "export", 16, 1)])
        self.assertEqual(values["copied_bytes"], 13 * 256)
        self.assertEqual(values["unpaced_host_copy_bytes_per_second"], 13 * 256 * 1e9 / 10000)
        self.assertEqual(values["meter_batch_mean_per_read_ns"]["median"], 40)


class BundleTests(unittest.TestCase):
    def test_recompute_complete_bundle_and_reject_tampered_raw(self):
        with tempfile.TemporaryDirectory() as name:
            directory = Path(name)
            fixture_bundle(directory)
            self.assertEqual(len(collect.audit_bundle(directory)), 7)
            path = directory / "service-idle-r1.csv.gz"
            path.write_bytes(path.read_bytes() + b"changed")
            with self.assertRaisesRegex(ValueError, "raw hash mismatch"):
                collect.audit_bundle(directory)

    def test_forged_summary_is_recomputed_even_when_its_hash_matches(self):
        with tempfile.TemporaryDirectory() as name:
            directory = Path(name)
            manifest = fixture_bundle(directory)
            summary_path = directory / "summary.json"
            summary = json.loads(summary_path.read_text())
            summary["service/idle"]["timing_ns"]["callback_ns"]["max"] = 1
            summary_path.write_text(json.dumps(summary))
            manifest["summary_sha256"] = collect.digest(summary_path.read_bytes())
            (directory / "manifest.json").write_text(json.dumps(manifest))
            with self.assertRaisesRegex(ValueError, "pooled summary mismatch"):
                collect.audit_bundle(directory)

    def test_incomplete_duplicate_and_dirty_source_cannot_be_final_evidence(self):
        with tempfile.TemporaryDirectory() as name:
            directory = Path(name)
            for change in (lambda m: m.update(complete=False),
                           lambda m: m["runs"].__setitem__(1, m["runs"][0]),
                           lambda m: m["source"].update(status_before="M source")):
                manifest = fixture_bundle(directory)
                change(manifest)
                (directory / "manifest.json").write_text(json.dumps(manifest))
                with self.assertRaises(ValueError):
                    collect.audit_bundle(directory)


if __name__ == "__main__":
    unittest.main()
