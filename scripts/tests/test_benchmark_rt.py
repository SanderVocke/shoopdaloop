import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import tempfile
import unittest
from unittest import mock

SPEC = importlib.util.spec_from_file_location("benchmark_rt", Path(__file__).resolve().parents[1] / "benchmark_rt.py")
BENCH = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BENCH)


def result():
    fixture = dict(version=3, fpga_chunk_frames=32, fpga_block_chunks=32, loops=64, channels=2, quantum=32, sample_rate=48_000,
                   fpga_clock_hz=150_000_000, callbacks=100, events=[dict(callback=0, count=64)], pattern="lifecycle")
    rows = [dict(callback=i, frame=3200 + i * 32, event="record-all" if i == 0 else None,
                 control_ns=10, recording=64, playing=0, wraps=0,
                 measurement=dict(elapsed_ns=999999999, fabric_cycles=150 * (i + 1), storage_bytes=128)) for i in range(100)]
    return dict(backend="fpga-rtl", status="completed", fixture=fixture,
                metadata=dict(chunk_frames=32, block_chunks=32,
                    provenance=dict(models=[dict(parameters=dict(CHUNK_FRAMES="32", BLOCK_CHUNKS="32"))]),
                    fabric_hz=150_000_000, memory_model="Dramsim3", memory=dict(native=dict(fabric_hz=150_000_000))),
                fixture_sha256=hashlib.sha256(json.dumps(fixture, separators=(",", ":")).encode()).hexdigest(),
                callbacks=rows, events=[dict(label="record-all", callback=0, commands=64,
                execution_frame=3200, groups=2, same_frame=True)], coverage=dict(complete_samples=True, cold_block_boundary_verified=True))


def midi_result(kind="midi"):
    run = result()
    run["fixture"].update(version=4, media=kind, channels=0 if kind == "midi" else 2,
        midi_block_events=256, midi_input_capacity=40, midi_output_capacity=2560, midi_scratch_events=65536)
    run["fixture_sha256"] = hashlib.sha256(json.dumps(run["fixture"], separators=(",", ":")).encode()).hexdigest()
    run["metadata"].update(shared_memory_controller=True, midi_memory=dict(native=dict(fabric_hz=150_000_000)))
    run["metadata"]["memory"]["native"].update(ticks=0, tck_fs=1_875_000)
    ticks = round(sum(row["measurement"]["fabric_cycles"] for row in run["callbacks"]) * 1e15 / 150_000_000 / 1_875_000)
    run["metadata"]["finalization"] = dict(native=dict(ticks=ticks, final_drain_dram_ticks=0))
    for i, row in enumerate(run["callbacks"]):
        count = 40 if i % 16 == 0 else (16 if i % 16 < 8 else 0)
        row.update(midi_input_events=count, midi_output_events=400)
        row["measurement"].update(midi_input_events=count, midi_output_events=400)
    run["coverage"].update(midi_bytes_timestamps_order_checked=True, midi_block_boundary_verified=True,
        midi_events_checked=40000, midi_silent_playback_callbacks=1, midi_dense_playback_callbacks=50)
    return run


