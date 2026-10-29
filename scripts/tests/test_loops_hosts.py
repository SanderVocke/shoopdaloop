"""The browser failure gate requires Rust witnesses, not just matching error text."""
import copy
import gzip
import json
import sys
import struct
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from run_loops_hosts import compare_graph_bindings, compare_graph_worklets, compare_native_graphs, compare_export_worklets, compare_staging_worklets, compare_dense_worklets, compare_media_worklets, compare_planner_worklets, validate_semantic, verify_failure


class DenseTests(unittest.TestCase):
    def fixture(self, stalled=False):
        native = json.loads(gzip.decompress((Path(__file__).parent / "data/loop_dense_native.json.gz").read_bytes()))
        chunks = [[s[c] for s in native["samples"][:128]] + [native["samples"][128][c]] + [0]*127
                  + [s[c] for s in native["samples"][129:]] + [0]*126 for c in range(2)]
        output = [[0]*1024 for _ in range(2)]
        for channel in range(2):
            for segment, start in enumerate((128,384,640)):
                output[channel][start:start+128] = chunks[channel][segment*128:(segment+1)*128]
        reports = []
        for mode in ("offline","live"):
            reports.append(copy.deepcopy({"mode":mode,"session":str((1<<64)-1),"data":native,
                "checkpoints":[{"frame":str(frame),"browser":str(browser),"frames":frames} for frame,browser,frames in ((128,128,128),(129,384,1),(131,640,2))],
                "states":[{"type":"dense-state","frame":"129","outstanding":4,"stop":15},{"type":"dense-state","frame":"131","outstanding":5,"stop":15}],
                "starts":["128","384","640"],"chunks":chunks,"offlineAudio":output if mode=="offline" else None,
                "ready":{"bytes":5963776,"shared":False,"marker":0},"controlBytes":5963776,"controlFinalBytes":5963776,"controlMarker":0x51a17,
                "summary":{"bytes":5963776,"finalBytes":5963776,"heap":[0,0,0],"frames":"131","callbacks":"3","returnedBuffers":10,
                    "returnedGraphBuffers":0,"returnedExportBuffers":0,"returnedStagingBuffers":0,"retainedGraphs":0,"retainedExports":0,"retainedStaging":0,
                    "journalOverflow":0,"recovered":[256,16,16,0],"firstFrame":"128","resumeFrame":None,"marker":0}}))
        if stalled:
            native = json.loads(gzip.decompress((Path(__file__).parent / "data/loop_stalls_native.json.gz").read_bytes()))
            for report in reports:
                report["data"] = copy.deepcopy(native)
                report["checkpoints"].append({"frame":"136","browser":"896","frames":5})
                report["starts"].append("896")
                report["states"].append({"type":"dense-state","frame":"136","outstanding":2,"stop":15})
                report["summary"].update(frames="136",callbacks="4")
                report["transferStall"] = {"frame":"136","outstanding":2,"availableBuffers":0,"received":10,"clientOutstanding":2,"heldBuffers":10}
                for channel in range(2):
                    report["chunks"][channel].extend([s[channel] for s in native["samples"][131:]] + [0]*123)
                    if report["offlineAudio"] is not None:
                        report["offlineAudio"][channel].extend([0]*256)
                        report["offlineAudio"][channel][896:1024] = report["chunks"][channel][384:512]
        return native,{"stallReports" if stalled else "denseReports":reports}

    def test_actual_transfer_buffers_and_rust_result_credits_are_separately_required(self):
        native,result = self.fixture(True); compare_dense_worklets(result,native,stalled=True)
        for field in ("frame","outstanding","availableBuffers","received","clientOutstanding","heldBuffers"):
            altered = copy.deepcopy(result); altered["stallReports"][0]["transferStall"].pop(field)
            with self.assertRaises(RuntimeError,msg=field): compare_dense_worklets(altered,native,stalled=True)
        for field in ("requests","responses","midiBytes","semanticBytes","samples"):
            altered = copy.deepcopy(result); altered["stallReports"][0]["data"][field].pop()
            with self.assertRaises(RuntimeError,msg=field): compare_dense_worklets(altered,native,stalled=True)

    def test_stalled_transport_requires_audible_restart_tail_and_final_recovery(self):
        native,result = self.fixture(True)
        for field,index in (("chunks",384),("chunks",389),("offlineAudio",896),("offlineAudio",901)):
            altered = copy.deepcopy(result); altered["stallReports"][0][field][0][index] += 1
            with self.assertRaises(RuntimeError): compare_dense_worklets(altered,native,stalled=True)
        altered = copy.deepcopy(result); altered["stallReports"][0]["states"].pop()
        with self.assertRaises(RuntimeError): compare_dense_worklets(altered,native,stalled=True)
        for field,value in (("returnedBuffers",9),("frames","135"),("heap",[1,0,0]),("finalBytes",6029312)):
            altered = copy.deepcopy(result); altered["stallReports"][0]["summary"][field]=value
            with self.assertRaises(RuntimeError,msg=field): compare_dense_worklets(altered,native,stalled=True)
        altered = copy.deepcopy(result); altered["stallReports"].pop()
        with self.assertRaises(RuntimeError): compare_dense_worklets(altered,native,stalled=True)

    def test_dense_complete_media_semantics_receipts_and_contexts_are_required(self):
        native,result = self.fixture(); compare_dense_worklets(result,native)
        for field in ("samples","requests","responses","midiBytes","semanticBytes","recovered","counts"):
            altered = copy.deepcopy(result); altered["denseReports"][0]["data"][field].pop()
            with self.assertRaises(RuntimeError,msg=field): compare_dense_worklets(altered,native)
        for field,offset in (("requests",24),("responses",72)):
            altered = copy.deepcopy(result); altered["denseReports"][0]["data"][field][0][offset] ^= 1
            with self.assertRaises(RuntimeError): compare_dense_worklets(altered,native)
        altered = copy.deepcopy(result); altered["denseReports"].pop()
        with self.assertRaises(RuntimeError): compare_dense_worklets(altered,native)

    def test_dense_short_tail_silent_stop_stalls_clocks_and_memory_are_checked(self):
        native,result = self.fixture()
        for field,value in (("returnedBuffers",9),("heap",[0,1,0]),("finalBytes",6029312),("callbacks","2"),("retainedGraphs",1),("journalOverflow",1)):
            altered = copy.deepcopy(result); altered["denseReports"][0]["summary"][field] = value
            with self.assertRaises(RuntimeError,msg=field): compare_dense_worklets(altered,native)
        for field,value in (("states",[]),("checkpoints",[]),("starts",["128"]*3),("controlFinalBytes",6029312),("offlineAudio",None)):
            altered = copy.deepcopy(result); altered["denseReports"][0][field] = value
            with self.assertRaises(RuntimeError,msg=field): compare_dense_worklets(altered,native)
        for field,index in (("chunks",129),("chunks",256),("offlineAudio",640)):
            altered = copy.deepcopy(result); altered["denseReports"][0][field][0][index] = 1
            with self.assertRaises(RuntimeError): compare_dense_worklets(altered,native)
        altered = copy.deepcopy(result); altered["denseReports"][0]["checkpoints"][2]["browser"]="128"
        with self.assertRaises(RuntimeError): compare_dense_worklets(altered,native)


