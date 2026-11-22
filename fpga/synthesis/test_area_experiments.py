"""Check report interpretation and reproducible experiment preparation."""

import hashlib
import json
from pathlib import Path
import tempfile
import unittest

import area_results
import midi_shared_jobs


ROOT = Path(__file__).resolve().parents[2]
ARCHIVE = ROOT/'docs/fpga/measurements/midi-expression-state'
PROFILE = ARCHIVE/'compiled-profile.json'
BASE = 'f218087511dc28e72a436ec825445f32a722b5e9'


class AreaExperiments(unittest.TestCase):
    def test_archived_resources_distinguish_routed_slices(self):
        archive = ARCHIVE/'vivado/compact-session-100'
        route = area_results.resources(archive/'post_route_utilization.rpt')
        opt = area_results.resources(archive/'post_opt_utilization.rpt')
        self.assertEqual((route['lut'], route['slices'], route['bram36'], route['carry4']),
                         (41980, 13205, 77.5, 5491))
        self.assertIsNone(opt['slices'])

    def test_incomplete_resource_report_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            p = Path(directory)/'report.rpt'
            p.write_text('| Slice LUTs | 100 |\n')
            with self.assertRaisesRegex(ValueError, 'Missing'):
                area_results.resources(p)

    def test_benchmark_keeps_cold_and_steady_separate(self):
        data = area_results.benchmark(ARCHIVE/'mixed-on-delay7.json')
        self.assertEqual(data['cycles']['steady']['median'], 54146)
        self.assertEqual(data['cycles']['playback_start']['max'], 110356)

    def test_tampered_manifest_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            build = Path(directory)
            (build/'data').write_text('original')
            manifest = {'data': area_results.digest(build/'data')}
            (build/'manifest.json').write_text(json.dumps(manifest))
            area_results.verify_manifest(build, 'manifest.json', '')
            (build/'data').write_text('modified')
            with self.assertRaisesRegex(ValueError, 'Manifest mismatch'):
                area_results.verify_manifest(build, 'manifest.json', '')

    def test_benchmark_source_mismatch_is_rejected(self):
        data = json.loads((ARCHIVE/'mixed-on-delay7.json').read_text())
        data['provenance'] = {'models': [{'sources_sha256': {'fpga/test.sv': 'wrong'},
                                          'parameters': {}}]}
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)/'benchmark.json'
            path.write_text(json.dumps(data))
            with self.assertRaisesRegex(ValueError, 'Benchmark source mismatch'):
                area_results.benchmark(path, {'fpga/test.sv': 'expected'}, {})

    def test_prepared_baseline_matches_archived_live_context(self):
        with tempfile.TemporaryDirectory() as directory:
            build = Path(directory)
            midi_shared_jobs.prepare(build, BASE, 100, PROFILE)
            archive = ARCHIVE/'vivado/compact-session-100'
            self.assertEqual(json.loads((build/'source-manifest.json').read_text()),
                             json.loads((archive/'source-manifest.json').read_text()))
            expected = json.loads((archive/'artifact-manifest.json').read_text())
            for name in ('context.sv', 'context.xdc', 'compiled-profile.json'):
                self.assertEqual(hashlib.sha256((build/name).read_bytes()).hexdigest(), expected[name])

    def test_stage_clock_and_parameters_reach_artifacts(self):
        with tempfile.TemporaryDirectory() as directory:
            build = Path(directory)
            midi_shared_jobs.prepare(build, BASE, 75, PROFILE, 'opt', {'TABLE_LINES': 64})
            provenance = json.loads((build/'provenance.json').read_text())
            self.assertEqual(provenance['parameters']['TABLE_LINES'], 64)
            self.assertEqual(provenance['stage'], 'opt')
            self.assertIn('.TABLE_LINES(64)', (build/'context.sv').read_text())
            self.assertIn('13.333333', (build/'context.xdc').read_text())
            self.assertTrue((build/'run.tcl').read_text().startswith('set stop_after opt\n'))

    def test_unknown_parameter_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(ValueError, 'Unknown top parameters'):
                midi_shared_jobs.prepare(Path(directory), BASE, 100, PROFILE,
                                         overrides={'TYPO': 1})

    def test_comparison_rejects_clock_mismatch(self):
        a = {'provenance': {'clock_mhz': 100, 'part': 'xc7z020'}}
        b = {'provenance': {'clock_mhz': 150, 'part': 'xc7z020'}}
        with self.assertRaisesRegex(ValueError, 'clock_mhz'):
            area_results.compare(a, b)


if __name__ == '__main__':
    unittest.main()
