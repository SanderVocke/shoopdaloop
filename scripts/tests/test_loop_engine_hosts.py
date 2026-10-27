"""The browser failure gate requires Rust witnesses, not just matching error text."""
import copy
import json
import sys
import struct
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from run_loop_engine_hosts import compare_graph_bindings, compare_graph_worklets, compare_native_graphs, validate_semantic, verify_failure


class NativeGraphTransportTests(unittest.TestCase):
    def test_complete_threaded_report_only_normalizes_correlated_opaque_scope(self):
        native = json.loads((Path(__file__).parent / "data/loop_graph_native.json").read_text())
        native["counts"] = [0] * 5
        threaded = copy.deepcopy(native)
        for request in threaded["graphRequests"][1:]:
            request[64:68] = list(struct.pack("<I", 0xffffffff))
        for response in threaded["graphResponses"]:
            response[76:80] = list(struct.pack("<I", 0xffffffff))
        compare_native_graphs(native, threaded)
        original = copy.deepcopy(threaded)
        for field in ("samples", "midi", "transitions", "semanticBytes", "recovered", "counts",
                      "payloads", "graphRequests", "graphResponses", "responses"):
            altered = copy.deepcopy(threaded)
            altered[field].pop()
            with self.assertRaises(RuntimeError, msg=field):
                compare_native_graphs(native, altered)
        for field in ("frames", "callbacks"):
            altered = copy.deepcopy(threaded); altered[field] += 1
            with self.assertRaises(RuntimeError):
                compare_native_graphs(native, altered)
        for field, index, offset in (("graphRequests", 1, 64), ("graphResponses", 2, 76),
                                     ("graphResponses", 2, 56), ("payloads", 0, 95),
                                     ("responses", 0, 95)):
            altered = copy.deepcopy(threaded); altered[field][index][offset] ^= 1
            with self.assertRaises(RuntimeError):
                compare_native_graphs(native, altered)
        self.assertEqual(threaded, original)


class FailureGateTests(unittest.TestCase):
    def report(self, kind):
        kind = kind.removeprefix("graph-")
        return {
            "passed": False,
            "failure": "intentional loop " + ("processor" if kind == "processor" else "Rust heap") + " failure canary detected",
            "workletFailure": {"code": 10 if kind == "processor" else 0xffff0001,
                               "processor": 1 if kind == "processor" else 2,
                               "heap": [0, 0, 0] if kind == "processor" else [1, 7, 1]},
        }

    def test_matching_rust_witnesses_and_nonzero_exit_are_required(self):
        for kind in ("processor", "rust-heap", "graph-processor", "graph-rust-heap"):
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
        for kind in ("processor", "rust-heap", "graph-processor", "graph-rust-heap"):
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


class SemanticJournalTests(unittest.TestCase):
    def sample(self):
        data = bytearray(40 + 3 * 160)
        data[:8] = b"LSJ1\x01\x00\xa0\x00"
        struct.pack_into("<I", data, 8, 3)
        frame = (1 << 64) - 1
        for index, kind in enumerate((1, 2, 3)):
            at = 40 + index * 160
            data[at] = kind
            struct.pack_into("<Q", data, at + 8, frame)
            if index:
                struct.pack_into("<I", data, at + 20, 1)
        struct.pack_into("<II", data, 40 + 16, 1, 1)
        data[40 + 2 * 160 + 1] = 9
        return data

    def test_complete_full_width_records_and_all_truncations(self):
        data = self.sample()
        self.assertEqual(validate_semantic(data), {"boundaries": [str((1 << 64) - 1)],
            "decisions": 1, "events": 1, "records": 3, "loss": ["0", "0", "0"]})
        for length in range(len(data)):
            with self.assertRaises(RuntimeError):
                validate_semantic(data[:length])
        with self.assertRaises(RuntimeError):
            validate_semantic(data + b"\x00")

    def test_padding_flags_identities_counts_and_optional_fields_are_canonical(self):
        positions = [0, 4, 6, 12, 40 + 1, 40 + 4, 40 + 32,
                     200 + 3, 200 + 4, 200 + 72, 200 + 80, 200 + 96, 200 + 104,
                     360 + 2, 360 + 4, 360 + 24, 360 + 32, 360 + 40]
        for position in positions:
            data = self.sample()
            data[position] ^= 1
            with self.assertRaises(RuntimeError, msg=str(position)):
                validate_semantic(data)
        for at, value in ((8, 129), (56, 3), (60, 2), (220, 0), (380, 0)):
            data = self.sample()
            struct.pack_into("<I", data, at, value)
            with self.assertRaises(RuntimeError):
                validate_semantic(data)
        for at, value in ((200, 3), (201, 3), (202, 128), (360, 2), (361, 0), (361, 15)):
            data = self.sample()
            data[at] = value
            with self.assertRaises(RuntimeError):
                validate_semantic(data)

    def test_loss_must_be_explicitly_accepted_and_batches_must_be_ordered(self):
        for loss in ((1, 3, 0), (0, 0, 7)):
            data = self.sample()
            struct.pack_into("<QQQ", data, 16, *loss)
            with self.assertRaises(RuntimeError):
                validate_semantic(data)
            self.assertEqual(validate_semantic(data, complete=False)["loss"], list(map(str, loss)))
        data = self.sample()
        data.extend(data[40:])
        struct.pack_into("<I", data, 8, 6)
        with self.assertRaises(RuntimeError):
            validate_semantic(data)
        data = self.sample()
        struct.pack_into("<QQ", data, 16, 1, 0)
        with self.assertRaises(RuntimeError):
            validate_semantic(data, complete=False)