class StagingTests(unittest.TestCase):
    def fixture(self):
        native=json.loads(gzip.decompress((Path(__file__).parent / "data/loop_staging_native.json.gz").read_bytes()))
        clocks=list(range(128,6785,128))+[6785,6789,6793]
        checkpoints=[{"frame":str(frame),"browser":str(128+i*256),"frames":count} for i,(frame,count) in enumerate(zip(clocks,[128]*53+[1,4,4]))]
        segments=[{"browser":checkpoints[i]["browser"],"frame":str(frame),"frames":count} for i,frame,count in ((0,0,128),(53,6784,1),(54,6785,4),(55,6789,4))]
        starts=[segment["browser"] for segment in segments]
        accepted=[{"bytes":request,"result":0,"frame":str(frame)} for request,frame in zip(native["stagingRequests"],list(range(0,6785,128))+[6785,6789])]
        bad=accepted[0]["bytes"].copy();bad[0]^=1
        foreign=accepted[0]["bytes"].copy();foreign[8]^=128
        chunks=[[0]*512 for _ in range(2)];offset=0
        for segment,count in enumerate((128,1,4,4)):
            for channel in range(2):chunks[channel][segment*128:segment*128+count]=[sample[channel] for sample in native["samples"][offset:offset+count]]
            offset+=count
        audio=[[0]*14592 for _ in range(2)]
        for channel in range(2):
            for segment,start in enumerate(map(int,starts)):audio[channel][start:start+128]=chunks[channel][segment*128:(segment+1)*128]
        reports=[]
        for mode in ("offline","live"):
            reports.append(copy.deepcopy({"mode":mode,"session":str((1<<64)-1),"data":native,"checkpoints":checkpoints,"segments":segments,"starts":starts,"chunks":chunks,"ingress":[{"bytes":bad,"result":2,"frame":"0"},{"bytes":foreign,"result":9,"frame":"0"}]+accepted,
                "ready":{"bytes":1835008,"shared":False,"marker":0},"controlBytes":1835008,"controlFinalBytes":1835008,"controlMarker":0x51a17,
                "summary":{"bytes":1835008,"finalBytes":1835008,"heap":[0,0,0],"returnedBuffers":10,"returnedStagingBuffers":2,"retainedStaging":0,"journalOverflow":0,"recovered":[256,16,16,0],"frames":"6793","callbacks":"56","firstFrame":"128","marker":0},"offlineAudio":audio if mode=="offline" else None}))
        return native,{"stagingReports":reports}

    def test_complete_staging_content_owning_returns_and_both_contexts_are_required(self):
        native,result=self.fixture();compare_staging_worklets(result,native)
        for field in ("samples","stagingRequests","stagingResponses","responses","midiBytes","semanticBytes","recovered","counts"):
            altered=copy.deepcopy(result);altered["stagingReports"][0]["data"][field].pop()
            with self.assertRaises(RuntimeError,msg=field):compare_staging_worklets(altered,native)
        for field,offset in (("stagingRequests",64),("stagingResponses",144),("responses",72)):
            altered=copy.deepcopy(result);altered["stagingReports"][0]["data"][field][0][offset]^=1
            with self.assertRaises(RuntimeError):compare_staging_worklets(altered,native)
        altered=copy.deepcopy(result);altered["stagingReports"].pop()
        with self.assertRaises(RuntimeError):compare_staging_worklets(altered,native)

    def test_short_callback_clocks_audio_tails_memory_and_credit_evidence_are_mandatory(self):
        native,result=self.fixture()
        for field,value in (("returnedStagingBuffers",1),("retainedStaging",1),("heap",[0,1,0]),("finalBytes",1900544),("callbacks","55")):
            altered=copy.deepcopy(result);altered["stagingReports"][0]["summary"][field]=value
            with self.assertRaises(RuntimeError):compare_staging_worklets(altered,native)
        for field,value in (("checkpoints",[]),("segments",[]),("starts",["0"]*4),("controlFinalBytes",1900544),("ingress",[]),("offlineAudio",None)):
            altered=copy.deepcopy(result);altered["stagingReports"][0][field]=value
            with self.assertRaises(RuntimeError):compare_staging_worklets(altered,native)
        for field,index in (("chunks",129),("offlineAudio",13697)):
            altered=copy.deepcopy(result);altered["stagingReports"][0][field][0][index]=1
            with self.assertRaises(RuntimeError):compare_staging_worklets(altered,native)
        altered=copy.deepcopy(result);altered["stagingReports"][0]["checkpoints"][1]["browser"]="128"
        with self.assertRaises(RuntimeError):compare_staging_worklets(altered,native)


