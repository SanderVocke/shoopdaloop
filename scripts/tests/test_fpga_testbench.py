"""Cross-language contract checks against the actual Rust state processor."""

from copy import deepcopy
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import fpga_testbench as wire
from jsonschema import ValidationError

ROOT = Path(__file__).resolve().parents[2]
U32_MAX = 2**32 - 1
U64_MAX = 2**64 - 1
I32_MIN = -(2**31)
I32_MAX = 2**31 - 1


def document(kind, value):
    return {"format_version": 3, "payload": {"kind": kind, "value": value}}


def word(value):
    return {"kind": "word", "value": value}


def ramp():
    return {
        "start": I32_MIN, "target": I32_MAX,
        "duration": U32_MAX, "elapsed": U32_MAX - 16,
    }


def graph():
    return document("graph", {
        "quantum": 32, "chunk_frames": 1,
        "nodes": [
            {"kind": "input", "port": 0},
            {"kind": "gain", "input": 0, "gain": 8388608},
            {"kind": "mix", "left": 0, "right": 1},
            {"kind": "capture", "input": 2, "view": 17, "recording": True},
            {"kind": "playback", "view": 17, "length_chunks": 45, "position_chunks": 0, "playing": False},
            {"kind": "output", "input": 4, "port": 0},
        ],
    })


