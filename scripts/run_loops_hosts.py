#!/usr/bin/env python3
"""Run independent primitive/graph loop hosts in native and real Web Audio contexts."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import urllib.request

import run_rt_runtime_hosts as harness

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "tests/loops_host"
ASSETS = ROOT / "target/loops-host"


def validate_semantic(data, *, complete=True, capacity=128):
    """Validate LSJ1 framing/canonical fields without reconstructing musical policy."""
    data = bytes(data)
    if len(data) < 40 or data[:8] != b"LSJ1\x01\x00\xa0\x00" or any(data[12:16]):
        raise RuntimeError("semantic journal header")
    count = struct.unpack_from("<I", data, 8)[0]
    loss = struct.unpack_from("<QQQ", data, 16)
    if count > capacity or len(data) != 40 + count * 160:
        raise RuntimeError("semantic journal length")
    if (bool(loss[0]) != bool(loss[1])) or loss[1] < loss[0] or (complete and any(loss)):
        raise RuntimeError("incomplete semantic journal")
    records = [data[40 + i * 160:40 + (i + 1) * 160] for i in range(count)]
    cursor, previous, decisions_total, events_total = 0, None, 0, 0
    frames = []
    while cursor < count:
        boundary = records[cursor]
        frame = struct.unpack_from("<Q", boundary, 8)[0]
        decisions, events = struct.unpack_from("<II", boundary, 16)
        source_loss = struct.unpack_from("<Q", boundary, 24)[0]
        if (boundary[0] != 1 or any(boundary[1:8]) or any(boundary[32:])
                or source_loss > loss[2] or (previous is not None and frame <= previous)
                or cursor + 1 + decisions + events > count):
            raise RuntimeError("semantic boundary framing")
        previous = frame
        frames.append(str(frame))
        cursor += 1
        seen = set()
        for index in range(decisions + events):
            record = records[cursor]
            kind, code, flags = record[:3]
            slot, generation = struct.unpack_from("<II", record, 16)
            if (any(record[3:8]) or struct.unpack_from("<Q", record, 8)[0] != frame
                    or not generation or slot == 0xffffffff):
                raise RuntimeError("semantic identity/frame/padding")
            if index < decisions:
                if (kind != 2 or code > 2 or flags & ~127 or any(record[104:])
                        or (slot, generation) in seen
                        or (not flags & 8 and any(record[72:80]))
                        or (not flags & 16 and (flags & 32 or any(record[80:104])))
                        or (not flags & 32 and any(record[96:104]))):
                    raise RuntimeError("semantic decision encoding")
                seen.add((slot, generation))
            else:
                a, b = struct.unpack_from("<QQ", record, 24)
                if (kind != 3 or code not in range(1, 15) or flags > 1 or any(record[40:])
                        or (code not in (1, 2, 4, 5, 6, 7) and flags)
                        or (code not in (2, 4, 5, 6, 7, 12, 14) and a)
                        or (code not in (5, 12) and b)
                        or (code == 14 and a not in range(257, 269))):
                    raise RuntimeError("semantic event encoding")
            cursor += 1
        decisions_total += decisions
        events_total += events
    return {"boundaries": frames, "decisions": decisions_total, "events": events_total,
            "records": count, "loss": [str(value) for value in loss]}


def compare(result, native):
    if result.get("format") != "LLH2":
        raise RuntimeError("unknown loop host report format")
    if result.get("cross_origin_isolated") is not False:
        raise RuntimeError("missing non-isolated browser evidence")
    if [report["mode"] for report in result["reports"]] != ["offline", "live"]:
        raise RuntimeError("both actual contexts are required")
    native_semantic = bytes(native["semanticBytes"])
    validate_semantic(native_semantic)
    for report in result["reports"]:
        report["semanticSummary"] = validate_semantic(report["semanticBytes"])
        if bytes(report["semanticBytes"]) != native_semantic:
            raise RuntimeError("complete semantic boundaries/decisions/events differ from native")
        expected_audio = [[sample[channel] for sample in native["samples"]] + [0] * (256 - len(native["samples"])) for channel in range(2)]
        if report["audio"] != expected_audio:
            raise RuntimeError(f"{report['mode']} Web Audio differs from native scalar oracle")
        data = bytes(report["midiBytes"])
        midi, cursor = [], 0
        while cursor < len(data):
            if cursor + 12 > len(data):
                raise RuntimeError("truncated MIDI journal header")
            at, length = struct.unpack_from("<QI", data, cursor)
            cursor += 12
            if not length or cursor + length > len(data):
                raise RuntimeError("truncated MIDI journal packet")
            midi.append([at, list(data[cursor:cursor + length])])
            cursor += length
        if midi != native["midi"]:
            raise RuntimeError("complete MIDI content/timing differs from native")
        transitions = []
        for response in report["responses"]:
            packet = bytes(response)
            if len(packet) != 96 or packet[:6] != b"LLR1\x01\x00":
                raise RuntimeError("invalid response journal")
            if packet[6] == 2:
                if struct.unpack_from("<I", packet, 28)[0] != 4 or packet[7] != 1 or struct.unpack_from("<I", packet, 72)[0] != 0:
                    raise RuntimeError("non-applied browser transition")
                requested, effective, delivered, terminal = struct.unpack_from("<QQQQ", packet, 40)
                if requested != effective:
                    raise RuntimeError("unexpected late scheduling")
                transitions.append([requested, delivered, terminal])
        if transitions != native["transitions"] or report["summary"]["recovered"] != native["recovered"]:
            raise RuntimeError("transition timing or ownership recovery differs")


def compare_graph_bindings(report, native):
    """Independent complete tables for the controlled public-binding graph script."""
    expected_audio = [[sample[channel] * (2 if 4 <= at < 12 else 1)
                       for at, sample in enumerate(native["samples"])] for channel in range(2)]
    expected_midi = b"".join(struct.pack("<QI", at, len(packet)) + bytes(packet)
                             for at, packet in native["midi"])
    validate_semantic(report["semanticBytes"])
    if (report["audio"] != expected_audio or bytes(report["midiBytes"]) != expected_midi
            or report["semanticBytes"] != native["semanticBytes"]
            or report["recovered"] != native["recovered"] or report["rounds"] != 128
            or report["generation"] != 130 or report["initialBytes"] != report["finalBytes"]):
        raise RuntimeError("graph bindings differ from independent native media/semantic/ownership oracle")
    validate_graph_receipts(report["graphResponses"], (1 << 64) - 1, 13)


def validate_graph_receipts(records, session, release_frame):
    packets = [bytes(packet) for packet in records]
    if len(packets) != 4 or any(len(packet) != 128 for packet in packets):
        raise RuntimeError("incomplete graph binding receipts")
    scope, slot, generation = struct.unpack_from("<III", packets[0], 76)
    if not scope or slot != 0 or generation != 1:
        raise RuntimeError("invalid local graph identity")
    for index, packet in enumerate(packets):
        expected = bytearray(128)
        phase, flags, sequence, status, runtime, requested, effective, delivered, observed, epoch, kind = (
            (1, 6, 1, 1, 4, 0, 0, 0, 0, 1, 1),
            (1, 2, 2, 3, 5, 4, 4, 0, 1, 0, 2),
            (2, 3, 2, 4, 5, 4, 4, 4, 4, 2, 2),
            (1, 6, 3, 2, 6, 0, 0, 0, release_frame, 0, 3),
        )[index]
        expected[:8] = b"LLS1\x01\x00" + bytes((phase, flags))
        for offset, value in ((8, session), (16, sequence), (32, runtime),
                              (40, requested), (48, effective), (56, delivered),
                              (64, observed), (88, epoch)):
            struct.pack_into("<Q", expected, offset, value)
        struct.pack_into("<I", expected, 28, status)
        struct.pack_into("<III", expected, 76, scope, slot, generation)
        if index in (0, 3):
            struct.pack_into("<IIIII", expected, 104, 0, 1 if index == 0 else 2,
                             *([4, 16, 4] if index == 0 else [3, 12, 2]))
        expected[124] = kind
        if packet != expected:
            raise RuntimeError("graph binding receipt differs from independent complete byte table")


def compare_graph_worklets(result, native):
    reports = result.get("graphReports", [])
    if [report["mode"] for report in reports] != ["offline", "live"]:
        raise RuntimeError("both actual graph contexts are required")
    validate_semantic(native["semanticBytes"])
    validate_graph_receipts(native["graphResponses"], (1 << 64) - 1, 129)
    expected_midi = b"".join(struct.pack("<QI", at, len(packet)) + bytes(packet) for at, packet in native["midi"])
    expected_audio = [[sample[channel] for sample in native["samples"]] for channel in range(2)]
    if (len(native["samples"]) != 129 or len(native["payloads"]) != 20
            or native["frames"] != 130 or native["callbacks"] != 3):
        raise RuntimeError("incomplete native graph reference")

    def normalized(packet, scope=None, release=False):
        data = bytearray(packet)
        data[8:16] = bytes(8)
        if scope is not None:
            data[scope:scope+4] = bytes(4)
        if release:
            data[64:72] = bytes(8)
        return bytes(data)

    def primitive(records):
        phases = {}
        for packet in records:
            if len(packet) != 96 or bytes(packet[:6]) != b"LLR1\x01\x00":
                raise RuntimeError("invalid graph-context primitive receipt")
            sequence = struct.unpack_from("<Q", bytes(packet), 16)[0]
            phases.setdefault(sequence, []).append(packet[6])
        if len(phases) != 3 or any(value != [1, 2] for value in phases.values()):
            raise RuntimeError("incomplete/out-of-order graph-context primitive outcomes")
        return sorted(normalized(packet) for packet in records)

    for index, report in enumerate(reports):
        summary = report["summary"]
        session = (1 << 64) - 3 - index
        if int(report["session"]) != session or report["audio"] != expected_audio or bytes(report["midiBytes"]) != expected_midi:
            raise RuntimeError("actual graph media differs from complete native reference")
        report["semanticSummary"] = validate_semantic(report["semanticBytes"])
        if report["semanticBytes"] != native["semanticBytes"]:
            raise RuntimeError("actual graph semantic bytes differ from native")
        if any(struct.unpack_from("<Q", bytes(packet), 8)[0] != session for packet in report["responses"]):
            raise RuntimeError("foreign graph-context primitive response")
        if primitive(report["responses"]) != primitive(native["responses"]):
            raise RuntimeError("actual graph primitive receipts differ from native")
        payloads, requests = report["payloads"], report["graphRequests"]
        if (len(payloads) != 21 or payloads[0]["result"] != 4 or any(p["result"] != 0 for p in payloads[1:])
                or any(p["frame"] != "0" for p in payloads)
                or len(requests) != 4 or requests[0]["result"] != 9 or any(p["result"] != 0 for p in requests[1:])
                or [p["frame"] for p in requests[:3]] != ["0", "0", "1"]):
            raise RuntimeError("missing transactional malformed/foreign graph ingress evidence")
        if any(struct.unpack_from("<Q", bytes(p["bytes"]), 8)[0] != session for p in payloads[1:] + requests[1:]):
            raise RuntimeError("foreign graph payload/request")
        for actual, expected in zip(payloads[1:], native["payloads"]):
            if normalized(actual["bytes"]) != normalized(expected):
                raise RuntimeError("actual graph descriptor differs from native bytes")
        bad_payload = bytearray(payloads[1]["bytes"]); bad_payload[7] ^= 128
        bad_request = bytearray(requests[1]["bytes"]); bad_request[8] ^= 128
        if bytes(payloads[0]["bytes"]) != bad_payload or bytes(requests[0]["bytes"]) != bad_request:
            raise RuntimeError("wrong actual graph rejection probes")
        release_at = int(requests[3]["frame"])
        if release_at < 129 or (release_at - 1) % 128:
            raise RuntimeError("graph release not on acknowledged callback clock")
        validate_graph_receipts(report["graphResponses"], session, release_at)
        staged_id = bytes(report["graphResponses"][0][76:88])
        if any(bytes(request["bytes"][64:76]) != staged_id for request in requests[2:]):
            raise RuntimeError("graph commands do not use the acknowledged Ready identity")
        for actual, expected in zip(requests[1:], native["graphRequests"]):
            if normalized(actual["bytes"], 64) != normalized(expected, 64):
                raise RuntimeError("actual graph command differs from native")
        for offset, (actual, expected) in enumerate(zip(report["graphResponses"], native["graphResponses"])):
            if normalized(actual, 76, offset == 3) != normalized(expected, 76, offset == 3):
                raise RuntimeError("actual graph receipt differs from native")
        starts = [int(frame) for frame in report["starts"]]
        if (len(starts) != 2 or any(frame < 0 or frame % 128 for frame in starts) or starts[1] < starts[0] + 128
                or report["checkpoint"] != {"type": "checkpoint", "frame": "1", "browser": str(starts[0])}
                or [summary["firstFrame"], summary["resumeFrame"]] != list(map(str, starts))):
            raise RuntimeError("actual graph output lacks matching segment clocks")
        expected_chunks = [[channel[0]] + [0] * 127 + channel[1:] for channel in expected_audio]
        if report["chunks"] != expected_chunks:
            raise RuntimeError("actual downstream graph samples or paused padding differ")
        frames, callbacks = int(summary["frames"]), int(summary["callbacks"])
        if (frames != 1 + (callbacks - 1) * 128 or frames < release_at + 128
                or summary["bytes"] != summary["finalBytes"] or summary["bytes"] != report["ready"]["bytes"]
                or summary["heap"] != [0,0,0] or summary["journalOverflow"] != 0
                or summary["returnedBuffers"] != 10 or summary["returnedGraphBuffers"] != 4
                or summary["retainedGraphs"] != 0 or summary["recovered"] != native["recovered"]
                or report["ready"]["shared"] is not False or report["ready"]["marker"] != 0 or summary["marker"] != 0
                or report["invalidReplies"] != 4 or report["duplicateReplies"] != 4
                or report["controlBytes"] != report["controlFinalBytes"] or report["controlMarker"] != 0x51a17
                or report["counts"] != [0,0,0,0,0] or report["returnedInputBuffers"] != [5,1,2]):
            raise RuntimeError("actual graph clock/heap/memory/ownership evidence incomplete")
        if report["mode"] == "offline":
            expected = [[0] * 768 for _ in range(2)]
            for channel in range(2):
                for segment, start in enumerate(starts):
                    expected[channel][start:start+128] = expected_chunks[channel][segment*128:(segment+1)*128]
            if report["offlineAudio"] != expected:
                raise RuntimeError("complete OfflineAudioContext graph output differs")
        elif report["offlineAudio"] is not None:
            raise RuntimeError("live graph report falsely claims OfflineAudioContext output")


def compare_staging_worklets(result, native):
    if (len(native["stagingRequests"]) != 56 or len(native["stagingResponses"]) != 82
            or len(native["responses"]) != 6 or len(native["samples"]) != 137
            or native["recordCount"] != 15 or native["programFrame"] != 6784
            or native["frames"] != 6793 or native["callbacks"] != 56
            or native["counts"] != [0]*4 or native["recovered"] != [256,16,16,0]):
        raise RuntimeError("incomplete native staging evidence")
    validate_semantic(native["semanticBytes"])
    reports=result.get("stagingReports", [])
    if [r["mode"] for r in reports] != ["offline","live"]:
        raise RuntimeError("both actual staging contexts required")
    for report in reports:
        value=json.loads(json.dumps(report["data"])); session=int(report["session"])
        for field,size in (("stagingRequests",128),("stagingResponses",256),("responses",96)):
            for packet in value[field]:
                if len(packet)!=size or struct.unpack_from("<Q",bytes(packet),8)[0]!=session:
                    raise RuntimeError("staging envelope/session")
                packet[8:16]=list(struct.pack("<Q",(1<<64)-1))
        if value!=native:
            raise RuntimeError("complete actual staging bytes/media/semantic/ownership differ")
        expected_clocks=list(range(128,6785,128))+[6785,6789,6793]
        cuts=report["checkpoints"]
        if ([int(c["frame"]) for c in cuts]!=expected_clocks
                or [c["frames"] for c in cuts]!=[128]*53+[1,4,4]):
            raise RuntimeError("staging acknowledged engine steps")
        browser_clocks = [int(c["browser"]) for c in cuts]
        if any(clock < 0 or clock % 128 for clock in browser_clocks) or any(a >= b for a, b in zip(browser_clocks, browser_clocks[1:])):
            raise RuntimeError("staging browser clock order/alignment")
        starts=list(map(int,report["starts"]))
        expected_segments=[{"browser":cuts[i]["browser"],"frame":str(frame),"frames":frames} for i,frame,frames in ((0,0,128),(53,6784,1),(54,6785,4),(55,6789,4))]
        if report["segments"]!=expected_segments or starts!=[int(s["browser"]) for s in expected_segments]:
            raise RuntimeError("staging actual segment clocks")
        ingress=report["ingress"]
        if (len(ingress)!=58 or [i["result"] for i in ingress]!=[2,9]+[0]*56
                or [int(i["frame"]) for i in ingress]!=[0,0]+list(range(0,6785,128))+[6785,6789]
                or [i["bytes"] for i in ingress[2:]]!=report["data"]["stagingRequests"]):
            raise RuntimeError("staging ingress/refusal/clock evidence")
        bad=ingress[2]["bytes"].copy();bad[0]^=1
        foreign=ingress[2]["bytes"].copy();foreign[8]^=128
        if ingress[0]["bytes"]!=bad or ingress[1]["bytes"]!=foreign:
            raise RuntimeError("staging malformed/foreign retries")
        chunks=[[0]*512 for _ in range(2)]
        offset=0
        for segment,count in enumerate((128,1,4,4)):
            for channel in range(2): chunks[channel][segment*128:segment*128+count]=[s[channel] for s in native["samples"][offset:offset+count]]
            offset+=count
        if report["chunks"]!=chunks:
            raise RuntimeError("staging actual short-call audio/tails")
        summary=report["summary"]
        if (summary["bytes"]!=summary["finalBytes"] or summary["bytes"]!=report["ready"]["bytes"]
                or report["controlBytes"]!=report["controlFinalBytes"] or summary["heap"]!=[0,0,0]
                or summary["returnedBuffers"]!=10 or summary["returnedStagingBuffers"]!=2
                or summary["retainedStaging"]!=0 or summary["journalOverflow"]!=0
                or summary["recovered"]!=native["recovered"] or summary["frames"]!="6793" or summary["callbacks"]!="56"
                or summary["firstFrame"]!=str(starts[0]) or summary["marker"]!=0 or report["controlMarker"]!=0x51a17
                or report["ready"]["shared"] is not False or report["ready"]["marker"] != 0):
            raise RuntimeError("staging memory/ownership/isolation evidence")
        if report["mode"]=="offline":
            expected=[[0]*14592 for _ in range(2)]
            for channel in range(2):
                for segment,start in enumerate(starts): expected[channel][start:start+128]=chunks[channel][segment*128:(segment+1)*128]
            if report["offlineAudio"]!=expected:
                raise RuntimeError("complete offline staging output differs")
        elif report["offlineAudio"] is not None:
            raise RuntimeError("live staging report claims offline output")


def compare_export_worklets(result, native):
    """Full immutable content/receipt comparison with acknowledged step clocks."""
    def normalized(data, session):
        value = json.loads(json.dumps(data))
        for field, size, offset in (("exportRequests", 96, 8), ("exportResponses", 1024, 16), ("responses", 96, 8)):
            for record in value[field]:
                if len(record) != size or struct.unpack_from("<Q", bytes(record), offset)[0] != session:
                    raise RuntimeError("immutable export envelope/session")
                record[offset:offset+8] = list(struct.pack("<Q", (1 << 64) - 1))
        return value
    if (len(native["exportRequests"]) != 44 or len(native["exportResponses"]) != 84
            or len(native["responses"]) != 12 or len(native["samples"]) != 256
            or native["counts"] != [0]*4 or native["recovered"] != [256,16,16,0]
            or native["frames"] != 5632 or native["callbacks"] != 44 or native["storage"] != [16384,64,96,1024,2,1]):
        raise RuntimeError("incomplete native immutable export evidence")
    semantic = validate_semantic(native["semanticBytes"])
    if semantic["boundaries"] != list(map(str, [0,4,8,12,128,132,136,140])) or semantic["decisions"] != 8 or semantic["events"] != 14:
        raise RuntimeError("immutable export musical boundaries")
    reports = result.get("exportReports", [])
    if [r["mode"] for r in reports] != ["offline", "live"]:
        raise RuntimeError("both actual immutable export contexts required")
    for report in reports:
        session = int(report["session"])
        if normalized(report["data"], session) != native:
            raise RuntimeError("complete actual immutable export samples/content/receipts differ from native")
        checkpoints = report["checkpoints"]
        if len(checkpoints) != 44 or [int(c["frame"]) for c in checkpoints] != [128*i for i in range(1,45)]:
            raise RuntimeError("immutable export acknowledged engine steps")
        starts = list(map(int, report["starts"]))
        clocks = [int(c["browser"]) for c in checkpoints]
        if starts != clocks[:2] or any(b <= a or b % 128 for a,b in zip(clocks,clocks[1:])):
            raise RuntimeError("immutable export downstream browser clocks")
        ingress = report["ingress"]
        if len(ingress) != 46 or [v["result"] for v in ingress] != [4,9]+[0]*44:
            raise RuntimeError("immutable export transactional refusals")
        expected_frames = [0,0,0,128]+list(range(256,5632,128))
        if list(map(lambda v: int(v["frame"]), ingress)) != expected_frames:
            raise RuntimeError("immutable export request acknowledgement clock")
        accepted = [v["bytes"] for v in ingress[2:]]
        if accepted != report["data"]["exportRequests"]:
            raise RuntimeError("immutable export accepted request trace")
        bad = accepted[0].copy(); bad[95] = 1
        foreign = accepted[0].copy(); foreign[8] ^= 128
        if ingress[0]["bytes"] != bad or ingress[1]["bytes"] != foreign:
            raise RuntimeError("immutable export refusal probes missing")
        summary = report["summary"]
        if (summary["bytes"] != summary["finalBytes"] or summary["bytes"] != report["ready"]["bytes"]
                or report["controlBytes"] != report["controlFinalBytes"] or summary["heap"] != [0,0,0]
                or summary["returnedBuffers"] != 10 or summary["returnedExportBuffers"] != 2
                or summary["retainedExports"] != 0 or summary["journalOverflow"] != 0
                or summary["recovered"] != native["recovered"] or summary["frames"] != "5632" or summary["callbacks"] != "44"
                or [summary["firstFrame"],summary["resumeFrame"]] != list(map(str,starts))
                or report["controlMarker"] != 0x51a17 or summary["marker"] != 0 or report["ready"]["shared"] is not False):
            raise RuntimeError("immutable export memory/ownership/isolation evidence")
        if report["mode"] == "offline":
            expected = [[0]*11520 for _ in range(2)]
            for channel in range(2):
                for segment,start in enumerate(starts):
                    expected[channel][start:start+128] = [sample[channel] for sample in native["samples"][segment*128:(segment+1)*128]]
            if report["offlineAudio"] != expected:
                raise RuntimeError("complete OfflineAudioContext immutable export output differs")
        elif report["offlineAudio"] is not None:
            raise RuntimeError("live export report claims offline audio")


def compare_dense_worklets(result, native, *, stalled=False):
    reports = result.get("stallReports" if stalled else "denseReports")
    frames = 136 if stalled else 131
    steps = [128,1,2,5] if stalled else [128,1,2]
    ends = ["128","129","131","136"] if stalled else ["128","129","131"]
    offline_frames = 1280 if stalled else 1024
    if not isinstance(reports, list) or [r.get("mode") for r in reports] != ["offline", "live"]:
        raise RuntimeError("both actual dense contexts required")
    if (len(native["samples"]) != frames or len(native["requests"]) != (7 if stalled else 5) or len(native["responses"]) != (14 if stalled else 10)
            or len(native["midiBytes"]) != (130695 if stalled else 126735) or native["counts"] != [0]*4
            or native["recovered"] != [256,16,16,0]):
        raise RuntimeError("incomplete native dense reference")
    semantics = validate_semantic(native["semanticBytes"], capacity=4096)
    if semantics["boundaries"] != list(map(str,range(frames))) or semantics["records"] != (2042 if stalled else 1968):
        raise RuntimeError("incomplete dense semantic boundaries")
    for report in reports:
        data = json.loads(json.dumps(report["data"]))
        session = int(report["session"])
        for field, size in (("requests",64),("responses",96)):
            for packet in data[field]:
                if len(packet) != size or int.from_bytes(bytes(packet[8:16]),"little") != session:
                    raise RuntimeError("dense wire session/size")
                packet[8:16] = [255]*8
        if data != native:
            raise RuntimeError("complete dense media/receipts/semantics differ from native")
        cuts = report["checkpoints"]
        if len(cuts) != len(steps) or [c["frame"] for c in cuts] != ends or [c["frames"] for c in cuts] != steps:
            raise RuntimeError("dense engine step clocks")
        clocks = [int(c["browser"]) for c in cuts]
        if (report["starts"] != [c["browser"] for c in cuts] or clocks[0] < 0
                or any(c % 128 for c in clocks) or any(b <= a for a,b in zip(clocks,clocks[1:]))):
            raise RuntimeError("dense downstream step clocks")
        states = [{"type":"dense-state","frame":"129","outstanding":4,"stop":15},
                  {"type":"dense-state","frame":"131","outstanding":5,"stop":15}]
        if stalled:
            states.append({"type":"dense-state","frame":"136","outstanding":2,"stop":15})
        if report["states"] != states:
            raise RuntimeError("dense stalled credits/recovery evidence")
        stall = {"frame":"136","outstanding":2,"availableBuffers":0,"received":10,"clientOutstanding":2,"heldBuffers":10} if stalled else None
        if report.get("transferStall") != stall:
            raise RuntimeError("actual transferred-buffer exhaustion evidence")
        expected = [[],[]]
        offset = 0
        for count in steps:
            for channel in range(2):
                expected[channel].extend([s[channel] for s in native["samples"][offset:offset+count]] + [0]*(128-count))
            offset += count
        if report["chunks"] != expected:
            raise RuntimeError("dense actual stereo/short tails")
        summary = report["summary"]
        if (summary["bytes"] != summary["finalBytes"] or summary["bytes"] != report["ready"]["bytes"]
                or report["controlBytes"] != report["controlFinalBytes"] or summary["heap"] != [0,0,0]
                or summary["frames"] != str(frames) or summary["callbacks"] != str(len(steps)) or summary["returnedBuffers"] != 10
                or summary["returnedGraphBuffers"] != 0 or summary["returnedExportBuffers"] != 0
                or summary["returnedStagingBuffers"] != 0 or summary["retainedGraphs"] != 0
                or summary["retainedExports"] != 0 or summary["retainedStaging"] != 0
                or summary["journalOverflow"] != 0 or summary["recovered"] != [256,16,16,0]
                or summary["firstFrame"] != str(clocks[0]) or summary["resumeFrame"] is not None
                or report["controlMarker"] != 0x51a17 or summary["marker"] != 0
                or report["ready"]["marker"] != 0 or report["ready"]["shared"] is not False):
            raise RuntimeError("dense fixed memory/isolation/ownership evidence")
        if report["mode"] == "offline":
            output = [[0]*offline_frames for _ in range(2)]
            for channel in range(2):
                for segment, clock in enumerate(clocks):
                    if clock + 128 > offline_frames:
                        raise RuntimeError("dense offline clock bounds")
                    output[channel][clock:clock+128] = expected[channel][segment*128:(segment+1)*128]
            if report["offlineAudio"] != output:
                raise RuntimeError("complete dense offline output differs")
        elif report["offlineAudio"] is not None:
            raise RuntimeError("dense live report claims offline output")


def compare_media_worklets(result, native):
    try:
        _compare_media_worklets(result, native)
    except (KeyError, TypeError, ValueError, IndexError, OverflowError) as error:
        raise RuntimeError("malformed media host evidence") from error


def _compare_media_worklets(result, native):
    reports = result.get("mediaReports")
    steps = [1, 6, 57] + [1] * 27
    if not isinstance(reports, list) or [r.get("mode") for r in reports] != ["offline", "live"]:
        raise RuntimeError("both actual media contexts required")
    sizes = (("requests", 64, 8, 2), ("responses", 96, 8, 4),
             ("stagingRequests", 128, 8, 4), ("stagingResponses", 256, 8, 8),
             ("exportRequests", 96, 8, 26), ("exportResponses", 1024, 16, 46))
    if (len(native["samples"]) != 91 or len(native["midiBytes"]) != 4110
            or native["counts"] != [0]*8 or native["recovered"] != [42,16,16,0]
            or native["storage"] != [2688,64,96,1024,2,1]
            or (native["frames"], native["callbacks"]) != (91,30)
            or any(len(native[field]) != count for field, _, _, count in sizes)):
        raise RuntimeError("incomplete native media reference")
    semantics = validate_semantic(native["semanticBytes"], capacity=4096)
    if semantics["records"] != 552 or semantics["boundaries"] != list(map(str,range(91))):
        raise RuntimeError("incomplete media semantic boundaries")
    for report in reports:
        data = json.loads(json.dumps(report["data"]))
        session = int(report["session"])
        for field, size, offset, count in sizes:
            if len(data[field]) != count:
                raise RuntimeError("incomplete media wire results")
            for packet in data[field]:
                if len(packet) != size or int.from_bytes(bytes(packet[offset:offset+8]),"little") != session:
                    raise RuntimeError("media wire size/session")
                packet[offset:offset+8] = [255]*8
        if data != native:
            raise RuntimeError("complete media content/metadata/receipts/semantics differ from native")
        cuts = report["checkpoints"]
        ends, frame = [], 0
        for count in steps:
            frame += count
            ends.append(str(frame))
        if [c["frames"] for c in cuts] != steps or [c["frame"] for c in cuts] != ends:
            raise RuntimeError("media acknowledged engine step clocks")
        clocks = [int(c["browser"]) for c in cuts]
        if (report["starts"] != [c["browser"] for c in cuts] or clocks[0] < 0
                or any(c % 128 for c in clocks) or any(b <= a for a,b in zip(clocks,clocks[1:]))):
            raise RuntimeError("media downstream clocks")
        expected, frame = [[],[]], 0
        for count in steps:
            for channel in range(2):
                expected[channel].extend([s[channel] for s in native["samples"][frame:frame+count]] + [0]*(128-count))
            frame += count
        if report["chunks"] != expected:
            raise RuntimeError("media downstream samples/short-call tails")
        ingress = report["ingress"]
        staged = [i for i in ingress if i["kind"] == "staging"]
        exported = [i for i in ingress if i["kind"] == "export"]
        if (len(ingress) != 34 or len(staged) != 8 or len(exported) != 26
                or [i["result"] for i in staged] != [2,9,0,7,7,0,0,0]
                or [i["frame"] for i in staged] != ["0"]*3+["1"]*3+["7","64"]
                or [i["bytes"] for i in staged if i["result"] == 0] != report["data"]["stagingRequests"]
                or [i["result"] for i in exported] != [0]*26
                or [i["frame"] for i in exported] != list(map(str,range(65,91)))
                or [i["bytes"] for i in exported] != report["data"]["exportRequests"]):
            raise RuntimeError("media ingress/refusal/owning retry evidence")
        bad, foreign = list(staged[2]["bytes"]), list(staged[2]["bytes"])
        bad[0] ^= 1
        foreign[8] ^= 128
        if staged[0]["bytes"] != bad or staged[1]["bytes"] != foreign:
            raise RuntimeError("media malformed/foreign retry")
        for probe, index in zip(staged[3:5], [1 << 32, (1 << 64)-1]):
            alias = list(staged[5]["bytes"])
            alias[48:56] = index.to_bytes(8,"little")
            if probe["bytes"] != alias:
                raise RuntimeError("media catalog Wasm32 alias rejection")
        summary = report["summary"]
        if (summary["bytes"] != summary["finalBytes"] or summary["bytes"] != report["ready"]["bytes"]
                or report["controlBytes"] != report["controlFinalBytes"] or summary["heap"] != [0,0,0]
                or summary["frames"] != "91" or summary["callbacks"] != "30"
                or summary["returnedBuffers"] != 10 or summary["returnedGraphBuffers"] != 0
                or summary["returnedExportBuffers"] != 2 or summary["returnedStagingBuffers"] != 2
                or [summary[k] for k in ("retainedGraphs","retainedExports","retainedStaging","journalOverflow")] != [0]*4
                or summary["recovered"] != [42,16,16,0] or report["ready"]["exportStorage"] != native["storage"]
                or summary["firstFrame"] != str(clocks[0]) or summary["resumeFrame"] is not None
                or report["controlMarker"] != 0x51a17 or summary["marker"] != 0
                or report["ready"]["marker"] != 0 or report["ready"]["shared"] is not False):
            raise RuntimeError("media memory/isolation/complete ownership evidence")
        if report["mode"] == "offline":
            output = [[0]*7936 for _ in range(2)]
            for channel in range(2):
                for segment, clock in enumerate(clocks):
                    if clock+128 > 7936:
                        raise RuntimeError("media offline clock bounds")
                    output[channel][clock:clock+128] = expected[channel][segment*128:(segment+1)*128]
            if report["offlineAudio"] != output:
                raise RuntimeError("complete media offline output differs")
        elif report["offlineAudio"] is not None:
            raise RuntimeError("media live report claims offline output")


def compare_planner_worklets(result, native):
    try:
        _compare_planner_worklets(result, native)
    except (KeyError, TypeError, ValueError, IndexError, OverflowError, AttributeError) as error:
        raise RuntimeError("malformed planner host evidence") from error


def _compare_planner_worklets(result, native):
    reports = result.get("plannerReports")
    if not isinstance(reports, list) or [r.get("mode") for r in reports] != ["offline", "live"]:
        raise RuntimeError("both actual planner control contexts required")
    semantic = validate_semantic(native["semanticBytes"], capacity=512)
    if (native["samples"] != [[v, v*10] for v in range(1,129)]
            or semantic["records"] != 512 or semantic["boundaries"] != list(map(str,range(128)))
            or native["recovered"] != [8,4,4]
            or [native[k] for k in ("frames","callbacks","plans","finishes")] != [128,1,128,128]):
        raise RuntimeError("incomplete independent native planner control")
    audio = [[sample[c] for sample in native["samples"]]+[0]*128 for c in range(2)]
    for report in reports:
        summary, ready = report["summary"], report["ready"]
        if (report["data"] != native or report["audio"] != audio
                or summary["frame"] != "128" or summary["callbacks"] != "1"
                or summary["plans"] != 128 or summary["finishes"] != 128
                or summary["heap"] != [0,0,0] or summary["recovered"] != [8,4,4]
                or summary["bytes"] != summary["finalBytes"] or summary["bytes"] != ready["bytes"]
                or report["controlBytes"] != report["controlFinalBytes"]
                or report["controlMarker"] != 0x51a17 or summary["marker"] != 0
                or ready["marker"] != 0 or ready["shared"] is not False):
            raise RuntimeError("planner control media/semantics/lifecycle/memory/recovery differs")
        start = int(report["start"])
        if start < 0 or start % 128:
            raise RuntimeError("planner downstream clock")
        if report["mode"] == "offline":
            if start+256 > 512 or report["offlineAudio"] != [[0]*start+c+[0]*(512-start-256) for c in audio]:
                raise RuntimeError("complete planner offline output differs")
        elif report["offlineAudio"] is not None:
            raise RuntimeError("planner live report claims offline output")


def compare_native_graphs(native, threaded):
    """Complete direct/threaded replay equality except instance-local graph scope."""
    def normalized(source):
        value = json.loads(json.dumps(source))
        if len(value["graphRequests"]) != 3 or len(value["graphResponses"]) != 4:
            raise RuntimeError("incomplete native graph transport report")
        scope = bytes(value["graphResponses"][0])[76:80]
        if len(scope) != 4 or scope == bytes(4):
            raise RuntimeError("native graph Ready scope missing")
        for request in value["graphRequests"][1:]:
            if len(request) != 96 or bytes(request[64:68]) != scope:
                raise RuntimeError("native graph request does not use Ready identity")
            request[64:68] = [0] * 4
        for response in value["graphResponses"]:
            if len(response) != 128 or bytes(response[76:80]) != scope:
                raise RuntimeError("native graph result identity mismatch")
            response[76:80] = [0] * 4
        return value
    if normalized(native) != normalized(threaded):
        raise RuntimeError("complete two-thread graph replay differs from direct native")


def verify_failure(result, exit_code, kind):
    base_kind = kind.removeprefix("graph-")
    if kind not in ("processor", "rust-heap", "graph-processor", "graph-rust-heap", "planner", "planner-rust-heap"):
        raise RuntimeError("unknown failure canary")
    planned = base_kind.startswith("planner")
    error = base_kind in ("processor", "planner")
    label = "planner" if base_kind == "planner" else "planner Rust heap" if planned else "processor" if error else "Rust heap"
    diagnostic = "intentional loop " + label + " failure canary detected"
    witness = result.get("workletFailure") or {}
    heap = witness.get("heap", [])
    expected_code = 10 if error else 0xffff0001
    expected_origin = (3 if error else 4) if planned else (1 if error else 2)
    if (exit_code == 0 or result.get("passed") is not False or diagnostic not in result.get("failure", "")
            or type(witness.get("code")) is not int or witness.get("code") != expected_code
            or type(witness.get("planner" if planned else "processor")) is not int
            or witness.get("planner" if planned else "processor") != expected_origin
            or (planned and (type(witness.get("processor")) is not int or witness.get("processor") != 0
                or type(witness.get("plans")) is not int or type(witness.get("finishes")) is not int
                or witness.get("plans") != (1 if error else 128) or witness.get("finishes") != (0 if error else 128)
                or witness.get("frame") != "128" or witness.get("callbacks") != "1"
                or type(witness.get("bytes")) is not int or witness.get("bytes", 0) <= 0
                or witness.get("finalBytes") != witness.get("bytes")))
            or len(heap) != 3 or any(type(count) is not int for count in heap)
            or (heap != [0, 0, 0] if error else any(count <= 0 for count in heap))):
        raise RuntimeError(f"{kind} did not fail with its specific Rust-origin witnesses")
    return {"kind": kind, "exit_code": exit_code, "diagnostic": diagnostic, "witness": witness}


def native_replays(executable, reports, profile):
    primitive = json.loads(harness.command([str(executable), "--json"], reports / "native.json"))
    results = {"primitive": primitive}
    for name, option in (("graph", "graph"), ("exports", "export"), ("staging", "staging"),
                         ("dense", "dense"), ("stalls", "stall"), ("media", "media")):
        direct = json.loads(harness.command([str(executable), f"--{option}-json"], reports / f"native-{name}.json"))
        threaded = json.loads(harness.command([str(executable), f"--{option}-two-thread-json"],
                                             reports / f"native-{name}-two-thread.json"))
        if name == "graph":
            compare_native_graphs(direct, threaded)
        elif direct != threaded:
            raise RuntimeError(f"complete native/threaded {name} replay reports differ")
        results[name] = direct
    results["planner"] = json.loads(harness.command([str(executable), "--planner-json"], reports / "native-planner.json"))
    for mode in ("--direct", "--wire", "--two-thread"):
        harness.command([str(executable), mode], reports / (mode[2:] + ".log"))
    return {"passed": True, "kind": "native-replay", "profile": profile, "native": results,
            "source_head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
            "source_dirty": bool(subprocess.check_output(["git", "status", "--porcelain", "--", str(FIXTURE),
                                                         str(Path(__file__).resolve())], cwd=ROOT, text=True).strip()),
            "limitations": ["Native replay only; no fresh Node, Chromium, AudioWorklet or memory-growth evidence."]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--no-build", action="store_true", help="Reuse existing native/Wasm artifacts")
    parser.add_argument("--profile", choices=("debug", "release"), default="release")
    parser.add_argument("--failure-canary", choices=("processor", "rust-heap", "graph-processor", "graph-rust-heap", "planner", "planner-rust-heap"), help="Must fail through the corresponding real worklet path")
    parser.add_argument("--verify-failures", action="store_true")
    parser.add_argument("--native-only", action="store_true", help="Native reference/actor replay only; no browser substitution")
    args = parser.parse_args()
    if args.native_only and (args.verify_failures or args.failure_canary):
        parser.error("native replay cannot satisfy actual worklet failure gates")
    reports = ASSETS / "reports" / ("native-" + args.profile if args.native_only else args.profile)
    if args.failure_canary:
        reports = reports / ("failure-" + args.failure_canary)
    reports.mkdir(parents=True, exist_ok=True)
    assets = ASSETS / args.profile
    assets.mkdir(parents=True, exist_ok=True)
    harness.ASSETS, harness.REPORTS = assets, reports
    for name in ("result.json", "browser.json", "browser-console.json"):
        (reports / name).unlink(missing_ok=True)
    manifest = str(FIXTURE / "Cargo.toml")
    profile = ["--release"] if args.profile == "release" else []
    if not args.no_build:
        harness.command(["cargo", "build", "--locked", "--manifest-path", manifest, *profile], reports / "native-build.log")
    if args.native_only:
        executable = FIXTURE / "target" / args.profile / ("replay.exe" if os.name == "nt" else "replay")
        result = native_replays(executable, reports, args.profile)
        (reports / "result.json").write_text(json.dumps(result, indent=2))
        print(json.dumps({"passed": True, "kind": "native-replay", "profile": args.profile, "reports": str(reports)}, indent=2))
        return
    if not args.no_build:
        harness.command(["wasm-pack", "build", str(FIXTURE), "--target", "web", "--release" if args.profile == "release" else "--dev",
                         "--out-dir", str(assets / "pkg"), "--out-name", "fixture", "--no-opt", "--", "--locked"], reports / "wasm-build.log")
    for source in (FIXTURE / "web").iterdir():
        if source.suffix in (".js", ".mjs"):
            shutil.copy2(source, assets / source.name)
    (assets / "package.json").write_text('{"type":"module"}\n')
    shutil.copy2(ROOT / "tests/rt_runtime_host/web/text_decoder.js", assets / "text_decoder.js")
    (assets / "index.html").write_text("<!doctype html><meta charset=utf-8><title>Independent loop host</title>\n")
    node = None
    if not args.failure_canary:
        node = json.loads(harness.command(["node", str(assets / "node_checks.mjs")], reports / "node-checks.json"))
        if not node.get("passed") or len(node.get("checks", [])) != 20:
            raise RuntimeError("incomplete Node binding checks")
    with harness.server() as url:
        with urllib.request.urlopen(url + "/index.html") as response:
            if response.headers.get("Cross-Origin-Opener-Policy") or response.headers.get("Cross-Origin-Embedder-Policy"):
                raise RuntimeError("unexpected isolation headers")
        result = harness.browser(url, args.failure_canary)
    executable = FIXTURE / "target" / args.profile / ("replay.exe" if os.name == "nt" else "replay")
    native = json.loads(harness.command([str(executable), "--json"], reports / "native.json"))
    compare(result, native)
    graph_native = json.loads(harness.command([str(executable), "--graph-json"], reports / "native-graph.json"))
    graph_threaded = json.loads(harness.command([str(executable), "--graph-two-thread-json"], reports / "native-graph-two-thread.json"))
    compare_native_graphs(graph_native, graph_threaded)
    compare_graph_worklets(result, graph_native)
    result["native_graph_two_thread"] = {"passed": True, "channel_capacity": 1,
                                          "frames": graph_threaded["frames"], "callbacks": graph_threaded["callbacks"]}
    if result.get("bindingChecks") != {key: node[key] for key in ("passed", "checks", "graph", "exports", "staging", "dense", "stalls", "media", "planner")}:
        raise RuntimeError("Chromium binding checks differ from Node")
    compare_graph_bindings(result["bindingChecks"]["graph"], native)
    exported = json.loads(harness.command([str(executable), "--export-json"], reports / "native-export.json"))
    threaded_exports = json.loads(harness.command([str(executable), "--export-two-thread-json"], reports / "native-export-two-thread.json"))
    if exported != threaded_exports or result["bindingChecks"]["exports"] != exported:
        raise RuntimeError("complete native/threaded/Node/Chromium immutable export reports differ")
    compare_export_worklets(result, exported)
    staged = json.loads(harness.command([str(executable), "--staging-json"], reports / "native-staging.json"))
    staged_threaded = json.loads(harness.command([str(executable), "--staging-two-thread-json"], reports / "native-staging-two-thread.json"))
    if staged != staged_threaded or result["bindingChecks"]["staging"] != staged:
        raise RuntimeError("complete native/threaded/Node/Chromium staging reports differ")
    compare_staging_worklets(result, staged)
    dense = json.loads(harness.command([str(executable), "--dense-json"], reports / "native-dense.json"))
    dense_threaded = json.loads(harness.command([str(executable), "--dense-two-thread-json"], reports / "native-dense-two-thread.json"))
    if dense != dense_threaded or result["bindingChecks"]["dense"] != dense:
        raise RuntimeError("complete native/threaded/Node/Chromium dense reports differ")
    compare_dense_worklets(result, dense)
    stalls = json.loads(harness.command([str(executable), "--stall-json"], reports / "native-stalls.json"))
    stalls_threaded = json.loads(harness.command([str(executable), "--stall-two-thread-json"], reports / "native-stalls-two-thread.json"))
    if stalls != stalls_threaded or result["bindingChecks"]["stalls"] != stalls:
        raise RuntimeError("complete native/threaded/Node/Chromium restart reports differ")
    compare_dense_worklets(result, stalls, stalled=True)
    media = json.loads(harness.command([str(executable), "--media-json"], reports / "native-media.json"))
    media_threaded = json.loads(harness.command([str(executable), "--media-two-thread-json"], reports / "native-media-two-thread.json"))
    if media != media_threaded or result["bindingChecks"]["media"] != media:
        raise RuntimeError("complete native/threaded/Node/Chromium media reports differ")
    compare_media_worklets(result, media)
    planner = json.loads(harness.command([str(executable), "--planner-json"], reports / "native-planner.json"))
    if result["bindingChecks"]["planner"] != planner:
        raise RuntimeError("complete native/Node/Chromium planner control differs")
    compare_planner_worklets(result, planner)
    result["node"] = node
    for mode in ("--direct", "--wire", "--two-thread"):
        harness.command([str(executable), mode], reports / (mode[2:] + ".log"))
    result["profile"] = args.profile
    result["wasm_sha256"] = hashlib.sha256((assets / "pkg/fixture_bg.wasm").read_bytes()).hexdigest()
    result["source_head"] = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    result["source_dirty"] = bool(subprocess.check_output(["git", "status", "--porcelain", "--", str(FIXTURE), str(Path(__file__).resolve())], cwd=ROOT, text=True).strip())
    if args.verify_failures:
        result["failure_canaries"] = []
        for kind in ("processor", "rust-heap", "graph-processor", "graph-rust-heap", "planner", "planner-rust-heap"):
            failed = subprocess.run([sys.executable, str(Path(__file__).resolve()), "--no-build", "--profile", args.profile, "--failure-canary", kind],
                                    cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=360)
            (reports / f"{kind}-failure-gate.log").write_text(failed.stdout)
            failure_report = reports / ("failure-" + kind) / "browser.json"
            if not failure_report.exists():
                raise RuntimeError(f"{kind} produced no browser failure evidence:\n{failed.stdout[-12000:]}")
            result["failure_canaries"].append(verify_failure(json.loads(failure_report.read_text()), failed.returncode, kind))
    (reports / "result.json").write_text(json.dumps(result, indent=2))
    print(json.dumps({"passed": True, "profile": args.profile, "contexts": [r["mode"] for r in result["reports"]], "reports": str(reports)}, indent=2))


if __name__ == "__main__":
    main()