class ImmutableExportTests(unittest.TestCase):
    def fixture(self):
        native = json.loads(gzip.decompress((Path(__file__).parent / "data/loop_export_native.json.gz").read_bytes()))
        reports = []
        for mode in ("offline", "live"):
            starts = [128,384]
            checkpoints = [{"frame": str(128*i), "browser": str(128+(i-1)*256)} for i in range(1,45)]
            accepted = [{"bytes": request, "result": 0, "frame": str(0 if i == 0 else 128 if i == 1 else 256+(i-2)*128)} for i,request in enumerate(native["exportRequests"])]
            bad = native["exportRequests"][0].copy(); bad[95] = 1
            foreign = native["exportRequests"][0].copy(); foreign[8] ^= 128
            audio = [[0]*11520 for _ in range(2)]
            for channel in range(2):
                for segment,start in enumerate(starts): audio[channel][start:start+128] = [sample[channel] for sample in native["samples"][segment*128:(segment+1)*128]]
            reports.append({"mode": mode, "session": str((1<<64)-1), "data": copy.deepcopy(native), "checkpoints": checkpoints, "starts": list(map(str,starts)), "ingress": [{"bytes":bad,"result":4,"frame":"0"},{"bytes":foreign,"result":9,"frame":"0"}]+accepted,
                "ready": {"bytes":1835008,"shared":False}, "controlBytes":1835008,"controlFinalBytes":1835008,"controlMarker":0x51a17,
                "summary": {"bytes":1835008,"finalBytes":1835008,"heap":[0,0,0],"returnedBuffers":10,"returnedExportBuffers":2,"retainedExports":0,"journalOverflow":0,"recovered":[256,16,16,0],"frames":"5632","callbacks":"44","firstFrame":"128","resumeFrame":"384","marker":0}, "offlineAudio":audio if mode == "offline" else None})
        return native, {"exportReports": reports}

    def test_complete_immutable_export_contexts_are_required(self):
        native, result = self.fixture(); compare_export_worklets(result, native)
        for field in ("exportRequests","exportResponses","responses","samples","midiBytes","semanticBytes","recovered","counts","storage"):
            altered = copy.deepcopy(result); altered["exportReports"][0]["data"][field].pop()
            with self.assertRaises((RuntimeError, ValueError)): compare_export_worklets(altered, native)
        for field,value in (("exportReports",[]),("exportReports",result["exportReports"][:1])):
            altered=copy.deepcopy(result); altered[field]=value
            with self.assertRaises(RuntimeError): compare_export_worklets(altered,native)
        altered=copy.deepcopy(result); altered["exportReports"][0]["data"]["exportResponses"][1][256] = 1
        with self.assertRaises(RuntimeError): compare_export_worklets(altered,native)

    def test_clock_transactional_rejections_memory_and_actual_output_cannot_be_omitted(self):
        native,result=self.fixture()
        for field,value in (("returnedExportBuffers",1),("retainedExports",1),("heap",[1,0,0]),("finalBytes",1900544),("recovered",[255,16,16,0]),("callbacks","43")):
            altered=copy.deepcopy(result); altered["exportReports"][0]["summary"][field]=value
            with self.assertRaises(RuntimeError): compare_export_worklets(altered,native)
        for field,value in (("checkpoints",[]),("starts",["128","512"]),("controlFinalBytes",1900544),("ingress",[]),("offlineAudio",None)):
            altered=copy.deepcopy(result); altered["exportReports"][0][field]=value
            with self.assertRaises(RuntimeError): compare_export_worklets(altered,native)
        altered=copy.deepcopy(result); altered["exportReports"][0]["offlineAudio"][1][384] += 1
        with self.assertRaises(RuntimeError): compare_export_worklets(altered,native)


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
        planned = kind.startswith("planner")
        error = kind in ("processor", "planner")
        label = "planner" if kind == "planner" else "planner Rust heap" if planned else "processor" if error else "Rust heap"
        witness = {"code": 10 if error else 0xffff0001,
                   "processor": 0 if planned else 1 if error else 2,
                   "heap": [0, 0, 0] if error else [1, 7, 1]}
        if planned:
            witness.update(planner=3 if error else 4, plans=1 if error else 128,
                           finishes=0 if error else 128, frame="128", callbacks="1", bytes=1310720, finalBytes=1310720)
        return {"passed": False, "failure": "intentional loop " + label + " failure canary detected", "workletFailure": witness}

    def test_matching_rust_witnesses_and_nonzero_exit_are_required(self):
        for kind in ("processor", "rust-heap", "graph-processor", "graph-rust-heap", "planner", "planner-rust-heap"):
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
        for kind in ("processor", "rust-heap", "graph-processor", "graph-rust-heap", "planner", "planner-rust-heap"):
            for field, value in (("planner" if kind.startswith("planner") else "processor", 0), ("code", None), ("heap", []), ("heap", ["1", 7, 1])):
                altered = self.report(kind)
                altered["workletFailure"][field] = value
                with self.assertRaises(RuntimeError):
                    verify_failure(altered, 1, kind)
        for counts in ([0, 7, 1], [1, 0, 1], [1, 7, 0], [1, -1, 1], [True, 7, 1]):
            altered = self.report("rust-heap")
            altered["workletFailure"]["heap"] = counts
            with self.assertRaises(RuntimeError):
                verify_failure(altered, 1, "rust-heap")