class BenchmarkReportTests(unittest.TestCase):
    def test_midi_populations_keep_active_input_and_recording_out_of_playback_baselines(self):
        run = midi_result()
        for index in (8, 9, 10, 11):
            run["callbacks"][index].update(playing=64, recording=0)
        run["callbacks"][8].update(midi_input_events=0, midi_output_events=0)
        run["callbacks"][10].update(midi_input_events=16, midi_output_events=0)
        run["callbacks"][11].update(recording=32)
        populations = BENCH.summarize(run, "run.json")["midi_populations"]
        self.assertEqual(populations["quiet MIDI playback"]["count"], 1)
        self.assertEqual(populations["quiet MIDI playback"]["max_us"], 9)
        self.assertEqual(populations["dense MIDI playback (>256 out)"]["count"], 1)
        self.assertEqual(populations["dense MIDI playback (>256 out)"]["max_us"], 10)

    def test_fpga_cycles_are_authoritative_and_percentiles_use_observation_counts(self):
        run = result()
        BENCH.validate(run)
        s = BENCH.summarize(run, "run.json")
        self.assertEqual(s["statistics"]["median_us"], 50.5)
        self.assertEqual(s["statistics"]["p99_us"], 99)
        self.assertEqual(s["statistics"]["max_us"], 100)
        self.assertIsNone(s["groups"]["record-all"]["p99_us"])
        self.assertEqual(s["worst_callback"]["callback"], 99)

    def test_midi_comparisons_require_exact_events_and_shared_clocked_memory(self):
        for kind in ("midi", "audio-midi"):
            run = midi_result(kind)
            BENCH.validate(run)
            self.assertEqual(BENCH.summarize(run, "run.json")["media"], kind)
            for change in (lambda r: r["coverage"].update(midi_bytes_timestamps_order_checked=False),
                           lambda r: r["callbacks"][0].update(midi_input_events=1),
                           lambda r: r["metadata"].update(shared_memory_controller=False),
                           lambda r: r["metadata"]["midi_memory"]["native"].update(fabric_hz=100_000_000),
                           lambda r: r["coverage"].update(midi_events_checked=1),
                           lambda r: r["metadata"]["finalization"]["native"].update(ticks=99_000_000),
                           lambda r: r["fixture"].pop("media")):
                changed = copy.deepcopy(run)
                change(changed)
                changed["fixture_sha256"] = hashlib.sha256(json.dumps(changed["fixture"], separators=(",", ":")).encode()).hexdigest()
                with self.assertRaises(ValueError):
                    BENCH.validate(changed)

    def test_incomplete_or_reduced_runs_cannot_pass_validation(self):
        for change in (lambda r: r["callbacks"].pop(),
                       lambda r: r["fixture"].update(channels=1),
                       lambda r: r["events"][0].update(execution_frame=3232),
                       lambda r: r["events"][0].update(same_frame=False),
                       lambda r: r["callbacks"][4].update(frame=3200),
                       lambda r: r["callbacks"][0]["measurement"].update(fabric_cycles=None)):
            run = copy.deepcopy(result()); change(run)
            run["fixture_sha256"] = hashlib.sha256(json.dumps(run["fixture"], separators=(",", ":")).encode()).hexdigest()
            with self.assertRaises(ValueError):
                BENCH.validate(run)

    def test_memory_model_and_report_cannot_disagree_about_fabric_clock(self):
        run = result()
        run["metadata"]["memory"]["native"]["fabric_hz"] = 100_000_000
        with self.assertRaisesRegex(ValueError, "DRAMsim3 fabric clock"):
            BENCH.validate(run)

    def test_geometry_and_cold_boundary_coverage_cannot_be_silently_changed(self):
        for change in (lambda r: r["metadata"].update(chunk_frames=1, block_chunks=128),
                       lambda r: r["metadata"]["provenance"]["models"][0]["parameters"].update(CHUNK_FRAMES="1"),
                       lambda r: r["coverage"].update(cold_block_boundary_verified=False)):
            run = result(); change(run)
            with self.assertRaises(ValueError):
                BENCH.validate(run)

    def test_report_regenerates_pdf_and_exports_without_a_backend(self):
        with tempfile.TemporaryDirectory() as name:
            output = Path(name)
            (output / "run.json").write_text(json.dumps(result()))
            (output / "manifest.json").write_text(json.dumps(dict(files=["run.json"], revision="test")))
            self.assertTrue(BENCH.report(output))
            self.assertTrue((output / "report.pdf").read_bytes().startswith(b"%PDF"))
            self.assertEqual(len(re.findall(rb"/Type\s*/Page\b", (output / "report.pdf").read_bytes())), 4)
            self.assertEqual(len((output / "callbacks.csv").read_text().splitlines()), 101)
            self.assertEqual(json.loads((output / "summary.json").read_text())["runs"][0]["statistics"]["max_us"], 100)
            self.assertEqual(json.loads((output / "summary.json").read_text())["fpga_media_geometries"],
                [dict(chunk_frames=32, block_chunks=32, block_frames=1024)])

    def test_report_stays_within_five_pages_as_repeats_grow_without_losing_exports(self):
        with tempfile.TemporaryDirectory() as name:
            output = Path(name)
            files = []
            for backend in BENCH.BACKENDS:
                for kind in BENCH.MEDIA:
                    for pattern in BENCH.PATTERNS:
                        for repeat in range(4):
                            run = result() if kind == "audio" else midi_result(kind)
                            run.update(backend=backend, repeat=repeat)
                            run["fixture"]["media"] = kind
                            run["fixture"]["pattern"] = pattern
                            run["fixture_sha256"] = hashlib.sha256(json.dumps(run["fixture"], separators=(",", ":")).encode()).hexdigest()
                            if backend != "fpga-rtl":
                                for row in run["callbacks"]:
                                    row["measurement"].update(fabric_cycles=None, elapsed_ns=1000 * (row["callback"] + 1))
                            if (backend.startswith("fpga-") and pattern == "grab") or (backend == "cpu" and kind != "audio"):
                                run.update(status="unsupported", error="grabs unsupported", callbacks=[], events=[])
                            filename = f"{backend}-{kind}-{pattern}-{repeat}.json"
                            files.append(filename)
                            (output / filename).write_text(json.dumps(run))
            (output / "manifest.json").write_text(json.dumps(dict(files=files, expected_runs=len(files), complete=True)))
            self.assertTrue(BENCH.report(output))
            pdf = (output / "report.pdf").read_bytes()
            # Matplotlib writes uncompressed page dictionaries; this excludes /Pages.
            self.assertEqual(len(re.findall(rb"/Type\s*/Page\b", pdf)), 5)
            summary = json.loads((output / "summary.json").read_text())
            self.assertEqual(len(summary["runs"]), 108)
            self.assertEqual(sum(run["status"] == "unsupported" for run in summary["runs"]), 48)
            self.assertEqual(len((output / "callbacks.csv").read_text().splitlines()), 6001)
            for run in summary["runs"]:
                if run["status"] == "completed":
                    self.assertEqual(run["statistics"]["max_us"], 100)
                    self.assertEqual(run["groups"]["record-all"]["count"], 1)

    def test_different_backend_fixtures_fail_comparison_without_discarding_statistics(self):
        with tempfile.TemporaryDirectory() as name:
            output = Path(name)
            first = result()
            second = result()
            second["backend"] = "fpga-reference"
            second["fixture"]["signal"] = "different-input"
            second["fixture_sha256"] = hashlib.sha256(json.dumps(second["fixture"], separators=(",", ":")).encode()).hexdigest()
            (output / "a.json").write_text(json.dumps(first))
            (output / "b.json").write_text(json.dumps(second))
            (output / "manifest.json").write_text(json.dumps(dict(files=["a.json", "b.json"])))
            self.assertFalse(BENCH.report(output))
            summary = json.loads((output / "summary.json").read_text())
            self.assertFalse(summary["fixture_comparisons_match"])
            self.assertEqual(len(summary["fixture_mismatches"]["audio/lifecycle"]), 2)
            self.assertEqual([run["statistics"]["count"] for run in summary["runs"]], [100, 100])

    def test_parallel_rtl_starts_after_all_host_timing_repeats(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            binary = root / "benchmark"
            binary.write_bytes(b"fake executable")
            output = root / "campaign"
            calls = []
            def execute(command, **kwargs):
                backend = command[command.index("--backend") + 1]
                calls.append(backend)
                path = Path(command[command.index("--output") + 1])
                path.write_text(json.dumps(dict(backend=backend, status="completed", fixture=dict(
                    version=4, media="audio", preset="standard", pattern="lifecycle",
                    fpga_chunk_frames=32, fpga_block_chunks=32))))
                return mock.Mock(returncode=0)
            arguments = ["benchmark_rt.py", "--binary", str(binary), "--backends", "fpga-rtl,cpu,fpga-reference",
                "--media", "audio", "--patterns", "lifecycle", "--repeats", "2", "--rtl-jobs", "2",
                "--memory-model", "fixed", "--output", str(output)]
            with mock.patch("sys.argv", arguments), mock.patch.object(BENCH, "capture", return_value="Verilator 5.032"), \
                    mock.patch.object(BENCH.subprocess, "run", side_effect=execute), mock.patch.object(BENCH, "report", return_value=True):
                self.assertEqual(BENCH.main(), 0)
            self.assertEqual(len(calls), 6)
            self.assertEqual(calls[-2:], ["fpga-rtl", "fpga-rtl"])
            self.assertNotIn("fpga-rtl", calls[:-2])
            manifest = json.loads((output / "manifest.json").read_text())
            self.assertTrue(manifest["complete"])
            self.assertEqual(len(manifest["files"]), 6)
            runtime = json.loads((output / "runtime.json").read_text())
            self.assertTrue(runtime["complete"])
            self.assertTrue(runtime["passed"])
            self.assertEqual(len(runtime["runs"]), 6)
            self.assertAlmostEqual(sum(runtime["phases_s"].values()), runtime["elapsed_s"])
            host_end = max(r["finished_s"] for r in runtime["runs"] if r["backend"] != "fpga-rtl")
            rtl_start = min(r["started_s"] for r in runtime["runs"] if r["backend"] == "fpga-rtl")
            self.assertLessEqual(host_end, rtl_start)

    def test_interrupted_campaign_cannot_pass_with_only_completed_subsets(self):
        with tempfile.TemporaryDirectory() as name:
            output = Path(name)
            (output / "run.json").write_text(json.dumps(result()))
            (output / "manifest.json").write_text(json.dumps(dict(files=["run.json"], complete=False, expected_runs=2)))
            self.assertFalse(BENCH.report(output))
            self.assertFalse(json.loads((output / "summary.json").read_text())["complete"])

    def test_new_campaign_rejects_an_old_binary_but_retains_its_original_data(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name); binary = root / "old-benchmark"; binary.write_bytes(b"old binary")
            output = root / "campaign"
            original_run = BENCH.subprocess.run
            def execute(command, **kwargs):
                if command[0] != str(binary):
                    return original_run(command, **kwargs)
                run = result(); run["backend"] = "fpga-reference"
                run["fixture"]["version"] = 2
                del run["fixture"]["fpga_chunk_frames"], run["fixture"]["fpga_block_chunks"]
                run["fixture_sha256"] = hashlib.sha256(json.dumps(run["fixture"], separators=(",", ":")).encode()).hexdigest()
                run["metadata"] = dict(chunk_frames=1, block_chunks=128)
                run["coverage"] = dict(complete_samples=True)
                BENCH.validate(run)
                Path(command[command.index("--output") + 1]).write_text(json.dumps(run))
                return mock.Mock(returncode=0)
            arguments = ["benchmark_rt.py", "--binary", str(binary), "--backends", "fpga-reference",
                         "--patterns", "lifecycle", "--media", "audio", "--repeats", "1", "--output", str(output)]
            with mock.patch("sys.argv", arguments), mock.patch.object(BENCH.subprocess, "run", side_effect=execute):
                self.assertEqual(BENCH.main(), 1)
            original = output / "fpga-reference-audio-lifecycle-0.incompatible.json"
            self.assertEqual(json.loads(original.read_text())["fixture"]["version"], 2)
            summary = json.loads((output / "summary.json").read_text())
            self.assertEqual(summary["runs"][0]["status"], "incorrect-results")
            self.assertEqual(summary["runs"][0]["statistics"]["count"], 0)


if __name__ == "__main__":
    unittest.main()