class GraphBindingTests(unittest.TestCase):
    def sample(self):
        native = {"samples": [[1.0, 10.0]] * 13, "midi": [[(1 << 64) - 1, [0x90, 60, 100]]],
                  "semanticBytes": list(SemanticJournalTests().sample()), "recovered": [256, 16, 16, 0]}
        packets = []
        for phase, flags, sequence, status, runtime, at, delivered, observed, epoch, kind in (
                (1, 6, 1, 1, 4, 0, 0, 0, 1, 1), (1, 2, 2, 3, 5, 4, 0, 1, 0, 2),
                (2, 3, 2, 4, 5, 4, 4, 4, 2, 2), (1, 6, 3, 2, 6, 0, 0, 13, 0, 3)):
            data = bytearray(128)
            data[:8] = b"LLS1\x01\x00" + bytes((phase, flags))
            struct.pack_into("<QQIIQQQQQI", data, 8, (1 << 64) - 1, sequence, 0, status,
                             runtime, at, at, delivered, observed, 0)
            struct.pack_into("<IIIQQ", data, 76, 7, 0, 1, epoch, 0)
            if status in (1, 2):
                struct.pack_into("<IIIII", data, 104, 0, 1 if status == 1 else 2,
                                 *([4, 16, 4] if status == 1 else [3, 12, 2]))
            data[124] = kind
            packets.append(list(data))
        report = {"audio": [[sample[channel] * (2 if 4 <= at < 12 else 1)
                             for at, sample in enumerate(native["samples"])] for channel in range(2)],
                  "midiBytes": list(struct.pack("<QI", (1 << 64) - 1, 3) + bytes([0x90, 60, 100])),
                  "semanticBytes": native["semanticBytes"], "recovered": native["recovered"],
                  "rounds": 128, "generation": 130, "initialBytes": 1835008, "finalBytes": 1835008,
                  "graphResponses": packets}
        return report, native

    def test_complete_graph_tables_reject_all_receipt_truncations_and_byte_corruptions(self):
        report, native = self.sample()
        compare_graph_bindings(report, native)
        for index in range(4):
            for length in range(128):
                altered = copy.deepcopy(report)
                altered["graphResponses"][index] = altered["graphResponses"][index][:length]
                with self.assertRaises(RuntimeError):
                    compare_graph_bindings(altered, native)
            for position in range(128):
                altered = copy.deepcopy(report)
                altered["graphResponses"][index][position] ^= 1
                with self.assertRaises(RuntimeError):
                    compare_graph_bindings(altered, native)

    def test_graph_music_semantics_recovery_and_128_rounds_are_independently_required(self):
        report, native = self.sample()
        for field, value in (("audio", []), ("midiBytes", []), ("semanticBytes", []),
                             ("recovered", [255,16,16,0]), ("rounds", 127), ("generation", 129),
                             ("finalBytes", 1900544), ("graphResponses", report["graphResponses"][:3])):
            altered = copy.deepcopy(report)
            altered[field] = value
            with self.assertRaises(RuntimeError):
                compare_graph_bindings(altered, native)