class PlannerFailureGateTests(unittest.TestCase):
    report = FailureGateTests.report

    def test_planner_phase_counters_and_processor_origin_cannot_be_substituted(self):
        for kind in ("planner", "planner-rust-heap"):
            for field, value in (("planner", 0), ("planner", 1), ("processor", 1), ("plans", 0),
                                 ("finishes", 1), ("frame", "1"), ("callbacks", "0"),
                                 ("plans", True), ("finishes", False), ("processor", False),
                                 ("bytes", 0), ("finalBytes", 1376256)):
                altered = self.report(kind)
                altered["workletFailure"][field] = value
                with self.assertRaises(RuntimeError):
                    verify_failure(altered, 1, kind)


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


class MediaWorkletTests(unittest.TestCase):
    """Captured parser inputs, not fresh browser or musical execution evidence."""
    def fixture(self):
        source = json.loads(gzip.decompress((Path(__file__).parent / "data/loop_media_reports.json.gz").read_bytes()))
        return {"mediaReports": source["mediaReports"]}, source["native"]

    def test_media_requires_all_content_receipts_state_projection_and_contexts(self):
        result, native = self.fixture()
        compare_media_worklets(result, native)
        for field in ("samples", "requests", "responses", "stagingRequests", "stagingResponses",
                      "exportRequests", "exportResponses", "midiBytes", "semanticBytes"):
            altered = copy.deepcopy(result)
            altered["mediaReports"][0]["data"][field] = []
            with self.assertRaises(RuntimeError, msg=field):
                compare_media_worklets(altered, native)
        for field, index, at in (("stagingResponses", 5, 88), ("stagingResponses", 7, 120),
                                 ("exportResponses", 1, 136), ("exportResponses", 14, 256),
                                 ("exportResponses", 45, 84)):
            altered = copy.deepcopy(result)
            altered["mediaReports"][0]["data"][field][index][at] ^= 1
            with self.assertRaises(RuntimeError):
                compare_media_worklets(altered, native)
        for reports in ([], result["mediaReports"][:1], result["mediaReports"][::-1]):
            with self.assertRaises(RuntimeError):
                compare_media_worklets({"mediaReports": reports}, native)

    def test_media_requires_clocked_downstream_tails_alias_refusals_and_full_recovery(self):
        result, native = self.fixture()
        for field, value in (("heap", [1,0,0]), ("frames", "90"), ("callbacks", "29"),
                             ("returnedExportBuffers", 1), ("returnedStagingBuffers", 1),
                             ("recovered", [41,16,16,0]), ("retainedExports", 1),
                             ("finalBytes", result["mediaReports"][0]["summary"]["bytes"]+65536)):
            altered = copy.deepcopy(result)
            altered["mediaReports"][0]["summary"][field] = value
            with self.assertRaises(RuntimeError, msg=field):
                compare_media_worklets(altered, native)
        for change in (lambda r: r["chunks"][0].__setitem__(127, 1),
                       lambda r: r["offlineAudio"][0].__setitem__(0, 1),
                       lambda r: r["checkpoints"][2].__setitem__("frames", 56),
                       lambda r: r["starts"].__setitem__(1, r["starts"][0]),
                       lambda r: r["ingress"][3].__setitem__("result", 0),
                       lambda r: r["ingress"][3]["bytes"].__setitem__(48, 1),
                       lambda r: r["data"]["counts"].__setitem__(7, 1),
                       lambda r: r.__setitem__("controlFinalBytes", r["controlBytes"]+65536)):
            altered = copy.deepcopy(result)
            change(altered["mediaReports"][0])
            with self.assertRaises(RuntimeError):
                compare_media_worklets(altered, native)


