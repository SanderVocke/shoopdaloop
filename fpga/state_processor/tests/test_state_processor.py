"""Behavioral comparison against the actual Rust executor plus RTL protocol checks."""

from copy import deepcopy
from collections import Counter
import json
import os
from pathlib import Path

import cocotb

import fpga_testbench as wire
from cases import case, directed, ins, ramp, random_cases, word
from codec import encode_instruction, encode_value
from driver import Driver

ROOT = Path(__file__).resolve().parents[3]
DEPTHS = tuple(map(int, os.environ["SP_DEPTHS"].split(",")))
SEED = int(os.environ["SP_SEED"])
ARTIFACTS = Path(os.environ["SP_ARTIFACT_DIR"])
EXECUTABLE = Path(os.environ["SHOOP_FPGA_REFERENCE"])


def document(kind, value):
    return {"format_version": 1, "payload": {"kind": kind, "value": value}}


def fits(value):
    return (len(value["program"]["instructions"]) <= DEPTHS[0]
            and len(value["initial_state"]["memory"]) <= DEPTHS[1])


async def compare(dut, value, *, seed):
    input_document = document("case", value)
    trace_document = wire.run_reference("trace", input_document, executable=EXECUTABLE)
    trace = trace_document["payload"]["value"]
    driver = Driver(dut, DEPTHS, seed)
    observed = []
    cycles = []
    try:
        await driver.load(value)
        for step, expected in zip(value["steps"], trace["results"]):
            result = await driver.quantum(step["writes"])
            observed.append(result)
            cycles.append(driver.last_run_cycles)
            wire.assert_result_matches(expected, result)
    except BaseException as error:
        wire.dump(input_document, ARTIFACTS / "failure.case.json")
        wire.dump(trace_document, ARTIFACTS / "failure.expected.trace.json")
        (ARTIFACTS / "failure.observed.json").write_text(json.dumps(
            {"name": value["name"], "seed": seed, "results": observed, "error": str(error)},
            indent=2) + "\n")
        dut._log.error("case %s failed, seed=%d; artifacts in %s", value["name"], seed, ARTIFACTS)
        raise
    return observed, cycles


def accumulate(results, jobs, statuses):
    for result in results:
        jobs.update(job["kind"] for job in result["jobs"])
        status = result["status"]
        statuses.update([status["value"]["error"] if status["kind"] == "failed" else "completed"])


@cocotb.test()
async def directed_against_rust(dut):
    replay = os.environ.get("SP_CASE_FILE")
    if replay:
        loaded = wire.load(replay)["payload"]
        value = loaded["value"]["case"] if loaded["kind"] == "trace" else loaded["value"]
        _, cycles = await compare(dut, value, seed=SEED)
        (ARTIFACTS / "replay-cycles.json").write_text(json.dumps(
            {"case": value["name"], "quantum_frames": value["program"]["quantum"],
             "execution_cycles": cycles}, indent=2) + "\n")
        return
    values = list(directed(DEPTHS))
    example = deepcopy(wire.load(ROOT / "scripts/fpga_testbench/examples/record_play.json")["payload"]["value"])
    example["queue_capacity"] = DEPTHS[2]
    values.append(example)
    graph = {"quantum": 32, "nodes": [
        {"kind": "input", "port": 3},
        {"kind": "playback", "view": 17, "length": 45, "position": 40, "playing": True},
        {"kind": "gain", "input": 0, "gain": 8388608},
        {"kind": "mix", "left": 1, "right": 2},
        {"kind": "capture", "input": 3, "view": 17, "recording": True},
        {"kind": "output", "input": 3, "port": 11},
    ]}
    compiled = wire.run_reference("compile", document("graph", graph), executable=EXECUTABLE)["payload"]["value"]
    values.append({"name": "rust-compiled-graph", "program": compiled["program"],
                   "initial_state": {"memory": compiled["initial_state"]},
                   "queue_capacity": DEPTHS[2], "steps": [{"writes": []}] * 3})
    executed, quanta = [], 0
    jobs, statuses = Counter(), Counter()
    cycle_counts = {}
    for index, value in enumerate(values):
        if fits(value):
            results, cycles = await compare(dut, value, seed=SEED + index)
            cycle_counts[value["name"]] = cycles
            quanta += len(results)
            accumulate(results, jobs, statuses)
            executed.append(value["name"])
    report = {"depths": DEPTHS, "cases": executed, "quanta": quanta,
              "jobs": dict(jobs), "statuses": dict(statuses),
              "execution_cycles": cycle_counts,
              "outside_physical_capacity": len(values) - len(executed)}
    (ARTIFACTS / "directed-summary.json").write_text(json.dumps(report, indent=2) + "\n")
    dut._log.info("Rust comparison: %d directed cases, %d quanta", len(executed), quanta)
    assert set(statuses) == {"completed", "queue_full", "invalid_state"}
    if DEPTHS[0] >= 7 and DEPTHS[1] >= 5:
        assert set(jobs) == {"input", "silence", "read_view", "gain", "mix", "capture", "output"}


