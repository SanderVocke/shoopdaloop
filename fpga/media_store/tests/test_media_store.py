"""Bit-exact RTL/Rust store comparison, including allocator metadata and DDR stalls."""
from copy import deepcopy
import json
import os
from pathlib import Path
import cocotb
import fpga_testbench as wire
from media_cases import directed, randomized, case, command, MAX
from media_driver import Driver

CAPACITIES = tuple(map(int, os.environ["MEDIA_CAPACITIES"].split(",")))
SEED = int(os.environ["MEDIA_SEED"])
OUTPUT = Path(os.environ["MEDIA_OUTPUT"])
REFERENCE = Path(os.environ["MEDIA_REFERENCE"])


def document(kind, value):
    return {"format_version": 3, "payload": {"kind": kind, "value": value}}


async def compare(dut, value, *, seed, stalls=True):
    expected = wire.run_reference("trace", document("media_case", value), executable=REFERENCE)
    driver = Driver(dut, CAPACITIES, seed)
    observed, timing = [], []
    try:
        await driver.load(value["media"])
        assert await driver.snapshot() == value["media"]
        for step, result in zip(value["steps"], expected["payload"]["value"]["results"]):
            requests = driver.sample_transactions
            actual = await driver.command(step, stalls=stalls)
            observed.append(actual)
            timing.append(driver.timing)
            wire.assert_media_result_matches(result, actual)
            if step["kind"] not in ("append", "read"):
                assert driver.sample_transactions == requests, "metadata command accessed sample memory"
    except BaseException as error:
        wire.dump(document("media_case", value), OUTPUT / "failure.case.json")
        wire.dump(expected, OUTPUT / "failure.expected.trace.json")
        (OUTPUT / "failure.observed.json").write_text(json.dumps(
            {"name": value["name"], "results": observed, "timing": timing, "error": str(error)}, indent=2) + "\n")
        dut._log.error("failed case %s; replay artifacts in %s", value["name"], OUTPUT)
        raise
    finally:
        driver.close()
    return expected, timing


@cocotb.test()
async def directed_media(dut):
    replay = os.environ.get("MEDIA_CASE")
    if replay:
        payload = wire.load(replay)["payload"]
        value = payload["value"]["case"] if payload["kind"] == "media_trace" else payload["value"]
        await compare(dut, value, seed=SEED)
        return
    summary = []
    for index, value in enumerate(directed(CAPACITIES)):
        trace, timing = await compare(dut, value, seed=SEED+index, stalls=index != 0)
        summary.append({"name": value["name"], "commands": len(timing), "cycles": timing})
        if index < 2:
            value = deepcopy(value)
            # Restore shared ownership, holes, free-stack order and partial tails.
            value["name"] += "-resumed"
            value["media"] = trace["payload"]["value"]["results"][3]["media"]
            value["steps"] = value["steps"][4:]
            _, timing = await compare(dut, value, seed=SEED+index+100)
            summary.append({"name": value["name"], "commands": len(timing), "cycles": timing})
    (OUTPUT / "directed-summary.json").write_text(json.dumps(summary, indent=2) + "\n")


@cocotb.test()
async def randomized_media(dut):
    count = int(os.environ["MEDIA_RANDOM_CASES"])
    commands = 0
    for index, value in enumerate(randomized(CAPACITIES, SEED, count)):
        _, timing = await compare(dut, value, seed=SEED+index+1000)
        commands += len(timing)
    (OUTPUT / "random-summary.json").write_text(json.dumps({"seed": SEED, "cases": count, "commands": commands}, indent=2) + "\n")


