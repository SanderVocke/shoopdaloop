"""Complete state/job/media quanta compared with Rust, including FIFO failure effects."""
from copy import deepcopy
import json
import os
import time
from pathlib import Path
import random
import cocotb
import fpga_testbench as wire
from pipeline_driver import PipelineDriver
from codec import encode_value

CAPACITIES = tuple(map(int, os.environ["AUDIO_CAPACITIES"].split(",")))
MEDIA = tuple(map(int, os.environ["MEDIA_CAPACITIES"].split(",")))
SEED = int(os.environ["MEDIA_SEED"])
OUTPUT = Path(os.environ["MEDIA_OUTPUT"])
REFERENCE = Path(os.environ["MEDIA_REFERENCE"])
ROOT = Path(__file__).resolve().parents[3]


def document(kind, value):
    return {"format_version": 3, "payload": {"kind": kind, "value": value}}


def record_case(frames, seed=0):
    rng = random.Random(seed)
    graph = {"chunk_frames": MEDIA[2], "quantum": frames, "nodes": [
        {"kind": "input", "port": 0}, {"kind": "gain", "input": 0, "gain": 8388608},
        {"kind": "capture", "input": 1, "view": 0, "recording": True},
        {"kind": "playback", "view": 0, "length_chunks": max(1, frames // MEDIA[2]), "position_chunks": 0, "playing": False},
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
                {"writes": writes(0, 1, 0), "inputs": [[0]*frames]},
                {"writes": writes(1, 0), "inputs": [[-3]*frames], "media_actions": [{"kind": "clear", "view": 0}]},
                {"writes": writes(0, 1), "inputs": [[0]*frames]},
            ]}


def session_case(seed):
    rng = random.Random(seed)
    frames, tracks, loops, length = 32, 8, 8, 512
    nodes, views, samples = [], [], []
    for track in range(tracks):
        mixed = None
        for loop in range(loops):
            view = track * loops + loop
            nodes.append({"kind": "playback", "view": view, "length_chunks": length // MEDIA[2],
                          "position_chunks": view % (length // MEDIA[2]), "playing": True})
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
    compiled = wire.run_reference("compile", document("graph", {"quantum": frames, "chunk_frames": MEDIA[2], "nodes": nodes}),
                                  executable=REFERENCE)["payload"]["value"]
    positions = [(binding["value"]["position"], node["position_chunks"])
                 for node, binding in zip(nodes, compiled["bindings"]) if node["kind"] == "playback"]
    return {"name": "8-tracks-8-playing-loops-q32", "program": compiled["program"],
            "initial_state": {"memory": compiled["initial_state"]}, "queue_capacity": CAPACITIES[2],
            "buffers": len(nodes), "output_ports": tracks,
            "media": {"chunk_frames": MEDIA[2], "chunk_limit": MEDIA[1], "samples": samples,
                      "free_chunks": [], "views": views},
            "steps": [{"writes": [{"address": address, "value": {"kind": "word", "value": (start+i) % (length // MEDIA[2])}}
                                   for address, start in positions], "inputs": []} for i in range(4)]}


def dependent_case():
    cf = MEDIA[2]
    graph = {"quantum": cf, "chunk_frames": cf, "nodes": [
        {"kind": "playback", "view": 0, "position_chunks": 0, "length_chunks": 1, "playing": True},
        {"kind": "gain", "input": 0, "gain": 8388608},
        {"kind": "capture", "input": 1, "view": 0, "recording": True},
        {"kind": "playback", "view": 0, "position_chunks": 1, "length_chunks": 2, "playing": True},
        {"kind": "output", "input": 3, "port": 0}]}
    compiled = wire.run_reference("compile", document("graph", graph), executable=REFERENCE)["payload"]["value"]
    first = compiled["bindings"][0]["value"]
    second = compiled["bindings"][3]["value"]
    rows = []
    for index in range(3):
        values = [(first["position"], index), (first["length"], index+1),
                  (second["position"], index+1), (second["length"], index+2)]
        rows.append({"writes": [{"address": a, "value": {"kind": "word", "value": v}} for a,v in values], "inputs": []})
    return {"name": "ordered-read-capture-publish-read", "program": compiled["program"],
            "initial_state": {"memory": compiled["initial_state"]}, "queue_capacity": CAPACITIES[2],
            "buffers": 5, "output_ports": 1,
            "media": {"chunk_frames": cf, "samples": [100000+i*128 for i in range(cf)],
                      "free_chunks": [], "views": [{"chunks": [0], "frames": cf}]}, "steps": rows}


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
    finally:
        driver.media.close()
    return timing


@cocotb.test()
async def directed_pipeline(dut):
    replay = os.environ.get("MEDIA_CASE")
    if replay:
        payload = wire.load(replay)["payload"]
        await compare(dut, payload["value"]["case"] if payload["kind"] == "audio_trace" else payload["value"], SEED)
        return
    cases = [record_case(n) for n in sorted({MEDIA[2], CAPACITIES[3]})]
    if CAPACITIES[3] > MEDIA[2]:
        wrapped = record_case(CAPACITIES[3])
        wrapped["name"] = "multiple-chunk-window-with-loop-wrap"
        position = wrapped["program"]["instructions"][3]["position"]
        for write in wrapped["steps"][1]["writes"]:
            if write["address"] == position:
                write["value"]["value"] = CAPACITIES[3] // MEDIA[2] - 1
        cases.append(wrapped)
    cf = MEDIA[2]
    shared = record_case(cf)
    shared["name"] = "aligned-retained-range-and-capture"
    shared["media"].update(samples=list(range(1, 3*cf+1)), views=[{"chunks": [0, 1, 2], "frames": 3*cf}])
    capture = shared["program"]["instructions"][2]
    playback = shared["program"]["instructions"][3]
    shared["initial_state"]["memory"][capture["enabled"]]["value"] = 0
    shared["initial_state"]["memory"][playback["enabled"]]["value"] = 1
    shared["initial_state"]["memory"][playback["view"]]["value"] = 1
    shared["steps"] = [
        {"writes": [], "inputs": [[0]*cf], "media_actions": [
            {"kind": "slice", "source": 0, "start": cf, "frames": cf}, {"kind": "clear", "view": 0}]},
        {"writes": [{"address": capture["enabled"], "value": {"kind": "word", "value": 1}}], "inputs": [[1234]*cf]},
    ]
    cases.append(shared)
    if CAPACITIES[1] >= 11:
        cases.append(dependent_case())
    invalid = record_case(cf)
    invalid["media"]["chunk_limit"] = 0
    invalid["name"] = "capture-capacity-failure"
    cases.append(invalid)
    readonly = record_case(cf)
    readonly["media"]["views"][0]["range"] = {"offset": 0}
    readonly["name"] = "capture-readonly-failure"
    cases.append(readonly)
    missing = record_case(cf)
    missing["media"]["views"] = []
    missing["name"] = "capture-missing-view-failure"
    cases.append(missing)
    control = record_case(cf)
    control["name"] = "slice-control-failure"
    control["steps"][0]["media_actions"] = [{"kind": "slice", "source": 0, "start": 1, "frames": 1}]
    cases.append(control)
    playback_failure = record_case(cf)
    playback_failure["name"] = "read-length-failure"
    playback = playback_failure["program"]["instructions"][3]
    playback_failure["steps"][1]["writes"].append({"address": playback["length"], "value": {"kind": "word", "value": 4}})
    cases.append(playback_failure)
    overflow = record_case(cf)
    overflow["name"] = "chunk-position-overflows-frame-address"
    playback = overflow["program"]["instructions"][3]
    position = 1 << (64 - (cf.bit_length()-1))
    for write in overflow["steps"][1]["writes"]:
        if write["address"] == playback["position"]:
            write["value"]["value"] = position
    overflow["steps"][1]["writes"].append({"address": playback["length"], "value": {"kind": "word", "value": position+1}})
    cases.append(overflow)
    timing = {}
    for index, value in enumerate(cases):
        timing[value["name"]] = await compare(dut, value, SEED+index, stalls=index >= 3)
    (OUTPUT / "directed-summary.json").write_text(json.dumps(
        {"cases": len(cases), "quanta": sum(map(len, timing.values())), "timing": timing}, indent=2)+"\n")


@cocotb.test()
async def randomized_pipeline(dut):
    count = int(os.environ["MEDIA_RANDOM_CASES"])
    for index in range(count):
        frames = (MEDIA[2], CAPACITIES[3])[index % 2]
        await compare(dut, record_case(frames, SEED+index), SEED+index+1000)
    (OUTPUT / "random-summary.json").write_text(json.dumps({"cases": count, "quanta": 4*count}, indent=2)+"\n")


@cocotb.test()
async def memory_timing(dut):
    session = os.environ["MEDIA_CONFIG"] == "8x8"
    analyze = os.environ.get("MEDIA_PREFETCH_ANALYSIS") == "1"
    case = session_case(SEED) if session else record_case(CAPACITIES[3], SEED)
    labels = [f"playback_{index+1}" for index in range(len(case["steps"]))] if session else [
        "fresh_record", "playback", "reused_record", "playback_again"]
    wire.dump(document("audio_case", case), OUTPUT / "memory-timing.case.json")
    trace = wire.run_reference("trace", document("audio_case", case), executable=REFERENCE)
    driver = PipelineDriver(dut, CAPACITIES, MEDIA, SEED)
    if analyze:
        from prefetch_timing import PrefetchTiming
        driver.timing_probe = PrefetchTiming(dut, case["media"], MEDIA[2])
    await driver.load(case)
    if driver.media.axi is not None and hasattr(driver.media.axi.memory.timing, "bus_snapshot"):
        from bus_timing import QuantumBusTiming
        driver.bus_probe = QuantumBusTiming(dut, driver.media.axi.memory.timing)
    response_cycles = int(os.environ["MEDIA_RESPONSE_CYCLES"])
    from options import metadata
    configuration = metadata()
    fabric_hz = configuration["fabric_hz"]
    started = time.perf_counter()
    results = []
    analysis = []
    for label, step, expected in zip(labels, case["steps"], trace["payload"]["value"]["results"]):
        reads, writes = driver.media.read_requests, driver.media.write_requests
        responses = len(driver.media.response_latencies)
        if analyze:
            driver.timing_probe.begin()
        if driver.bus_probe is not None:
            driver.bus_probe.begin()
        actual = await driver.quantum_audio(step, stalls=False)
        wire.assert_audio_result_matches(expected, actual)
        latencies = driver.media.response_latencies[responses:]
        if driver.media.axi is None:
            assert all(value == response_cycles for value in latencies), "memory response delay differs from configured service time"
        if session:
            assert sum(job["kind"] == "read_view" for job in actual["jobs"]) == 64
            assert driver.media.write_requests == writes
            assert len(actual["jobs"]) == 192
        results.append({"operation": label, "cycles": driver.timing,
                        "jobs": len(actual["jobs"]),
                        "reads": driver.media.read_requests - reads,
                        "writes": driver.media.write_requests - writes,
                        "response_cycles": response_cycles})
        results[-1]["total_us"] = driver.timing["total"]*1e6/fabric_hz
        if driver.bus_probe is not None:
            results[-1]["ddr_bus"] = driver.bus_probe.finish(driver.timing["total"])
        if analyze:
            measured = driver.timing_probe.finish(driver.timing["total"], response_cycles,
                                                 uniform_word_port=driver.media.axi is None)
            measured["operation"] = label
            analysis.append(measured)
        dut._log.info("%s: %s cycles, %s reads, %s writes", label, driver.timing,
                      results[-1]["reads"], results[-1]["writes"])
    statistics = driver.media.axi.report(final=True) if driver.media.axi is not None else None
    (OUTPUT / "memory-timing.json").write_text(json.dumps(
        {"case": case["name"], "fabric_hz": fabric_hz, "quantum_frames": case["program"]["quantum"],
         "configuration": configuration, "seed": SEED, "simulation_seconds": time.perf_counter()-started,
         "memory_statistics": statistics,
         "read_buffers": int(os.environ["MEDIA_READ_BUFFERS"]), "write_buffers": int(os.environ["MEDIA_WRITE_BUFFERS"]),
         "response_cycles": response_cycles, "results": results}, indent=2)+"\n")
    if analyze:
        (OUTPUT / "prefetch-analysis.json").write_text(json.dumps(
            {"fabric_hz": fabric_hz, "quantum_frames": MEDIA[2], "configuration": configuration,
             "assumptions": ["Playback only, one aligned unique chunk per read, stable published mappings",
                             "All current-quantum chunks fit in the read pool; hints are not dropped",
                             "One serial chunk transfer at the measured word-port service rate",
                             "No prefetch or residency carried into the model from earlier iterations",
                             "Eight cycles per serial metadata resolution unless specified; RAM-backed tables",
                             "Replay is only supplied for legacy fixed latency; AXI results are measured RTL timings"],
             "results": analysis}, indent=2)+"\n")
    driver.media.close()


@cocotb.test()
async def playback_stream_protocol(dut):
    driver = PipelineDriver(dut, CAPACITIES, MEDIA, SEED+91)
    core = dut.core.pipeline.sample_engine if driver.media.axi is not None else dut.pipeline.sample_engine
    writes = []
    before_clock = driver.media.before_clock

    def observe():
        before_clock()
        if not int(dut.reset.value) and int(core.playback_write.value):
            assert not (int(dut.buffer_ready.value) & 1), 'playback published before its final write'
            writes.append((driver.media.cycles, int(core.scratch_ram_write_address.value)))

    driver.media.before_clock = observe

    def make_case(frames, second_view=False):
        nodes = [{'kind':'playback', 'view':0, 'playing':True, 'position_chunks':0, 'length_chunks':1}]
        if second_view:
            nodes.append({'kind':'playback', 'view':1, 'playing':True, 'position_chunks':0, 'length_chunks':1})
        nodes.append({'kind':'output', 'input':0, 'port':0})
        compiled = wire.run_reference('compile', document('graph', {'quantum':frames,
            'chunk_frames':MEDIA[2], 'nodes':nodes}), executable=REFERENCE)['payload']['value']
        views = 2 if second_view else 1
        return {'name':'playback-stream', 'program':compiled['program'],
            'initial_state':{'memory':compiled['initial_state']}, 'queue_capacity':CAPACITIES[2],
            'buffers':2, 'output_ports':1,
            'media':{'chunk_frames':MEDIA[2], 'samples':[13-7*i for i in range(views*MEDIA[2])],
                     'free_chunks':[], 'views':[{'chunks':[i], 'frames':MEDIA[2]} for i in range(views)]},
            'steps':[{'writes':[], 'inputs':[]}]}

    async def warm(case):
        await driver.load(case)
        result = await driver.media.command({'kind':'read', 'view':0, 'position':0,
                                            'length':MEDIA[2], 'frames':case['program']['quantum']}, stalls=False)
        assert result['status']['kind'] == 'completed'
        writes.clear()

    frames = min(CAPACITIES[3], MEDIA[2])
    case = make_case(frames)
    target = wire.run_reference('trace', document('audio_case', case), executable=REFERENCE)['payload']['value']['results'][0]
    assert target['status']['kind'] == 'completed'
    await warm(case)
    wire.assert_audio_result_matches(target, await driver.quantum_audio(case['steps'][0], stalls=False))
    assert [address for _,address in writes] == list(range(frames))
    assert all(b[0]-a[0] == 1 for a,b in zip(writes, writes[1:])), 'playback scratch writes were not consecutive'
    for prefix in sorted({0, min(1, frames-1), frames//2, frames-1}):
        await warm(case)
        driver.consumed, driver.drained = [], []
        driver.set(run_buffers=2, run_inputs=0, run_outputs=1, job_enable=1, drain_ready=0)
        await driver.start()
        for _ in range(10000):
            if (len(writes) >= prefix and int(core.media_read_ready.value)
                    and (prefix != 0 or int(core.playback_write.value))):
                break
            await driver.tick()
        else:
            raise AssertionError('playback did not reach reset placement')
        driver.set(reset=1)
        await driver.tick()
        driver.set(reset=0)
        for _ in range(6):
            await driver.tick()
            assert not int(core.scratch_write.value) and not int(dut.buffer_ready.value), 'stale playback write after reset'
        await warm(case)
        wire.assert_audio_result_matches(target, await driver.quantum_audio(case['steps'][0], stalls=False))

    if frames >= 16 and driver.media.axi is not None:
        abort_case = make_case(frames, second_view=True)
        await warm(abort_case)
        driver.media.pause_responses = True
        driver.media.fail_next = True
        driver.consumed, driver.drained = [], []
        driver.set(run_buffers=2, run_inputs=0, run_outputs=1, job_enable=1, drain_ready=0)
        await driver.start()
        for _ in range(10000):
            await driver.tick()
            if writes:
                break
        else:
            raise AssertionError('cached playback did not overlap failing prefetch')
        assert driver.media.pending is not None and driver.media.pending[2]
        driver.media.pause_responses = False
        for _ in range(10000):
            _, after = await driver.tick()
            if after['done']:
                break
        else:
            raise AssertionError('playback did not abort on a memory fault')
        assert after['error_stage'] == 2 and after['error_code'] == 8
        assert not (after['buffer_ready'] & 1)
        for _ in range(6):
            await driver.tick()
            assert not int(core.playback_write.value) and not int(core.scratch_write.value)
        await warm(case)
        wire.assert_audio_result_matches(target, await driver.quantum_audio(case['steps'][0], stalls=False))
    driver.media.close()


@cocotb.test()
async def pipeline_protocol(dut):
    value = record_case(MEDIA[2])
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
    fault_case = deepcopy(value)
    fault_case["media"]["samples"] = list(range(1, MEDIA[2]+1))
    fault_case["media"]["views"] = [{"chunks": [0], "frames": MEDIA[2]}]
    await driver.load(fault_case)
    for write in fault_case["steps"][1]["writes"]:
        await driver.configure(True, write["address"], encode_value(write["value"]))
    driver.media.fail_next = True
    assert await driver.execute_audio(value["steps"][1]["inputs"]) == (2, 8)
    assert not int(dut.run_ready.value), "memory fault allowed another quantum"
    await driver.load(value)
    wire.assert_audio_result_matches(trace["payload"]["value"]["results"][0],
                                    await driver.quantum_audio(value["steps"][0]))
    if driver.media.axi is not None and not driver.media.native_memory and CAPACITIES[3] >= 16:
        frames, cf = CAPACITIES[3], MEDIA[2]
        graph = {"quantum": frames, "chunk_frames": cf, "nodes": [
            {"kind": "input", "port": 0}, {"kind": "gain", "input": 0, "gain": 8388608},
            {"kind": "mix", "left": 0, "right": 1},
            {"kind": "playback", "view": 0, "position_chunks": 0, "length_chunks": frames//cf, "playing": True},
            {"kind": "output", "input": 2, "port": 0}]}
        compiled = wire.run_reference("compile", document("graph", graph), executable=REFERENCE)["payload"]["value"]
        abort_case = {"name": "fault-during-arithmetic-stream", "program": compiled["program"],
            "initial_state": {"memory": compiled["initial_state"]}, "queue_capacity": CAPACITIES[2],
            "buffers": 4, "output_ports": 1,
            "media": {"chunk_frames": cf, "samples": list(range(frames)), "free_chunks": [],
                      "views": [{"chunks": list(range(frames//cf)), "frames": frames}]},
            "steps": [{"writes": [], "inputs": [[7*i-13 for i in range(frames)]]}]}
        complete = wire.run_reference("trace", document("audio_case", abort_case), executable=REFERENCE)
        complete_buffers = complete["payload"]["value"]["results"][0]["buffers"]
        core = dut.core.pipeline.sample_engine
        for kind, destination in (("gain", 1), ("mix", 2)):
            await driver.load(abort_case)
            driver.media.stalls = False
            driver.media.pause_responses = True
            driver.media.fail_next = True
            for frame, sample in enumerate(abort_case["steps"][0]["inputs"][0]):
                await driver.audio_configure(0, frame, sample)
            driver.consumed, driver.drained = [], []
            driver.set(run_buffers=4, run_inputs=1, run_outputs=1, job_enable=1, drain_ready=0)
            await driver.start()
            for _ in range(1000):
                await driver.tick()
                if (driver.consumed and driver.consumed[-1]["kind"] == kind
                        and int(core.streaming.value) and int(core.stream_store_valid.value)):
                    break
            else:
                raise AssertionError(f"{kind} did not start while the failing prefetch was held")
            assert driver.media.pending is not None and driver.media.pending[2]
            driver.media.pause_responses = False
            ready_at_fault = None
            for _ in range(1000):
                _, after = await driver.tick()
                if int(core.media_fault.value) and ready_at_fault is None:
                    ready_at_fault = after["buffer_ready"] & (1 << destination)
                if after["done"]:
                    break
            else:
                raise AssertionError("arithmetic stream did not abort on a memory fault")
            assert after["error_stage"] == 2 and after["error_code"] == 8
            assert ready_at_fault is not None
            assert after["buffer_ready"] & (1 << destination) == ready_at_fault
            if ready_at_fault:
                buffers, _ = await driver.read_samples()
                assert buffers[destination] == complete_buffers[destination]
            for _ in range(6):
                await driver.tick()
                assert not int(core.stream_store_valid.value) and not int(core.scratch_write.value)
        await driver.load(value)
        wire.assert_audio_result_matches(trace["payload"]["value"]["results"][0],
                                        await driver.quantum_audio(value["steps"][0]))
    elif driver.media.axi is not None:
        dut._log.info('Active-arithmetic fault placement needs a Python backend and >=16 frames; short jobs can finish before burst drain reports a fault')
    driver.media.close()
