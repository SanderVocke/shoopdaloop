"""Compare to the production resolver, including standalone transport checks."""
import json
from copy import deepcopy
import os
from pathlib import Path
import cocotb
from cocotb.triggers import FallingEdge, Timer
import codec as abi
from cases import case as make_case, directed, seeded, workloads, spec, lid, rid, play, command, step
import rt_backend_testbench as wire
from driver import Driver

ROOT = Path(__file__).resolve().parents[3]
DEPTHS = tuple(map(int, os.environ["TP_DEPTHS"].split(",")))
OUTPUT = Path(os.environ["TP_OUTPUT"])
REFERENCE = Path(os.environ["TP_REFERENCE"])



async def compare(dut, case):
    source = dict(case,format_version=1)
    trace = wire.run(source,kind="transition_case",command="transition-run",executable=REFERENCE)
    image = wire.run(source,kind="transition_case",command="transition-compile",executable=REFERENCE)
    driver = Driver(dut, DEPTHS)
    observed = []
    try:
        await driver.load(case, image)
        for step, expected in zip(case["steps"], trace["results"]):
            actual = await driver.step(step)
            observed.append(actual)
            wire.assert_exact(actual,expected)
    except BaseException as error:
        wire.dump(source, OUTPUT / "failure.case.json", "transition_case")
        wire.dump(trace, OUTPUT / "failure.expected.trace.json", "transition_trace")
        (OUTPUT / "failure.observed.json").write_text(json.dumps({"results": observed, "error": str(error)}, indent=2) + "\n")
        dut._log.error("Failed case %s, step %d", case["name"], len(observed) - 1)
        raise
    return driver.phase_cycles


@cocotb.test()
async def directed_against_rust(dut):
    replay = os.environ.get("TP_CASE")
    if replay:
        case = wire.load(replay,"transition_case")
        cycles = await compare(dut, case)
        (OUTPUT / "replay-cycles.json").write_text(json.dumps(cycles, indent=2) + "\n")
        return
    values = list(directed(DEPTHS))
    summary = {"cases": {}, "quanta": 0}
    for value in values:
        summary["cases"][value["name"]] = await compare(dut, value)
        summary["quanta"] += len(value["steps"])
    (OUTPUT / "directed-summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    dut._log.info("Directed: %d cases, %d quanta", len(values), summary["quanta"])


@cocotb.test()
async def seeded_against_rust(dut):
    count = int(os.environ["TP_RANDOM_CASES"])
    summary = {"cases": count, "quanta": 0, "maximum_process_cycles": 0}
    for value in seeded(DEPTHS, count):
        cycles = await compare(dut, value)
        summary["quanta"] += len(value["steps"])
        summary["maximum_process_cycles"] = max(summary["maximum_process_cycles"],
            max(row["process"] for row in cycles))
        if summary["quanta"] % 800 == 0:
            dut._log.info("Seeded comparison: %d/%d quanta", summary["quanta"], count*80)
    (OUTPUT / "seeded-summary.json").write_text(json.dumps(summary, indent=2) + "\n")


@cocotb.test()
async def transport_and_negative_cases(dut):
    value=make_case('transport',[spec(0,32)],[step([command(0,1)])],requests=1)
    image=wire.run(dict(value,format_version=1),kind='transition_case',command='transition-compile',executable=REFERENCE)
    driver=Driver(dut,DEPTHS)
    await driver.load(value,image)
    before=await driver.states()
    for opcode in (7,8,10,14,255):
        assert (await driver.command(opcode,stall=4,intrude=True))[0]==10
        assert await driver.states()==before
    assert (await driver.command(abi.SUBMIT,data=abi.encode_command(dict(command(0,1),target=lid(0,0)))))[0]==3
    await driver.ok(abi.SUBMIT,data=abi.encode_command(command(0,1)))
    await driver.command(abi.PROCESS,stall=5,intrude=True)
    assert (await driver.status())['frame']==16
    assert (await driver.states())[0]['position']==16
    await FallingEdge(dut.clk)
    dut.core.frame.value = 2**64-16
    assert (await driver.command(abi.PROCESS))[0]==8
    status=await driver.status()
    assert status['phase']==3 and status['fault']==8
    assert (await driver.command(abi.PROCESS))[0]==8
    assert (await driver.command(abi.SUBMIT,data=abi.encode_command(command(0,2))))[0]==5
    await driver.load(value,image)
    assert (await driver.status())['fault']==0
    await driver.ok(abi.PROCESS)
    assert (await driver.status())['frame']==16


@cocotb.test()
async def scratch_initialization_and_reset(dut):
    value=make_case('reset-during-processing',[spec(0,32)],[step([command(0,1)])],requests=1)
    image=wire.run(dict(value,format_version=1),kind='transition_case',command='transition-compile',executable=REFERENCE)
    driver=Driver(dut,DEPTHS)
    await driver.load(value,image)
    await driver.ok(abi.SUBMIT,data=abi.encode_command(command(0,1)))
    await FallingEdge(dut.clk)
    driver.set(command_valid=1,command_opcode=abi.PROCESS,response_ready=0)
    await driver.edge();driver.set(command_valid=0)
    assert int(dut.busy.value)
    await driver.reset()
    assert not (await driver.status())['configured']
    await driver.load(value,image)
    result=await driver.step(value['steps'][0])
    assert result['next_frame']==16 and result['states'][0]['position']==16


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
        data = {"quantum_frames": 32, "clock_hz": 150_000_000, "quanta": cycles}
        totals = [row["process"] for row in cycles]
        data["quantum_cycles"] = totals
        data["quantum_microseconds_at_150mhz"] = [count / 150 for count in totals]
        summary["cases"][value["name"]] = data
    (OUTPUT / "workload-summary.json").write_text(json.dumps(summary, indent=2) + "\n")