@cocotb.test()
async def media_protocol(dut):
    driver = Driver(dut, CAPACITIES, SEED)
    unpublished = case("unpublished-hint", CAPACITIES[2], CAPACITIES[1], [])["media"]
    unpublished["views"] = [{"chunks": [], "frames": 0}]
    await driver.load(unpublished)
    driver.set(hint_valid=1, hint_view=0, hint_position=0, hint_length=CAPACITIES[2], hint_count=CAPACITIES[2])
    for _ in range(100):
        ready = int(dut.hint_ready.value)
        await driver.tick()
        if ready:
            break
    else:
        raise AssertionError("unpublished hint was not admitted")
    driver.set(hint_valid=0)
    for _ in range(20):
        await driver.tick()
    driver.base += 0x1004
    driver.set(arena_base=driver.base)
    result = await driver.command(command("append", view=0, samples=[117]*CAPACITIES[2]))
    assert result["status"]["kind"] == "completed", "ignored hint bound the old arena"
    await driver.load(case("empty", CAPACITIES[2], CAPACITIES[1], [])["media"])
    for space, size in ((0, 4), (1, CAPACITIES[0]), (2, driver.table_address(CAPACITIES[0],0)),
                        (3, CAPACITIES[1]), (4, CAPACITIES[1]), (7, 0)):
        await driver.configure(space, size, 123, expect_error=True)
        await driver.configure(space, size, expect_error=True)
        await driver.configure(space, (1 << 32)-1, expect_error=True)
    if CAPACITIES[1] & (CAPACITIES[1]-1):
        await driver.configure(2,driver.table_address(0,CAPACITIES[1]),expect_error=True)
    await driver.configure(0, 0, CAPACITIES[0]+1, expect_error=True)
    for index in range(CAPACITIES[0]):
        result = await driver.command(command("create"))
        assert result["value"] == {"kind": "view", "value": index}
    result = await driver.command(command("create"))
    assert result["status"] == {"kind": "failed", "value": {"error": "memory_full"}}
    await driver.reset()
    assert await driver.configure(0, 0) == 0 and await driver.configure(0, 1) == 0
    await driver.command(command("create"))
    await driver.command(command("append", view=0, samples=[MAX]))
    assert (await driver.command(command("read", view=0, position=0, length=1, frames=3)))["value"]["value"] == [MAX]*3
    async def admit(opcode, view=0, start=0, length=0, count=0):
        driver.set(cmd_valid=1, cmd_opcode=opcode, cmd_view=view, cmd_start=start,
                   cmd_length=length, cmd_count=count, write_valid=0, read_ready=0)
        before, after = await driver.tick()
        assert before["cmd_ready"]
        driver.set(cmd_valid=0)
        return after

    async def finish(after, code):
        for _ in range(1000):
            if after["done"]:
                break
            _, after = await driver.tick()
        else:
            raise AssertionError("protocol command did not finish")
        assert after["error_code"] == code
        await driver.tick()

    await finish(await admit(6, view=1 << 32, length=1, count=1), 1)
    original = await driver.snapshot()
    after = await admit(6, length=1, count=3)
    # In-flight metadata and samples are isolated from host/configuration traffic.
    driver.set(cfg_valid=1, cfg_write=1, cfg_space=0, cfg_address=0, cfg_write_data=0,
               cmd_valid=1, cmd_opcode=0, cmd_view=(1 << 64)-1, cmd_start=(1 << 64)-1,
               arena_base=driver.base+0x100000000)
    for _ in range(200):
        before, after = await driver.tick()
        assert not before["cfg_ready"] and not before["cmd_ready"]
        if after["read_valid"]:
            break
    else:
        raise AssertionError("read data never arrived")
    for _ in range(12):
        _, after = await driver.tick()
        assert after["read_valid"] and after["read_data"] == MAX
    driver.set(cfg_valid=0, cmd_valid=0, read_ready=1)
    samples = []
    for _ in range(1000):
        before, after = await driver.tick()
        if before["read_valid"]:
            samples.append(before["read_data"])
        if after["done"]:
            break
    assert samples == [MAX]*3 and after["error_code"] == 0
    driver.set(read_ready=0, arena_base=driver.base)
    await driver.tick()
    assert await driver.snapshot() == original, "blocked write changed ownership"
    driver.set(arena_base=driver.base+0x100000)
    await finish(await admit(6, length=1, count=1), 4)
    await finish(await admit(5, count=driver.chunk_frames), 4)
    assert not int(dut.memory_fault.value), "a rejected base change poisoned chunk memory"
    driver.set(arena_base=driver.base)
    recovered = await driver.command(command("read", view=0, position=0, length=1, frames=1))
    assert recovered["value"] == {"kind": "samples", "value": [MAX]}
    assert recovered["media"] == original, "rejected base change partially allocated media"
    await driver.load(original)
    driver.fail_next = True
    after = await admit(6, length=1, count=1)
    driver.set(read_ready=1)
    await finish(after, 5)
    driver.set(cmd_valid=1, cmd_opcode=0)
    for _ in range(5):
        before, _ = await driver.tick()
        assert not before["cmd_ready"], "memory fault must stop execution until reset"
    await driver.reset()
    assert driver.memory[0] == MAX, "reset cleared external samples"
    driver.set(arena_base=driver.base+1)
    await finish(await admit(0), 4)
    driver.set(arena_base=(1 << 64)-4)
    await finish(await admit(0), 4)
    driver.set(arena_base=driver.base)
    assert await driver.configure(0, 0) == 0
    driver.close()