@cocotb.test()
async def randomized_against_rust(dut):
    count = int(os.environ["SP_RANDOM_CASES"])
    quanta = 0
    jobs, statuses = Counter(), Counter()
    measured_cycles = []
    for index, value in enumerate(random_cases(DEPTHS, SEED, count)):
        results, cycles = await compare(dut, value, seed=SEED + 10000 + index)
        measured_cycles.extend(value for value in cycles if value is not None)
        quanta += len(results)
        accumulate(results, jobs, statuses)
    (ARTIFACTS / "random-summary.json").write_text(json.dumps(
        {"depths": DEPTHS, "seed": SEED, "cases": count, "quanta": quanta,
         "execution_cycles": {"min": min(measured_cycles, default=None),
                              "max": max(measured_cycles, default=None), "total": sum(measured_cycles)},
         "jobs": dict(jobs), "statuses": dict(statuses)}, indent=2) + "\n")
    dut._log.info("Rust comparison: %d randomized cases, %d quanta, seed=%d", count, quanta, SEED)


@cocotb.test()
async def configuration_and_error_protocol(dut):
    driver = Driver(dut, DEPTHS, SEED)
    value = case("protocol", [ins("gain", ramp=0, source=1, destination=2)], [ramp()], DEPTHS[2])
    await driver.load(value)
    for space, depth in ((False, DEPTHS[0]), (True, DEPTHS[1])):
        await driver.configure(space, depth, 123, expect_error=True)
        await driver.configure(space, depth, expect_error=True)
        await driver.configure(space, 65535, expect_error=True)
    assert await driver.snapshot() == value["initial_state"], "rejected configuration write aliased state"
    assert await driver.configure(False, 0) == encode_instruction(value["program"]["instructions"][0])

    driver.run_inputs()
    driver.set(cfg_valid=1, cfg_state=1, cfg_write=0, cfg_address=0)
    before, after = await driver.tick()
    assert before["cfg_ready"] and not before["run_ready"], "configuration did not win arbitration"
    assert after["cfg_response_valid"] and not after["busy"]
    driver.set(cfg_valid=0)
    before, _ = await driver.tick()
    assert not before["run_ready"], "run accepted before configuration response retired"
    driver.set(run_valid=0)

    # Hold configuration and a changed run request across every busy cycle.
    after = await driver.start()
    assert after["busy"]
    driver.set(cfg_valid=1, cfg_state=1, cfg_write=1, cfg_address=0, cfg_write_data=encode_value(word(7)),
               run_valid=1, run_instruction_count=0, job_ready=1)
    for _ in range(20):
        before, after = await driver.tick()
        if after["done"]:
            break
    else:
        raise AssertionError("run with blocked requests failed to finish")
    assert before["busy"] and not before["cfg_ready"] and not before["run_ready"]
    driver.set(cfg_valid=0, run_valid=0, job_ready=0)
    await driver.tick()
    expected = wire.run_reference("trace", document("case", value), executable=EXECUTABLE)["payload"]["value"]["results"][0]
    wire.assert_result_matches(expected, {"jobs": await driver.drain(), "state": await driver.snapshot(),
                                          "status": {"kind": "completed"}})

    await driver.load(value)
    assert await driver.execute(run_instruction_count=DEPTHS[0] + 1) == 3
    assert await driver.snapshot() == value["initial_state"]
    assert await driver.execute(run_state_count=DEPTHS[1] + 1) == 2
    assert await driver.snapshot() == value["initial_state"]
    await driver.configure(False, 0, 255)
    assert await driver.execute() == 3
    assert await driver.drain() == []
    assert await driver.snapshot() == value["initial_state"]

    for bad in (65536, (1 << 32) - 1, -1):
        try:
            encode_instruction(ins("input", port=bad, destination=0))
        except ValueError:
            pass
        else:
            raise AssertionError("codec silently truncated an unrepresentable operand")


@cocotb.test()
async def nonempty_admission_and_reset(dut):
    driver = Driver(dut, DEPTHS, SEED)
    value = case("reset", [ins("gain", ramp=0, source=1, destination=2)], [ramp()], DEPTHS[2])
    await driver.load(value)
    after = await driver.start()
    assert after["busy"]
    driver.set(reset=1)
    _, after = await driver.tick()
    assert not after["busy"] and not after["job_valid"] and not after["queued_jobs"]
    driver.set(reset=0)
    await driver.tick()
    assert await driver.snapshot() == value["initial_state"]
    assert await driver.execute() == 0
    updated = await driver.snapshot()
    assert int(dut.queued_jobs.value) == 1
    # A second run must fail without mutating the completed first run's ramp or queue.
    assert await driver.execute() == 1
    assert await driver.snapshot() == updated
    assert int(dut.queued_jobs.value) == 1
    driver.set(reset=1)
    await driver.tick()
    driver.set(reset=0)
    await driver.tick()
    assert int(dut.queued_jobs.value) == 0
    assert await driver.snapshot() == updated, "reset cleared retained architectural state"
    assert await driver.configure(False, 0) == encode_instruction(value["program"]["instructions"][0])
    resumed = deepcopy(value)
    resumed["initial_state"] = updated
    expected = wire.run_reference("trace", document("case", resumed), executable=EXECUTABLE)["payload"]["value"]["results"][0]
    wire.assert_result_matches(expected, await driver.quantum([]))
