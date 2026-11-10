"""Architectural comparisons against Rust and native resolver plus bus checks."""
import json
from copy import deepcopy
import os
from pathlib import Path
import cocotb
from cocotb.triggers import FallingEdge, Timer
import codec as abi
from cases import case as make_case, directed, seeded, workloads, spec, lid, rid, play, command, step
import fpga_testbench as wire
from driver import Driver

ROOT = Path(__file__).resolve().parents[3]
DEPTHS = tuple(map(int, os.environ["TP_DEPTHS"].split(",")))
OUTPUT = Path(os.environ["TP_OUTPUT"])
REFERENCE = Path(os.environ["TP_REFERENCE"])


def document(kind, value):
    return {"format_version": 1, "payload": {"kind": kind, "value": value}}


async def compare(dut, case):
    source = document("transition_case", case)
    trace = wire.run_reference("trace", source, executable=REFERENCE)
    wire.run_reference("check", trace, executable=REFERENCE)
    image = wire.run_reference("compile", document("transition_plan", case["plan"]), executable=REFERENCE)["payload"]["value"]
    driver = Driver(dut, DEPTHS)
    observed = []
    try:
        await driver.load(case, image)
        for step, expected in zip(case["steps"], trace["payload"]["value"]["results"]):
            actual = await driver.step(step)
            observed.append(actual)
            wire.assert_transition_result_matches(expected, actual)
    except BaseException as error:
        wire.dump(source, OUTPUT / "failure.case.json")
        wire.dump(trace, OUTPUT / "failure.expected.trace.json")
        (OUTPUT / "failure.observed.json").write_text(json.dumps({"results": observed, "error": str(error)}, indent=2) + "\n")
        dut._log.error("Failed case %s, step %d", case["name"], len(observed) - 1)
        raise
    return driver.phase_cycles


