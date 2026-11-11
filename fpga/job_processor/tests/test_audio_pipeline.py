"""Bit-exact combined state/job simulation against the actual Rust processors."""
from copy import deepcopy
import json
import os
from pathlib import Path
import cocotb
import fpga_testbench as wire
from audio_cases import directed, graph_case, make_case, ramp, randomized, samples, ONE, MIN, MAX
from audio_driver import AudioDriver
from cases import ins

CAPACITIES = tuple(map(int, os.environ["AUDIO_CAPACITIES"].split(",")))
SEED = int(os.environ["AUDIO_SEED"])
OUTPUT = Path(os.environ["AUDIO_OUTPUT"])
REFERENCE = Path(os.environ["AUDIO_REFERENCE"])


def document(kind, value):
    return {"format_version": 2, "payload": {"kind": kind, "value": value}}


def compile_graphs():
    result = {}
    for frames in sorted({1, min(3, CAPACITIES[3]), CAPACITIES[3]}):
        graph = {"chunk_frames": 1, "quantum": frames, "nodes": [
            {"kind": "input", "port": 0}, {"kind": "gain", "input": 0, "gain": ONE // 2},
            {"kind": "input", "port": 1}, {"kind": "gain", "input": 2, "gain": ONE // 2},
            {"kind": "mix", "left": 1, "right": 3}, {"kind": "output", "input": 4, "port": 0}]}
        result[frames] = wire.run_reference("compile", document("graph", graph), executable=REFERENCE)["payload"]["value"]
    return result


async def compare(dut, case, *, seed, stalls=True):
    trace = wire.run_reference("trace", document("audio_case", case), executable=REFERENCE)
    driver = AudioDriver(dut, CAPACITIES, seed)
    observed, timing = [], []
    try:
        await driver.load(case)
        for step, expected in zip(case["steps"], trace["payload"]["value"]["results"]):
            result = await driver.quantum_audio(step, stalls=stalls)
            observed.append(result)
            timing.append(driver.timing.copy())
            wire.assert_audio_result_matches(expected, result)
    except BaseException as error:
        wire.dump(document("audio_case", case), OUTPUT / "failure.case.json")
        wire.dump(trace, OUTPUT / "failure.expected.trace.json")
        (OUTPUT / "failure.observed.json").write_text(json.dumps(
            {"name": case["name"], "results": observed, "timing": timing, "error": str(error)}, indent=2) + "\n")
        dut._log.error("failed case %s; replay data in %s", case["name"], OUTPUT)
        raise
    return observed, timing


@cocotb.test()
async def directed_audio(dut):
    replay = os.environ.get("AUDIO_CASE")
    if replay:
        payload = wire.load(replay)["payload"]
        case = payload["value"]["case"] if payload["kind"] == "audio_trace" else payload["value"]
        _, timing = await compare(dut, case, seed=SEED)
        (OUTPUT / "replay-timing.json").write_text(json.dumps(timing, indent=2) + "\n")
        return
    summary, quanta, sample_count = {}, 0, 0
    for index, case in enumerate(directed(compile_graphs(), CAPACITIES)):
        results, timing = await compare(dut, case, seed=SEED + index,
                                        stalls=not case["name"].startswith("milestone"))
        summary[case["name"]] = timing
        quanta += len(results)
        sample_count += sum(sum(len(buffer) for buffer in result["buffers"] if buffer is not None)
                            + sum(map(len, result["outputs"])) for result in results)
    (OUTPUT / "directed-summary.json").write_text(json.dumps(
        {"cases": len(summary), "quanta": quanta, "compared_samples": sample_count, "timing": summary}, indent=2) + "\n")
    dut._log.info("directed audio: %d cases, %d quanta, %d sample comparisons", len(summary), quanta, sample_count)


@cocotb.test()
async def randomized_audio(dut):
    count = int(os.environ["AUDIO_RANDOM_CASES"])
    quanta = 0
    for index, case in enumerate(randomized(compile_graphs(), CAPACITIES, SEED, count)):
        results, _ = await compare(dut, case, seed=SEED + index + 1000)
        quanta += len(results)
    (OUTPUT / "random-summary.json").write_text(json.dumps({"cases": count, "quanta": quanta, "seed": SEED}, indent=2) + "\n")
    dut._log.info("random audio: %d cases, %d quanta", count, quanta)


@cocotb.test()
async def sample_protocol(dut):
    compiled = compile_graphs()[CAPACITIES[3]]
    case = graph_case(compiled, CAPACITIES, CAPACITIES[3],
                      ramps=[ramp(0, ONE, 71), ramp(ONE, -ONE, 100)], steps=1)
    driver = AudioDriver(dut, CAPACITIES, SEED)
    await driver.load(case)
    for space, size in ((0, CAPACITIES[5] * CAPACITIES[3]), (1, CAPACITIES[6] * CAPACITIES[3]),
                        (2, CAPACITIES[4] * CAPACITIES[3]), (3, 0)):
        await driver.audio_configure(space, size, MAX, expect_error=True)
        await driver.audio_configure(space, size, expect_error=True)
    for frame in range(CAPACITIES[3]):
        await driver.audio_configure(1, CAPACITIES[3] + frame, MIN if frame % 2 else MAX)
    expected = wire.run_reference("trace", document("audio_case", case), executable=REFERENCE)["payload"]["value"]["results"][0]
    wire.assert_audio_result_matches(expected, await driver.quantum_audio(case["steps"][0]))

    await driver.load(case)
    for port, data in enumerate(case["steps"][0]["inputs"]):
        for frame, value in enumerate(data):
            await driver.audio_configure(0, port * CAPACITIES[3] + frame, value)
    driver.set(run_buffers=case["buffers"], run_inputs=2, run_outputs=2, job_enable=0)
    await driver.start()
    for _ in range(200):
        _, after = await driver.tick()
        if after["jobs_busy"]:
            break
    else:
        raise AssertionError("job processor did not start")
    driver.set(cfg_valid=1, cfg_state=1, cfg_write=1, cfg_address=0, cfg_write_data=7,
               audio_cfg_valid=1, audio_cfg_space=0, audio_cfg_write=1, audio_cfg_address=0, audio_cfg_write_data=123,
               run_valid=1)
    queued = int(dut.queued_jobs.value)
    for _ in range(2 * CAPACITIES[3] + 20):
        before, after = await driver.tick()
        assert not before["monitor_ready"] and after["queued_jobs"] == queued
    driver.set(cfg_valid=0, audio_cfg_valid=0, run_valid=0, job_enable=1)
    for _ in range(300):
        await driver.tick()
        if any(job["kind"] == "gain" for job in driver.consumed):
            break
    else:
        raise AssertionError("no gain job accepted after FIFO unpause")
    for _ in range(10):
        await driver.tick()
    driver.set(reset=1)
    _, after = await driver.tick()
    assert not after["busy"] and not after["queued_jobs"] and not after["buffer_ready"]
    driver.set(reset=0)
    await driver.tick()
    assert await driver.audio_configure(0, 0) == case["steps"][0]["inputs"][0][0], "blocked sample write changed RAM"
    resumed = deepcopy(case)
    resumed["initial_state"] = await driver.snapshot()
    expected = wire.run_reference("trace", document("audio_case", resumed), executable=REFERENCE)["payload"]["value"]["results"][0]
    wire.assert_audio_result_matches(expected, await driver.quantum_audio(resumed["steps"][0]))

    unsupported = make_case("unsupported-capture", [ins("input", port=0, destination=0),
                           ins("loop_capture", view=0, enabled=1, source=0)],
                           [{"kind": "word", "value": 1}] * 2, CAPACITIES,
                           frames=min(3, CAPACITIES[3]), buffers=2, outputs=1, steps=1)
    await driver.load(unsupported)
    assert await driver.execute_audio(unsupported["steps"][0]["inputs"]) == (2, 3)