class GraphWorkletReportTests(unittest.TestCase):
    """Report validation inputs, not fresh Web Audio or musical execution evidence."""
    def sample(self):
        native = json.loads((Path(__file__).parent / "data/loop_graph_native.json").read_text())
        reports = []
        for index, mode in enumerate(("offline", "live")):
            session, scope, release = (1 << 64) - 3 - index, 11 + index, 129 + index * 256
            def packet(raw, scope_at=None):
                data = bytearray(raw)
                struct.pack_into("<Q", data, 8, session)
                if scope_at is not None:
                    struct.pack_into("<I", data, scope_at, scope)
                return list(data)
            payloads = [{"bytes": packet(raw), "result": 0, "frame": "0"} for raw in native["payloads"]]
            bad = copy.deepcopy(payloads[0]); bad["bytes"][7] ^= 128; bad["result"] = 4
            payloads.insert(0, bad)
            requests = [{"bytes": packet(raw, 64 if offset else None), "result": 0,
                         "frame": str((0, 1, release)[offset])} for offset, raw in enumerate(native["graphRequests"])]
            bad = copy.deepcopy(requests[0]); bad["bytes"][8] ^= 128; bad["result"] = 9
            requests.insert(0, bad)
            responses = [packet(raw, 76) for raw in native["graphResponses"]]
            struct.pack_into("<Q", data := bytearray(responses[3]), 64, release)
            responses[3] = list(data)
            audio = [[sample[channel] for sample in native["samples"]] for channel in range(2)]
            chunks = [[channel[0]] + [0] * 127 + channel[1:] for channel in audio]
            starts = [128, 256] if not index else [512, 1024]
            offline = None
            if not index:
                offline = [[0] * 768 for _ in range(2)]
                for channel in range(2):
                    for segment, start in enumerate(starts):
                        offline[channel][start:start+128] = chunks[channel][segment*128:(segment+1)*128]
            frames = release + 128
            report = {"mode": mode, "session": str(session), "audio": audio, "chunks": chunks,
                      "midiBytes": list(b"".join(struct.pack("<QI", at, len(raw)) + bytes(raw) for at, raw in native["midi"])),
                      "semanticBytes": native["semanticBytes"], "responses": [packet(raw) for raw in native["responses"]],
                      "payloads": payloads, "graphRequests": requests, "graphResponses": responses,
                      "starts": list(map(str, starts)), "checkpoint": {"type": "checkpoint", "frame": "1", "browser": str(starts[0])},
                      "summary": {"firstFrame": str(starts[0]), "resumeFrame": str(starts[1]), "frames": str(frames),
                                  "callbacks": str((frames - 1) // 128 + 1), "bytes": 1835008, "finalBytes": 1835008,
                                  "heap": [0,0,0], "journalOverflow": 0, "returnedBuffers": 10, "returnedGraphBuffers": 4,
                                  "retainedGraphs": 0, "recovered": native["recovered"], "marker": 0},
                      "ready": {"bytes": 1835008, "shared": False, "marker": 0}, "invalidReplies": 4, "duplicateReplies": 4,
                      "controlBytes": 1835008, "controlFinalBytes": 1835008, "controlMarker": 0x51a17,
                      "counts": [0,0,0,0,0], "returnedInputBuffers": [5,1,2], "offlineAudio": offline}
            reports.append(report)
        return {"graphReports": reports}, native

    def test_actual_contexts_clock_segments_and_all_downstream_samples_are_required(self):
        result, native = self.sample()
        compare_graph_worklets(result, native)
        for field, value in (("audio", []), ("chunks", []), ("starts", ["128", "128"]),
                             ("checkpoint", {"type": "checkpoint", "frame": "128", "browser": "128"}),
                             ("offlineAudio", None), ("midiBytes", []), ("semanticBytes", [])):
            altered = copy.deepcopy(result); altered["graphReports"][0][field] = value
            with self.assertRaises(RuntimeError, msg=field):
                compare_graph_worklets(altered, native)
        for reports in ([], result["graphReports"][:1], result["graphReports"][::-1]):
            with self.assertRaises(RuntimeError):
                compare_graph_worklets({"graphReports": reports}, native)

    def test_real_ingress_refusals_sessions_and_ready_identity_cannot_be_relabelled(self):
        result, native = self.sample()
        changes = (("payloads", 0, "result", 0), ("payloads", 1, "frame", "1"),
                   ("graphRequests", 0, "result", 0), ("graphRequests", 3, "frame", "130"))
        for field, index, key, value in changes:
            altered = copy.deepcopy(result); altered["graphReports"][0][field][index][key] = value
            with self.assertRaises(RuntimeError):
                compare_graph_worklets(altered, native)
        for field, index, at in (("payloads", 1, 8), ("payloads", 3, 40), ("graphRequests", 2, 64)):
            altered = copy.deepcopy(result); altered["graphReports"][0][field][index]["bytes"][at] ^= 1
            with self.assertRaises(RuntimeError):
                compare_graph_worklets(altered, native)
        altered = copy.deepcopy(result)
        altered["graphReports"][0]["responses"][0], altered["graphReports"][0]["responses"][1] = altered["graphReports"][0]["responses"][1], altered["graphReports"][0]["responses"][0]
        with self.assertRaises(RuntimeError):
            compare_graph_worklets(altered, native)

    def test_graph_memory_heap_ownership_buffers_and_terminal_clocks_are_independent_gates(self):
        result, native = self.sample()
        for field, value in (("heap", [1,0,0]), ("frames", "129"), ("returnedGraphBuffers", 3),
                             ("retainedGraphs", 1), ("recovered", [255,16,16,0]), ("finalBytes", 1900544)):
            altered = copy.deepcopy(result); altered["graphReports"][0]["summary"][field] = value
            with self.assertRaises(RuntimeError):
                compare_graph_worklets(altered, native)
        for field, value in (("controlFinalBytes", 1900544), ("invalidReplies", 0), ("duplicateReplies", 0),
                             ("counts", [0,0,1,0,0]), ("returnedInputBuffers", [5,0,2])):
            altered = copy.deepcopy(result); altered["graphReports"][0][field] = value
            with self.assertRaises(RuntimeError):
                compare_graph_worklets(altered, native)
        altered = copy.deepcopy(result); altered["graphReports"][0]["graphResponses"][2][56] ^= 1
        with self.assertRaises(RuntimeError):
            compare_graph_worklets(altered, native)


if __name__ == "__main__":
    unittest.main()
