"""Complete state/job/media quanta compared with Rust, including FIFO failure effects."""
from copy import deepcopy
import json
import os
from pathlib import Path
import random
import cocotb
import fpga_testbench as wire
from pipeline_driver import PipelineDriver

CAPACITIES = tuple(map(int, os.environ["AUDIO_CAPACITIES"].split(",")))
MEDIA = tuple(map(int, os.environ["MEDIA_CAPACITIES"].split(",")))
SEED = int(os.environ["MEDIA_SEED"])
OUTPUT = Path(os.environ["MEDIA_OUTPUT"])
REFERENCE = Path(os.environ["MEDIA_REFERENCE"])
ROOT = Path(__file__).resolve().parents[3]


def document(kind, value):
    return {"format_version": 1, "payload": {"kind": kind, "value": value}}


def record_case(frames, seed=0):
    rng = random.Random(seed)
    graph = {"quantum": frames, "nodes": [
        {"kind": "input", "port": 0}, {"kind": "gain", "input": 0, "gain": 8388608},
        {"kind": "capture", "input": 1, "view": 0, "recording": True},
        {"kind": "playback", "view": 0, "length": frames, "position": 0, "playing": False},
        {"kind": "output", "input": 3, "port": 0}]}
    compiled = wire.run_reference("compile", document("graph", graph), executable=REFERENCE)["payload"]["value"]
    capture = compiled["bindings"][2]["value"]["enabled"]
    playback = compiled["bindings"][3]["value"]
    def writes(record, play, position=0):
        return [{"address": address, "value": {"kind": "word", "value": value}}
                for address, value in ((capture, record), (playback["enabled"], play), (playback["position"], position))]
    return {"name": f"record-gain-play-reuse-{frames}-{seed}", "program": compiled["program"],
            "initial_state": {"memory": compiled["initial_state"]}, "queue_capacity": CAPACITIES[2],
            "buffers": 5, "output_ports": 2,
            "media": {"chunk_frames": MEDIA[2], "chunk_limit": MEDIA[1], "samples": [],
                      "free_chunks": [], "views": [{"chunks": [], "frames": 0}]},
            "steps": [
                {"writes": [], "inputs": [[rng.randint(-(1 << 31), (1 << 31)-1) for _ in range(frames)]]},
                {"writes": writes(0, 1, frames-1), "inputs": [[0]*frames]},
                {"writes": writes(1, 0), "inputs": [[-3]*frames], "media_actions": [{"kind": "clear", "view": 0}]},
                {"writes": writes(0, 1), "inputs": [[0]*frames]},
            ]}


