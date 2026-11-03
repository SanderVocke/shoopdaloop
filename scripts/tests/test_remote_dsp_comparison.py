"""Audit regression checks use frozen records, not new timing measurements."""
import copy
import gzip
import importlib.util
import json
from pathlib import Path
import sys
import unittest
from unittest import mock
import xml.etree.ElementTree as ET

if sys.platform == "win32":
    raise unittest.SkipTest("Linux scheduling tools require the POSIX resource module")

SCRIPTS = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("remote_dsp_comparison", SCRIPTS / "compare_remote_dsp.py")
comparison = importlib.util.module_from_spec(SPEC)
with mock.patch.object(sys, "path", [str(SCRIPTS), *sys.path]):
    SPEC.loader.exec_module(comparison)


class RemoteDspComparisonTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.archive = SCRIPTS.parent / "docs/remote-dsp/tuning-evidence/results.jsonl.gz"
        cls.rows = comparison.read_archive(cls.archive)

    def test_frozen_cloud_campaign_passes_without_timing_thresholds(self):
        audit = comparison.audit(self.rows, self.archive)
        self.assertEqual(audit["records"], 184)
        self.assertEqual(audit["late"], 6813)
        self.assertEqual(audit["correctness_errors"], 0)
        self.assertEqual(audit["verified_samples"], 124441600)

    def test_published_local_campaign_has_complete_audited_coverage(self):
        root = SCRIPTS.parent / "docs/remote-dsp/local-evidence"
        published = json.loads((root / "audit.json").read_text())
        groups = {name: comparison.read_archive(root / name / "results.jsonl.gz")
                  for name in comparison.CAMPAIGNS}
        groups["root"] = root
        self.assertEqual(comparison.check_coverage(groups), published["coverage"])
        for name in comparison.CAMPAIGNS:
            self.assertEqual(comparison.audit(groups[name], root / name / "results.jsonl.gz"),
                             published["campaigns"][name])

    def test_heatmap_is_valid_svg_and_distinguishes_zero_misses(self):
        row = {"suite": "baseline", "status": "measured", "requested": {"P": 8, "k": 1, "N": 1},
               "late": 0, "missing": 0, "measured_packets": 10}
        svg = comparison.heatmap([row], [row])
        image = ET.fromstring(svg)
        self.assertEqual(len(image.findall("{http://www.w3.org/2000/svg}rect")), 201)
        self.assertEqual(svg.count('fill="#d9f0d3"'), 2)
        self.assertIn("ref = N &lt; k", svg)

    def test_audio_latency_includes_packetization_without_double_counting_aggregation(self):
        times = comparison.timing_ms(16, 4, 4)
        self.assertAlmostEqual(times["packetization"], 1 / 3)
        self.assertAlmostEqual(times["aggregation"], 1)
        self.assertAlmostEqual(times["allowance"], 4 / 3)
        self.assertAlmostEqual(times["audio_latency"], 5 / 3)
        self.assertEqual(times["audio_latency"], comparison.timing_ms(16, 1, 4)["audio_latency"])

    def test_audio_latency_adds_algorithm_delay_and_uses_sample_rate(self):
        times = comparison.timing_ms(128, 8, 64, delay=1027)
        self.assertAlmostEqual(times["audio_latency"], 9347 / 48)
        self.assertAlmostEqual(times["audio_latency"] - comparison.timing_ms(128, 8, 64)["audio_latency"], 1027 / 48)
        self.assertAlmostEqual(comparison.timing_ms(128, 8, 64, delay=1027, sample_rate=96000)["audio_latency"],
                               times["audio_latency"] / 2)

    def test_duplicate_case_is_rejected(self):
        with self.assertRaises(AssertionError):
            comparison.audit([self.rows[0], self.rows[0]], self.archive)

    def test_inconsistent_packet_or_sample_counts_are_rejected(self):
        for field in ["measured_packets", "verified_samples", "correctness_errors", "client_exit_code"]:
            with self.subTest(field=field):
                row = copy.deepcopy(self.rows[0])
                row[field] = row.get(field, 0) + 1
                with self.assertRaises(AssertionError):
                    comparison.audit([row], self.archive)

    def test_histogram_counts_and_quantile_bins_are_checked(self):
        for field in ["count", "p99_ns", "max_ns", "mean_ns"]:
            with self.subTest(field=field):
                distribution = copy.deepcopy(self.rows[0]["rtt"])
                distribution[field] += 1000000000
                with self.assertRaises(AssertionError):
                    comparison.check_distribution(distribution)

    def test_empty_distribution_has_no_fabricated_quantiles(self):
        distribution = {"count": 0, "histogram_us": [], "mean_ns": None, "max_ns": None,
                        "p50_ns": None, "p95_ns": None, "p99_ns": None, "p999_ns": None}
        comparison.check_distribution(distribution)
        distribution["p99_ns"] = 0
        with self.assertRaises(AssertionError):
            comparison.check_distribution(distribution)

    def test_reference_sample_count_includes_explicitly_unverified_samples(self):
        row = copy.deepcopy(self.rows[0])
        row["verified_samples"] -= 1
        row["unverified_samples"] += 1
        audit = comparison.audit([row], self.archive)
        self.assertEqual(audit["unverified_samples"], 1)

    def test_archive_hash_is_of_uncompressed_payload(self):
        audit = comparison.audit(self.rows, self.archive)
        expected = comparison.hashlib.sha256(gzip.decompress(self.archive.read_bytes())).hexdigest()
        self.assertEqual(audit["decompressed_sha256"], expected)


if __name__ == "__main__":
    unittest.main()
