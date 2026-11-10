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
    return {"format_version": 1, "payload": {"kind": kind, "value": value}}


def word(value):
    return {"kind": "word", "value": value}


def ramp():
    return {
        "start": I32_MIN, "target": I32_MAX,
        "duration": U32_MAX, "elapsed": U32_MAX - 16,
    }


def graph():
    return document("graph", {
        "quantum": 32,
        "nodes": [
            {"kind": "input", "port": 0},
            {"kind": "gain", "input": 0, "gain": 8388608},
            {"kind": "mix", "left": 0, "right": 1},
            {"kind": "capture", "input": 2, "view": 17, "recording": True},
            {"kind": "playback", "view": 17, "length": 45, "position": 0, "playing": False},
            {"kind": "output", "input": 4, "port": 0},
        ],
    })


class CrossLanguageContract(unittest.TestCase):
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
            {"kind": "read_view", "view": U32_MAX, "position": U64_MAX - 1,
             "length": U64_MAX, "destination": U32_MAX},
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
        for error in ["invalid_graph", "invalid_state", "invalid_buffer", "invalid_view", "invalid_shape", "queue_full"]:
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

    def test_audio_traces_preserve_samples_readiness_and_reject_sample_mismatches(self):
        compiled = self.reference("compile", document("graph", {
            "quantum": 3,
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
        self.assertEqual(results[1]["jobs"][3]["position"], 40)
        self.assertEqual(results[2]["jobs"][3]["position"], 40)
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
            {"format_version": 1, "payload": {"kind": "mystery", "value": []}},
            document("status", {"kind": "completed", "extra": 1}),
            document("bindings", [{"kind": "none", "extra": 1}]),
            document("status", {"kind": "completed", "value": None}),
            document("bindings", [{"kind": "none", "value": None}]),
            document("status", {"kind": "failed", "value": {"error": "mystery"}}),
        ]
        for version in [0, 2, True, 1.0, "1"]:
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
            '{"format_version":1,"format_version":1,"payload":{"kind":"jobs","value":[]}}',
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