def session_case(seed):
    rng = random.Random(seed)
    frames, tracks, loops, length = 32, 8, 8, 96
    nodes, views, samples = [], [], []
    for track in range(tracks):
        mixed = None
        for loop in range(loops):
            view = track * loops + loop
            nodes.append({"kind": "playback", "view": view, "length": length,
                          "position": (view * 17) % length, "playing": True})
            nodes.append({"kind": "gain", "input": len(nodes)-1, "gain": 8388608})
            gained = len(nodes)-1
            if mixed is not None:
                nodes.append({"kind": "mix", "left": mixed, "right": gained})
                mixed = len(nodes)-1
            else:
                mixed = gained
            start = len(samples) // MEDIA[2]
            samples.extend(rng.randint(-(1 << 24), (1 << 24)-1) for _ in range(length))
            views.append({"chunks": list(range(start, start + length // MEDIA[2])), "frames": length})
        nodes.append({"kind": "output", "input": mixed, "port": track})
    compiled = wire.run_reference("compile", document("graph", {"quantum": frames, "nodes": nodes}),
                                  executable=REFERENCE)["payload"]["value"]
    return {"name": "8-tracks-8-playing-loops-q32", "program": compiled["program"],
            "initial_state": {"memory": compiled["initial_state"]}, "queue_capacity": CAPACITIES[2],
            "buffers": len(nodes), "output_ports": tracks,
            "media": {"chunk_frames": MEDIA[2], "chunk_limit": MEDIA[1], "samples": samples,
                      "free_chunks": [], "views": views},
            "steps": [{"writes": [], "inputs": []} for _ in range(4)]}


async def compare(dut, case, seed, *, stalls=True):
    case = deepcopy(case)
    case["queue_capacity"] = CAPACITIES[2]
    trace = wire.run_reference("trace", document("audio_case", case), executable=REFERENCE)
    driver = PipelineDriver(dut, CAPACITIES, MEDIA, seed)
    observed, timing = [], []
    try:
        await driver.load(case)
        for step, expected in zip(case["steps"], trace["payload"]["value"]["results"]):
            actual = await driver.quantum_audio(step, stalls=stalls)
            observed.append(actual)
            timing.append(driver.timing)
            wire.assert_audio_result_matches(expected, actual)
    except BaseException as error:
        wire.dump(document("audio_case", case), OUTPUT / "failure.case.json")
        wire.dump(trace, OUTPUT / "failure.expected.trace.json")
        (OUTPUT / "failure.observed.json").write_text(json.dumps({"results": observed, "error": str(error)}, indent=2)+"\n")
        dut._log.error("failed pipeline case %s", case["name"])
        raise
    return timing


@cocotb.test()
async def directed_pipeline(dut):
    replay = os.environ.get("MEDIA_CASE")
    if replay:
        payload = wire.load(replay)["payload"]
        await compare(dut, payload["value"]["case"] if payload["kind"] == "audio_trace" else payload["value"], SEED)
        return
    cases = [record_case(n) for n in sorted({1, 3, CAPACITIES[3]})]
    if MEDIA[2] == 4:
        cases.append(wire.load(ROOT / "scripts/fpga_testbench/examples/shared_ranges.json")["payload"]["value"])
    else:
        saved = wire.load(ROOT / "scripts/fpga_testbench/examples/shared_ranges.json")["payload"]["value"]
        cf = MEDIA[2]
        saved["program"]["quantum"] = cf
        saved["media"]["chunk_frames"] = cf
        saved["media"]["samples"] = list(range(1, 3*cf+1))
        saved["media"]["views"][0]["frames"] = 3*cf
        playback = saved["program"]["instructions"][2]
        saved["initial_state"]["memory"][playback["length"]]["value"] = cf+1
        saved["steps"][0]["media_actions"][0].update(start=cf-1, frames=cf+1)
        saved["steps"][0]["inputs"] = [[0]*cf]
        saved["steps"][1]["inputs"] = [[-(1 << 31), (1 << 31)-1, -3, 3]*(cf//4)]
        for write in saved["steps"][1]["writes"]:
            if write["address"] == playback["position"]:
                write["value"]["value"] = cf
        saved["steps"][2]["inputs"] = [list(range(100, 100+cf))]
        saved["steps"][3]["inputs"] = [list(range(100+cf, 100+2*cf))]
        cases.append(saved)
    invalid = record_case(3)
    invalid["media"]["chunk_limit"] = 0
    invalid["name"] = "capture-capacity-failure"
    cases.append(invalid)
    readonly = record_case(3)
    readonly["media"]["views"][0]["range"] = {"offset": 0}
    readonly["name"] = "capture-readonly-failure"
    cases.append(readonly)
    missing = record_case(3)
    missing["media"]["views"] = []
    missing["name"] = "capture-missing-view-failure"
    cases.append(missing)
    control = record_case(3)
    control["name"] = "slice-control-failure"
    control["steps"][0]["media_actions"] = [{"kind": "slice", "source": 0, "start": 1, "frames": 1}]
    cases.append(control)
    playback_failure = record_case(3)
    playback_failure["name"] = "read-length-failure"
    playback = playback_failure["program"]["instructions"][3]
    playback_failure["steps"][1]["writes"].append({"address": playback["length"], "value": {"kind": "word", "value": 4}})
    cases.append(playback_failure)
    timing = {}
    for index, value in enumerate(cases):
        timing[value["name"]] = await compare(dut, value, SEED+index, stalls=index >= 3)
    (OUTPUT / "directed-summary.json").write_text(json.dumps(
        {"cases": len(cases), "quanta": sum(map(len, timing.values())), "timing": timing}, indent=2)+"\n")


@cocotb.test()
async def randomized_pipeline(dut):
    count = int(os.environ["MEDIA_RANDOM_CASES"])
    for index in range(count):
        frames = (1, 3, CAPACITIES[3])[index % 3]
        await compare(dut, record_case(frames, SEED+index), SEED+index+1000)
    (OUTPUT / "random-summary.json").write_text(json.dumps({"cases": count, "quanta": 4*count}, indent=2)+"\n")


@cocotb.test()
async def memory_timing(dut):
    session = os.environ["MEDIA_CONFIG"] == "8x8"
    case = session_case(SEED) if session else record_case(CAPACITIES[3], SEED)
    labels = [f"playback_{index+1}" for index in range(len(case["steps"]))] if session else [
        "fresh_record", "playback", "reused_record", "playback_again"]
    wire.dump(document("audio_case", case), OUTPUT / "memory-timing.case.json")
    trace = wire.run_reference("trace", document("audio_case", case), executable=REFERENCE)
    driver = PipelineDriver(dut, CAPACITIES, MEDIA, SEED)
    await driver.load(case)
    response_cycles = int(os.environ["MEDIA_RESPONSE_CYCLES"])
    results = []
    for label, step, expected in zip(labels, case["steps"], trace["payload"]["value"]["results"]):
        reads, writes = driver.media.read_requests, driver.media.write_requests
        responses = len(driver.media.response_latencies)
        actual = await driver.quantum_audio(step, stalls=False)
        wire.assert_audio_result_matches(expected, actual)
        latencies = driver.media.response_latencies[responses:]
        assert latencies and set(latencies) == {response_cycles}, "memory response delay differs from configured service time"
        if session:
            assert driver.media.read_requests - reads == 64*32
            assert driver.media.write_requests == writes
            assert len(actual["jobs"]) == 192
        results.append({"operation": label, "cycles": driver.timing,
                        "jobs": len(actual["jobs"]),
                        "reads": driver.media.read_requests - reads,
                        "writes": driver.media.write_requests - writes,
                        "response_cycles": response_cycles})
        dut._log.info("%s: %s cycles, %s reads, %s writes", label, driver.timing,
                      results[-1]["reads"], results[-1]["writes"])
    (OUTPUT / "memory-timing.json").write_text(json.dumps(
        {"case": case["name"], "fabric_hz": 100_000_000, "quantum_frames": case["program"]["quantum"],
         "response_cycles": response_cycles, "results": results}, indent=2)+"\n")


@cocotb.test()
async def pipeline_protocol(dut):
    value = record_case(3)
    trace = wire.run_reference("trace", document("audio_case", value), executable=REFERENCE)
    driver = PipelineDriver(dut, CAPACITIES, MEDIA, SEED)
    await driver.load(value)
    for port, samples in enumerate(value["steps"][0]["inputs"]):
        for frame, sample in enumerate(samples):
            await driver.audio_configure(0, port*driver.max_frames+frame, sample)
    driver.set(run_buffers=value["buffers"], run_inputs=1, run_outputs=2, job_enable=0, drain_ready=0)
    await driver.start()
    for _ in range(200):
        _, after = await driver.tick()
        if after["jobs_busy"]:
            break
    else:
        raise AssertionError("jobs did not start")
    driver.media.set(cmd_valid=1, cmd_opcode=0, cfg_valid=1, cfg_write=1,
                     cfg_space=0, cfg_address=0, cfg_write_data=0)
    driver.set(run_valid=1, cfg_valid=1, cfg_state=1, cfg_write=1, cfg_address=0, cfg_write_data=7,
               audio_cfg_valid=1, audio_cfg_space=0, audio_cfg_write=1, audio_cfg_address=0, audio_cfg_write_data=123)
    for _ in range(25):
        before, _ = await driver.tick()
        assert not before["cfg_ready"] and not before["audio_cfg_ready"] and not before["run_ready"]
        assert not int(dut.store_cmd_ready.value) and not int(dut.store_cfg_ready.value)
    driver.media.set(cmd_valid=0, cfg_valid=0)
    driver.set(run_valid=0, cfg_valid=0, audio_cfg_valid=0, job_enable=1)
    for _ in range(10000):
        _, after = await driver.tick()
        if after["done"]:
            break
    else:
        raise AssertionError("pipeline did not finish")
    assert after["error_code"] == 0
    await driver.tick()
    buffers, outputs = await driver.read_samples()
    observed = {"jobs": driver.consumed, "remaining_jobs": [], "state": await driver.snapshot(),
                "buffers": buffers, "outputs": outputs, "stage": "jobs", "status": {"kind": "completed"},
                "media": await driver.media.snapshot()}
    wire.assert_audio_result_matches(trace["payload"]["value"]["results"][0], observed)
    await driver.load(value)
    driver.media.fail_next = True
    assert await driver.execute_audio(value["steps"][0]["inputs"]) == (2, 8)
    assert not int(dut.run_ready.value), "memory fault allowed another quantum"
    await driver.load(value)
    wire.assert_audio_result_matches(trace["payload"]["value"]["results"][0],
                                    await driver.quantum_audio(value["steps"][0]))