@cocotb.test()
async def directed_against_rust(dut):
    replay = os.environ.get("TP_CASE")
    if replay:
        loaded = wire.load(replay)["payload"]
        case = loaded["value"]["case"] if loaded["kind"] == "transition_trace" else loaded["value"]
        cycles = await compare(dut, case)
        (OUTPUT / "replay-cycles.json").write_text(json.dumps(cycles, indent=2) + "\n")
        return
    values = list(directed(DEPTHS))
    example = wire.load(ROOT / "scripts/fpga_testbench/examples/transition_record_play_sync.json")["payload"]["value"]
    if (example["plan"]["limits"]["loop_slots"] <= DEPTHS[0]
            and example["plan"]["limits"]["pending_requests"] <= DEPTHS[1]):
        values.append(example)
    summary = {"cases": {}, "quanta": 0}
    for value in values:
        summary["cases"][value["name"]] = await compare(dut, value)
        summary["quanta"] += len(value["steps"])
    (OUTPUT / "directed-summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    dut._log.info("Directed: %d cases, %d quanta", len(values), summary["quanta"])


@cocotb.test()
async def seeded_against_rust(dut):
    count = int(os.environ["TP_RANDOM_CASES"])
    summary = {"cases": count, "quanta": 0, "maximum_preview_cycles": 0}
    for value in seeded(DEPTHS, count):
        cycles = await compare(dut, value)
        summary["quanta"] += len(value["steps"])
        summary["maximum_preview_cycles"] = max(summary["maximum_preview_cycles"],
            max(c for row in cycles for c in row["preview"]))
        if summary["quanta"] % 800 == 0:
            dut._log.info("Seeded comparison: %d/%d quanta", summary["quanta"], count*80)
    (OUTPUT / "seeded-summary.json").write_text(json.dumps(summary, indent=2) + "\n")


@cocotb.test()
async def transport_and_negative_cases(dut):
    driver = Driver(dut, DEPTHS)
    await driver.reset()
    assert not (await driver.status())["configured"]
    assert (await driver.command(abi.READ_STATE))[0] == 5
    assert (await driver.command(abi.WRITE_SPEC, DEPTHS[0]))[0] == 2
    config = 1 | (1 << 32) | (3 << 64) | (1 << 96) | (16 << 128)
    assert (await driver.command(abi.PREPARE, data=config))[0] == 1
    for changed, error in [(dict(spec(0, 16), kind="composite"), 10),
                           (dict(spec(0, 16), timeline=0), 10),
                           (dict(spec(0, 16), id=lid(0, 0)), 3),
                           (spec(0, 17), 6), (spec(0, 16, 1), 3)]:
        await driver.reset()
        await driver.ok(abi.WRITE_SPEC, data=abi.encode_spec(changed))
        await driver.ok(abi.WRITE_MEMBER, data=1 << 32)
        assert (await driver.command(abi.PREPARE, data=config))[0] == error
    value = make_case("protocol", [spec(0, 16)], [], requests=1)
    image = wire.run_reference("compile", document("transition_plan", value["plan"]), executable=REFERENCE)["payload"]["value"]
    await driver.load(value, image)
    assert (await driver.command(255, data=2**640-1))[0] == 10
    assert (await driver.command(abi.WRITE_SPEC, data=0))[0] == 5
    assert (await driver.command(abi.PREPARE, data=config))[0] == 5
    assert (await driver.command(abi.READ_STATE, 2**32-1))[0] == 3
    assert (await driver.command(abi.READ_REQUEST, 2**32-1))[0] == 3
    assert (await driver.command(abi.READ_EVENT))[0] == 3
    accepted = await driver.ok(abi.SUBMIT, data=abi.encode_command(command(0, 1)))
    assert abi.decode_identity(accepted) == rid(0)
    await driver.ok(abi.OBS_CLEAR)
    error, first = await driver.command(abi.PREVIEW, intrude=True, stall=5)
    assert error == 0
    assert (await driver.states())[0]["mode"] == "stopped"
    for op in (abi.SUBMIT, abi.CANCEL, abi.COLLECT, abi.FINISH):
        assert (await driver.command(op))[0] == 5
    await driver.ok(abi.OBS_ADD, data=abi.encode_observation({"kind": "refused", "request": rid(0), "reason": "capacity"}))
    second = await driver.ok(abi.PREVIEW)
    assert first != second
    for op in (abi.COMMIT, abi.READ_DECISION):
        assert (await driver.command(op, data=first))[0] == 5
    await driver.ok(abi.COMMIT, data=second)
    for op in (abi.COMMIT, abi.PREVIEW, abi.OBS_CLEAR, abi.FINISH):
        assert (await driver.command(op, data=first))[0] == 5
    await driver.ok(abi.FINISH, data=second)
    assert (await driver.status())["frame"] == 16
    assert (await driver.command(abi.FINISH, data=second))[0] == 5
    assert (await driver.command(abi.READ_DECISION, data=second))[0] == 5
    result = await driver.collect(rid(0))
    assert result["result"]["value"]["outcome"] == {"kind": "failed", "frame": 0, "reason": "capacity"}
    # Fault injection reaches exhaustion boundaries without 2**64 quanta.
    await driver.load(value, image)
    await FallingEdge(dut.clk)
    dut.core.frame.value = 2**64-16
    assert (await driver.command(abi.PREVIEW))[0] == 8
    assert (await driver.status())["frame"] == 2**64-16
    await driver.load(value, image)
    await FallingEdge(dut.clk)
    dut.core.serial.value = 2**64-1
    assert (await driver.command(abi.PREVIEW))[0] == 8
    await driver.load(value, image)
    await FallingEdge(dut.clk)
    dut.core.requests.memory[0].value = (2**32-1) << 1
    request = await driver.ok(abi.SUBMIT, data=abi.encode_command(command(0, 1)))
    assert abi.decode_identity(request) == rid(0, 2**32-1)
    await driver.ok(abi.CANCEL, data=2**32-1)
    await driver.collect(rid(0, 2**32-1))
    assert (await driver.command(abi.SUBMIT, data=abi.encode_command(command(0, 2))))[0] == 2
    for mode, field in [(0, "revision"), (1, "cycle"), (2, "capture")]:
        await driver.load(value, image)
        raw = await driver.ok(abi.READ_STATE)
        if field == "revision":
            raw |= (2**64-1) << 448
        elif field == "cycle":
            raw |= (mode << 33) | (16 << 128) | ((2**64-1) << 256)
        else:
            raw |= (mode << 33) | ((2**64-16) << 192)
        await FallingEdge(dut.clk)
        dut.core.loops.memory[0].value = raw
        before = await driver.states()
        if field == "revision":
            await driver.ok(abi.SUBMIT, data=abi.encode_command(command(0, 1, {"kind": "record"})))
            token = await driver.ok(abi.PREVIEW)
            await driver.ok(abi.COMMIT, data=token)
            assert (await driver.requests())[0]["outcome"]["reason"] == "exhausted"
        else:
            assert (await driver.command(abi.PREVIEW))[0] == 8
        assert await driver.states() == before
    # Synchronous reset aborts an in-flight operation and invalidates the plan.
    await driver.load(value, image)
    await FallingEdge(dut.clk)
    driver.set(command_valid=1, command_opcode=abi.PREVIEW, command_index=0, command_data=0)
    await driver.edge()
    driver.set(command_valid=0)
    await Timer(20, units="ns")
    assert int(dut.busy.value)
    await driver.reset()
    assert not (await driver.status())["configured"]
    await Timer(100, units="ns")
    assert not int(dut.response_valid.value)
    await driver.load(value, image)
    assert (await driver.states())[0]["mode"] == "stopped"


@cocotb.test()
async def sixty_four_loop_workload(dut):
    summary = {"depths": DEPTHS, "cases": {}, "outside_physical_capacity": 0}
    values = list(workloads())
    if DEPTHS[1] >= 130:
        spare = deepcopy(values[1])
        spare["name"] = "64-member-cycle-with-130-request-slots"
        spare["plan"]["limits"]["pending_requests"] = 130
        spare["plan"]["limits"]["events"] = 258
        values.append(spare)
    for value in values:
        if DEPTHS[0] < 64 or DEPTHS[1] < 64 or DEPTHS[2] < 192:
            summary["outside_physical_capacity"] += 1
            continue
        cycles = await compare(dut, value)
        if "cycle" in value["name"]:
            for row in cycles[2:]:
                assert row["preview_work"][-1] == {"request_checks": 0, "follower_visits": 64, "pulse_deliveries": 64}
        data = {"quantum_frames": 32, "clock_hz": 150_000_000, "quanta": cycles}
        totals = [row["preview"][-1] + row["commit"] + row["finish"] for row in cycles]
        data["quantum_cycles"] = totals
        data["quantum_microseconds_at_150mhz"] = [count / 150 for count in totals]
        summary["cases"][value["name"]] = data
    (OUTPUT / "workload-summary.json").write_text(json.dumps(summary, indent=2) + "\n")