class CrossLanguageContract(unittest.TestCase):
    def test_resolver_intents_roundtrip_chunk_units_and_actual_windows(self):
        intents = [{"view": U32_MAX, "position_chunks": U64_MAX,
                    "length_chunks": U64_MAX, "chunks": U64_MAX}]
        source = document("read_intents", intents)
        self.assertEqual(self.reference("roundtrip", source), source)
        case = document("resolver_case", {
            "name": "chunk-window-wrap-and-unpublished", "media": {
                "chunk_frames": 4, "samples": list(range(12)), "free_chunks": [],
                "views": [{"chunks": [0, 1, 2], "frames": 12}, {"chunks": [], "frames": 0}]},
            "requests": [
                {"view": 0, "position_chunks": 2, "length_chunks": 3, "chunks": 2},
                {"view": 1, "position_chunks": 0, "length_chunks": 1, "chunks": 1},
                intents[0]],
        })
        trace = self.reference("trace", case)
        self.assertEqual(trace["payload"]["value"]["results"], [[2, 0], [], []])
        self.assertEqual(self.reference("check", trace), trace)
        trace["payload"]["value"]["results"][0][0] = 1
        with self.assertRaises(subprocess.CalledProcessError):
            self.reference("check", trace)

    @classmethod
    def setUpClass(cls):
        default = ROOT / "target/debug" / ("fpga-reference.exe" if os.name == "nt" else "fpga-reference")
        cls.executable = Path(os.environ.get("SHOOP_FPGA_REFERENCE", default)).resolve()
        if not cls.executable.is_file():
            raise RuntimeError("build the reference tool first: cargo build -p rt_fpga_testbench")
        cls.compiled = cls.reference("compile", graph())["payload"]["value"]

    @classmethod
    def reference(cls, command, value):
        return wire.run_reference(command, value, executable=cls.executable)

    def case(self):
        compiled = deepcopy(self.compiled)
        bindings = compiled["bindings"]
        memory = compiled["initial_state"]
        memory[bindings[1]["value"]["ramp"]] = {"kind": "ramp", "value": ramp()}
        memory.append(word(U64_MAX))
        return document("case", {
            "name": "record/play with integer limits — no sample access",
            "program": compiled["program"],
            "initial_state": {"memory": memory},
            "queue_capacity": 6,
            "steps": [
                {"writes": []},
                {"writes": [
                    {"address": bindings[3]["value"]["enabled"], "value": word(0)},
                    {"address": bindings[4]["value"]["enabled"], "value": word(1)},
                    {"address": bindings[4]["value"]["position"], "value": word(40)},
                ]},
                {"writes": []},
            ],
        })

    def test_rust_and_python_roundtrip_every_boundary_variant_and_integer_limit(self):
        instructions = [
            {"kind": "process_input", "port": U32_MAX, "destination": U32_MAX},
            {"kind": "process_loop_playback", "view": U32_MAX, "position": U32_MAX,
             "length": U32_MAX, "enabled": U32_MAX, "destination": U32_MAX},
            {"kind": "process_gain", "ramp": U32_MAX, "source": U32_MAX, "destination": U32_MAX},
            {"kind": "process_mix", "left": U32_MAX, "right": U32_MAX, "destination": U32_MAX},
            {"kind": "process_loop_capture", "view": U32_MAX, "enabled": U32_MAX, "source": U32_MAX},
            {"kind": "process_output", "source": U32_MAX, "port": U32_MAX},
        ]
        jobs = [
            {"kind": "input", "port": U32_MAX, "destination": U32_MAX},
            {"kind": "silence", "destination": U32_MAX},
            {"kind": "read_view", "view": U32_MAX, "position_chunks": U64_MAX - 1,
             "length_chunks": U64_MAX, "destination": U32_MAX},
            {"kind": "gain", "source": U32_MAX, "destination": U32_MAX, "ramp": ramp()},
            {"kind": "mix", "left": U32_MAX, "right": U32_MAX, "destination": U32_MAX},
            {"kind": "capture", "source": U32_MAX, "view": U32_MAX},
            {"kind": "output", "source": U32_MAX, "port": U32_MAX},
        ]
        values = [
            graph(), document("compiled", self.compiled),
            document("program", self.compiled["program"]),
            document("instructions", instructions),
            document("jobs", jobs),
            document("state", {"memory": [word(U64_MAX), {"kind": "ramp", "value": ramp()}]}),
            document("bindings", self.compiled["bindings"]),
            document("state_writes", [{"address": U32_MAX, "value": word(U64_MAX)}]),
            document("on_wrap", {"source": U32_MAX, "target": U32_MAX, "enabled": True}),
            document("status", {"kind": "completed"}),
            self.case(), self.reference("trace", self.case()),
        ]
        for error in ["invalid_graph", "invalid_state", "invalid_buffer", "invalid_view", "invalid_shape", "queue_full", "memory_full"]:
            values.append(document("status", {"kind": "failed", "value": {"error": error}}))
        for value in values:
            with self.subTest(kind=value["payload"]["kind"]):
                rust = self.reference("roundtrip", value)
                python = wire.loads(wire.dumps(rust))
                self.assertEqual(python, value)
                self.assertEqual(self.reference("roundtrip", python), value)

    def test_generated_schema_is_current(self):
        result = subprocess.run(
            [str(self.executable), "schema"], text=True, capture_output=True, check=True
        )
        self.assertEqual(json.loads(result.stdout), wire.schema())

    def test_transition_traces_compare_model_native_resolver_and_exact_boundary_results(self):
        case = wire.load(ROOT / "scripts/fpga_testbench/examples/transition_record_play_sync.json")
        self.assertEqual(self.reference("roundtrip", case), case)
        plan = document("transition_plan", case["payload"]["value"]["plan"])
        image = self.reference("compile", plan)
        self.assertEqual(image["payload"]["kind"], "transition_plan_image")
        self.assertEqual(self.reference("roundtrip", image), image)
        trace = self.reference("trace", case)
        self.assertEqual(self.reference("check", wire.loads(wire.dumps(trace))), trace)
        results = trace["payload"]["value"]["results"]
        self.assertEqual(results[4]["states"][1]["mode"], "playing")
        self.assertEqual(results[4]["states"][1]["content_length"], 64)
        self.assertEqual(results[4]["states"][2]["last_pulse"], 64)
        self.assertEqual(results[0]["processed"], {"kind": "accepted", "value": None})
        self.assertEqual(results[0]["states"][1]["mode"], "recording")
        for result in results:
            wire.assert_transition_result_matches(result, deepcopy(result))
        changed = deepcopy(trace)
        changed["payload"]["value"]["results"][4]["states"][1]["position"] += 1
        with self.assertRaises(AssertionError):
            wire.assert_transition_result_matches(results[4], changed["payload"]["value"]["results"][4])
        with self.assertRaises(subprocess.CalledProcessError):
            self.reference("check", changed)

    def test_transition_values_preserve_wide_counters_and_validate_identity_fields(self):
        case = wire.load(ROOT / "scripts/fpga_testbench/examples/transition_record_play_sync.json")
        command = case["payload"]["value"]["steps"][0]["commands"][0]
        command["sequence"] = U64_MAX
        command["at"] = U64_MAX
        command["operation"]["position"] = U64_MAX
        self.assertEqual(self.reference("roundtrip", case), case)
        for field, invalid in [("sequence", U64_MAX + 1), ("at", True)]:
            bad = deepcopy(case)
            bad["payload"]["value"]["steps"][0]["commands"][0][field] = invalid
            self.assert_rejected_by_both(bad)
        bad = deepcopy(case)
        bad["payload"]["value"]["steps"][0]["commands"][0]["target"]["generation"] = U32_MAX + 1
        self.assert_rejected_by_both(bad)
        unsupported = deepcopy(case)
        unsupported["payload"]["value"]["plan"]["description"]["loops"][0]["kind"] = "composite"
        self.assertEqual(self.reference("roundtrip", unsupported), unsupported)
        with self.assertRaises(subprocess.CalledProcessError):
            self.reference("trace", unsupported)

    def test_audio_traces_preserve_samples_readiness_and_reject_sample_mismatches(self):
        compiled = self.reference("compile", document("graph", {
            "quantum": 3, "chunk_frames": 1,
            "nodes": [{"kind": "input", "port": 0},
                      {"kind": "gain", "input": 0, "gain": 8388608},
                      {"kind": "output", "input": 1, "port": 0}],
        }))["payload"]["value"]
        case = document("audio_case", {
            "name": "exact samples", "program": compiled["program"],
            "initial_state": {"memory": compiled["initial_state"]},
            "queue_capacity": 3, "buffers": 3, "output_ports": 1,
            "steps": [{"writes": [], "inputs": [[I32_MIN, I32_MAX, -3]]}],
        })
        trace = self.reference("trace", case)
        self.assertEqual(self.reference("check", wire.loads(wire.dumps(trace))), trace)
        result = trace["payload"]["value"]["results"][0]
        self.assertEqual(result["outputs"], [[-1073741824, 1073741823, -1]])
        self.assertIsNone(result["buffers"][2])
        wire.assert_audio_result_matches(result, deepcopy(result))
        for field in ["outputs", "buffers"]:
            bad = deepcopy(result)
            if field == "outputs":
                bad["outputs"][0][0] += 1
            else:
                bad["buffers"][0] = None
            with self.assertRaises(AssertionError):
                wire.assert_audio_result_matches(result, bad)
        for invalid in [True, 1.0, I32_MAX + 1]:
            bad = deepcopy(case)
            bad["payload"]["value"]["steps"][0]["inputs"][0][0] = invalid
            self.assert_rejected_by_both(bad)

    def test_checked_in_example_generates_a_replayable_trace(self):
        case = wire.load(ROOT / "scripts/fpga_testbench/examples/record_play.json")
        trace = self.reference("trace", case)
        self.assertEqual(len(trace["payload"]["value"]["results"]), 5)
        self.assertEqual(self.reference("check", trace), trace)

    def test_chunk_memory_trace_roundtrip_and_rejection(self):
        case = wire.load(ROOT / "fpga/chunk_memory/examples/ordering.json")
        self.assertEqual(self.reference("roundtrip", case), case)
        trace = self.reference("trace", case)
        self.assertEqual(self.reference("check", wire.loads(wire.dumps(trace))), trace)
        rows = trace["payload"]["value"]["results"]
        self.assertEqual(rows[2]["samples"], [0, I32_MIN, I32_MAX, 3])
        self.assertEqual(rows[6]["samples"], [0, I32_MIN, I32_MAX, -17])
        self.assertEqual(rows[-1]["status"], {"kind": "completed"})
        bad = deepcopy(case)
        bad["payload"]["value"]["requests"][0]["id"] = U32_MAX+1
        self.assert_rejected_by_both(bad)

    def test_elastic_media_growth_reuse_snapshots_and_bounded_capture_failure(self):
        case = wire.load(ROOT / "scripts/fpga_testbench/examples/elastic_record_play.json")
        trace = self.reference("trace", case)
        results = trace["payload"]["value"]["results"]
        self.assertEqual(len(results), 6)
        self.assertEqual(results[2]["outputs"], [[7, 8, 1, 2]])
        self.assertEqual(results[4]["outputs"], [[I32_MIN, I32_MAX, -3, 3]])
        self.assertEqual(results[0]["media"]["views"][0]["chunks"], [0, 1, 2, 3])
        self.assertEqual(results[1]["media"]["views"][0]["chunks"], list(range(8)))
        self.assertEqual(results[3]["media"]["views"][0]["chunks"], [7, 6, 5, 4])
        self.assertEqual(results[-1]["media"]["views"], [None])
        self.assertEqual(results[-1]["media"]["free_chunks"], [0, 1, 2, 3, 7, 6, 5, 4])
        self.assertEqual(self.reference("check", wire.loads(wire.dumps(trace))), trace)
        media = document("media", results[-1]["media"])
        self.assertEqual(self.reference("roundtrip", media), media)
        changed = deepcopy(results[3])
        changed["media"]["views"][0]["chunks"].reverse()
        with self.assertRaises(AssertionError):
            wire.assert_audio_result_matches(results[3], changed)

        bounded = deepcopy(case)
        bounded["payload"]["value"]["media"]["chunk_limit"] = 1
        failed = self.reference("trace", bounded)["payload"]["value"]["results"]
        self.assertEqual(len(failed), 1)
        self.assertEqual(failed[0]["status"], {"kind": "failed", "value": {"error": "memory_full"}})
        self.assertEqual(failed[0]["media"], bounded["payload"]["value"]["media"])

        # Resume from a published state/media pair, preserving physical IDs.
        resumed = deepcopy(case)
        value = resumed["payload"]["value"]
        value["initial_state"] = results[3]["state"]
        value["media"] = results[3]["media"]
        value["steps"] = [value["steps"][4]]
        replay = self.reference("trace", resumed)["payload"]["value"]["results"][0]
        self.assertEqual(replay, results[4])

    def test_shared_ranges_survive_source_clear_and_match_sample_and_ownership_traces(self):
        case = wire.load(ROOT / "scripts/fpga_testbench/examples/shared_ranges.json")
        self.assertEqual(self.reference("roundtrip", case), case)
        trace = self.reference("trace", case)
        results = trace["payload"]["value"]["results"]
        self.assertEqual(len(results), 4)
        self.assertTrue(all(result["status"]["kind"] == "completed" for result in results))
        self.assertEqual(results[0]["outputs"], [[4, 5, 6, 7]])
        self.assertEqual(results[1]["outputs"], [[8, 4, 5, 6]])
        first = results[0]["media"]
        self.assertEqual(first["samples"], case["payload"]["value"]["media"]["samples"])
        self.assertEqual(first["views"][1], {"chunks": [3, 4, 5, 6, 7], "frames": 5, "range": {"offset": 0}})
        self.assertEqual(results[1]["media"]["views"][0]["chunks"], [11, 10, 9, 8])
        self.assertEqual(results[2]["media"]["free_chunks"], [0, 1, 2, 3])
        self.assertEqual(results[3]["media"]["views"][0]["chunks"], list(reversed(range(12))))
        self.assertEqual(results[3]["media"]["free_chunks"], [])
        self.assertEqual(self.reference("check", wire.loads(wire.dumps(trace))), trace)
        media = document("media", results[1]["media"])
        self.assertEqual(self.reference("roundtrip", media), media)
        resumed = deepcopy(case)
        resumed["payload"]["value"]["initial_state"] = results[1]["state"]
        resumed["payload"]["value"]["media"] = results[1]["media"]
        resumed["payload"]["value"]["steps"] = resumed["payload"]["value"]["steps"][2:]
        self.assertEqual(self.reference("trace", resumed)["payload"]["value"]["results"], results[2:])
        for field in ("offset", "writable", "free"):
            observed = deepcopy(results[0])
            if field == "offset":
                observed["media"]["views"][1]["range"]["offset"] = 2
            elif field == "writable":
                del observed["media"]["views"][1]["range"]
            else:
                observed["media"]["free_chunks"].append(0)
            with self.subTest(field=field), self.assertRaises(AssertionError):
                wire.assert_audio_result_matches(results[0], observed)

    def test_shared_range_wire_bounds_and_failed_creation_preserve_media(self):
        case = wire.load(ROOT / "scripts/fpga_testbench/examples/shared_ranges.json")
        for invalid in (None, {"offset": -1}, {"offset": 1.0}, {"offset": U32_MAX + 1}, {}):
            bad = deepcopy(case)
            bad["payload"]["value"]["media"]["views"][0]["range"] = invalid
            self.assert_rejected_by_both(bad)
        aligned = deepcopy(case)
        aligned["payload"]["value"]["media"]["views"][0]["range"] = {"offset": 0}
        self.assertEqual(self.reference("roundtrip", aligned), aligned)
        for field, value in (("start", -1), ("start", 1.0), ("frames", True), ("frames", U64_MAX + 1)):
            bad = deepcopy(case)
            bad["payload"]["value"]["steps"][0]["media_actions"][0][field] = value
            self.assert_rejected_by_both(bad)
        bad = deepcopy(case)
        bad["payload"]["value"]["steps"][0]["media_actions"][0].update(start=U64_MAX, frames=U64_MAX)
        self.assertEqual(self.reference("roundtrip", bad), bad)
        failed = self.reference("trace", bad)["payload"]["value"]["results"]
        self.assertEqual(len(failed), 1)
        self.assertEqual(failed[0]["stage"], "media")
        self.assertEqual(failed[0]["status"], {"kind": "failed", "value": {"error": "invalid_view"}})
        self.assertEqual(failed[0]["media"], bad["payload"]["value"]["media"])

    def test_media_wire_shapes_and_snapshot_invariants_are_checked(self):
        case = wire.load(ROOT / "scripts/fpga_testbench/examples/elastic_record_play.json")
        for field, invalid in [("chunk_frames", 1.0), ("chunk_limit", None),
                               ("chunk_limit", U32_MAX + 1)]:
            bad = deepcopy(case)
            bad["payload"]["value"]["media"][field] = invalid
            self.assert_rejected_by_both(bad)
        empty_actions = deepcopy(case)
        empty_actions["payload"]["value"]["steps"][0]["media_actions"] = []
        self.assertEqual(self.reference("roundtrip", empty_actions), empty_actions)
        bad = deepcopy(case)
        bad["payload"]["value"]["steps"][0]["media_actions"] = None
        self.assert_rejected_by_both(bad)
        bad = deepcopy(case)
        bad["payload"]["value"]["media"]["samples"] = [0]
        # Numerically valid JSON, but not a whole number of physical slots.
        wire.validate(bad)
        with self.assertRaises(subprocess.CalledProcessError):
            self.reference("trace", bad)

    def test_serial_media_command_traces_compare_samples_results_and_chunk_ownership(self):
        case = wire.load(ROOT / "fpga/media_store/examples/shared_ranges.json")
        self.assertEqual(self.reference("roundtrip", case), case)
        trace = self.reference("trace", case)
        self.assertEqual(self.reference("check", wire.loads(wire.dumps(trace))), trace)
        results = trace["payload"]["value"]["results"]
        self.assertEqual(results[5]["value"], {"kind": "samples", "value": [8, 4, 5, 6, 7, 8, 4, 5, 6, 7, 8]})
        self.assertEqual(results[12]["status"], {"kind": "failed", "value": {"error": "invalid_view"}})
        for field in ("samples", "offset", "free"):
            observed = deepcopy(results[5])
            if field == "samples":
                observed["value"]["value"][0] ^= 1
            elif field == "offset":
                observed["media"]["views"][1]["range"]["offset"] = 2
            else:
                observed["media"]["free_chunks"].append(0)
            with self.subTest(field=field), self.assertRaises(AssertionError):
                wire.assert_media_result_matches(results[5], observed)

    def test_trace_runs_the_rust_processor_and_can_be_replayed_after_python_io(self):
        trace = self.reference("trace", self.case())
        results = trace["payload"]["value"]["results"]
        self.assertEqual(len(results), 3)
        self.assertEqual([j["kind"] for j in results[0]["jobs"]],
                         ["input", "gain", "mix", "capture", "silence", "output"])
        self.assertEqual([j["kind"] for j in results[1]["jobs"]],
                         ["input", "gain", "mix", "read_view", "output"])
        self.assertEqual(results[0]["jobs"][1]["ramp"], ramp())
        self.assertEqual(results[1]["jobs"][1]["ramp"]["elapsed"], U32_MAX)
        self.assertEqual(results[1]["jobs"][3]["position_chunks"], 40)
        self.assertEqual(results[2]["jobs"][3]["position_chunks"], 40)
        self.assertEqual(results[2]["state"]["memory"][-1], word(U64_MAX))
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "trace.json"
            wire.dump(trace, path)
            self.assertEqual(self.reference("check", wire.load(path)), trace)
        changed = deepcopy(trace)
        changed["payload"]["value"]["results"][0]["jobs"].pop()
        with self.assertRaises(subprocess.CalledProcessError):
            self.reference("check", changed)

    def test_cocotb_scoreboard_checks_order_state_and_status(self):
        result = self.reference("trace", self.case())["payload"]["value"]["results"][0]
        wire.assert_result_matches(result, deepcopy(result))
        for field in ["jobs", "state", "status"]:
            observed = deepcopy(result)
            if field == "jobs":
                observed["jobs"][0], observed["jobs"][1] = observed["jobs"][1], observed["jobs"][0]
            elif field == "state":
                observed["state"]["memory"][-1] = word(0)
            else:
                observed["status"] = {"kind": "failed", "value": {"error": "invalid_state"}}
            with self.subTest(field=field), self.assertRaises(AssertionError):
                wire.assert_result_matches(result, observed)

    def test_fifo_admission_failure_preserves_state_and_stops_the_trace(self):
        case = self.case()
        case["payload"]["value"]["queue_capacity"] = 5
        trace = self.reference("trace", case)["payload"]["value"]
        self.assertEqual(len(trace["results"]), 1)
        result = trace["results"][0]
        self.assertEqual(result["status"], {"kind": "failed", "value": {"error": "queue_full"}})
        self.assertEqual(result["jobs"], [])
        self.assertEqual(result["state"], trace["case"]["initial_state"])

    def assert_rejected_by_both(self, value):
        with self.assertRaises((ValidationError, ValueError)):
            wire.loads(json.dumps(value))
        result = subprocess.run(
            [str(self.executable), "roundtrip"], input=json.dumps(value), text=True, capture_output=True
        )
        self.assertNotEqual(result.returncode, 0, result.stdout)

    def test_unknown_missing_and_wrongly_typed_fields_are_rejected_by_both(self):
        bad = [
            {"payload": {"kind": "jobs", "value": []}},
            {"format_version": 3, "payload": {"kind": "mystery", "value": []}},
            document("status", {"kind": "completed", "extra": 1}),
            document("bindings", [{"kind": "none", "extra": 1}]),
            document("status", {"kind": "completed", "value": None}),
            document("bindings", [{"kind": "none", "value": None}]),
            document("status", {"kind": "failed", "value": {"error": "mystery"}}),
        ]
        for version in [0, 1, 2, 4, True, 3.0, "3"]:
            value = document("jobs", [])
            value["format_version"] = version
            bad.append(value)
        extra = document("jobs", [])
        extra["extra"] = 1
        bad.append(extra)
        missing = deepcopy(self.compiled["program"])
        del missing["quantum"]
        bad.append(document("program", missing))
        surrogate = self.case()
        surrogate["payload"]["value"]["name"] = "\ud800"
        bad.append(surrogate)
        for value in bad:
            with self.subTest(value=value):
                self.assert_rejected_by_both(value)

    def test_integer_bounds_do_not_depend_on_json_schema_format_annotations(self):
        for value in [-1, U32_MAX + 1, True, 1.0, "1"]:
            with self.subTest(index=value):
                self.assert_rejected_by_both(document("on_wrap", {"source": value, "target": 0, "enabled": True}))
        for value in [-1, U64_MAX + 1, True, 1.0, "1"]:
            with self.subTest(word=value):
                self.assert_rejected_by_both(document("state", {"memory": [word(value)]}))
        for key, value in [("start", I32_MIN - 1), ("target", I32_MAX + 1),
                           ("duration", -1), ("elapsed", U32_MAX + 1)]:
            invalid = ramp()
            invalid[key] = value
            self.assert_rejected_by_both(document("state", {"memory": [{"kind": "ramp", "value": invalid}]}))

    def test_duplicate_fields_and_nonfinite_numbers_are_rejected(self):
        for text in [
            '{"format_version":3,"format_version":3,"payload":{"kind":"jobs","value":[]}}',
            '{"format_version":NaN,"payload":{"kind":"jobs","value":[]}}',
        ]:
            with self.subTest(text=text), self.assertRaises(ValueError):
                wire.loads(text)
            result = subprocess.run(
                [str(self.executable), "roundtrip"], input=text, text=True, capture_output=True
            )
            self.assertNotEqual(result.returncode, 0)


if __name__ == "__main__":
    unittest.main()