class PlannerControlTests(unittest.TestCase):
    """Captured parser inputs; actual worklet execution is a separate gate."""
    def fixture(self):
        source = json.loads(gzip.decompress((Path(__file__).parent / "data/loop_planner_reports.json.gz").read_bytes()))
        return {"plannerReports": source["plannerReports"]}, source["native"]

    def test_planner_control_requires_both_contexts_full_samples_and_semantics(self):
        result, native = self.fixture()
        compare_planner_worklets(result, native)
        for reports in ([], result["plannerReports"][:1], result["plannerReports"][::-1], [None, None]):
            with self.assertRaises(RuntimeError):
                compare_planner_worklets({"plannerReports": reports}, native)
        for field in ("samples", "semanticBytes", "recovered"):
            altered = copy.deepcopy(result)
            altered["plannerReports"][0]["data"][field] = []
            with self.assertRaises(RuntimeError):
                compare_planner_worklets(altered, native)
        for change in (lambda r: r["audio"][0].__setitem__(128, 1),
                       lambda r: r["offlineAudio"][1].__setitem__(0, 1),
                       lambda r: r.__setitem__("start", "129"),
                       lambda r: r["data"]["semanticBytes"].__setitem__(88, 99)):
            altered = copy.deepcopy(result)
            change(altered["plannerReports"][0])
            with self.assertRaises(RuntimeError):
                compare_planner_worklets(altered, native)

    def test_planner_control_requires_real_lifecycle_fixed_memory_and_successful_recovery(self):
        result, native = self.fixture()
        for key, value in (("plans", 1), ("finishes", 0), ("callbacks", "2"), ("frame", "0"),
                           ("heap", [1,0,0]), ("recovered", [7,4,4]), ("marker", 1),
                           ("finalBytes", result["plannerReports"][0]["summary"]["bytes"]+65536)):
            altered = copy.deepcopy(result)
            altered["plannerReports"][0]["summary"][key] = value
            with self.assertRaises(RuntimeError, msg=key):
                compare_planner_worklets(altered, native)
        for key, value in (("controlMarker", 0), ("controlFinalBytes", 0)):
            altered = copy.deepcopy(result)
            altered["plannerReports"][1][key] = value
            with self.assertRaises(RuntimeError):
                compare_planner_worklets(altered, native)


if __name__ == "__main__":
    unittest.main()
