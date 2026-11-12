import json
import os
from pathlib import Path
import random
import cocotb
import fpga_testbench as wire
from chunk_driver import Driver, ERRORS, signed

CAPACITIES = tuple(map(int, os.environ["CHUNK_CAPACITIES"].split(",")))
REFERENCE = Path(os.environ["CHUNK_REFERENCE"])
OUTPUT = Path(os.environ["CHUNK_OUTPUT"])


def document(kind, value):
    return {"format_version": 2, "payload": {"kind": kind, "value": value}}


async def compare(dut, operations, seed, case=None):
    rng = random.Random(seed)
    slots, frames, _, _ = CAPACITIES
    if case is None:
        case = {"name": f"chunks-{seed}", "chunk_frames": frames,
            "samples": [rng.randint(-(1 << 31), (1 << 31)-1) for _ in range(slots*frames)],
            "requests": [{"id": (0xffffffff-index) & 0xffffffff, "operation": op}
                         for index, op in enumerate([*operations, {"kind": "fence"}])]}
    assert case["chunk_frames"] == frames and len(case["samples"]) == slots*frames
    trace = wire.run_reference("trace", document("chunk_case", case), executable=REFERENCE)
    wire.run_reference("check", trace, executable=REFERENCE)
    driver = Driver(dut, CAPACITIES, seed)
    await driver.reset(case["samples"])
    try:
        for request, expected in zip(case["requests"], trace["payload"]["value"]["results"]):
            code, samples = await driver.command(request["operation"], request["id"])
            status = {"kind": "completed"} if code == 0 else {"kind": "failed", "value": {"error": ERRORS[code]}}
            assert status == expected["status"]
            assert samples == expected["samples"]
            if request["operation"]["kind"] == "fence":
                assert [signed(x) for x in await driver.read_memory()] == expected["memory"]
        assert await driver.command({"kind": "fence"}) == (0, [])
        results = trace["payload"]["value"]["results"]
        assert [signed(x) for x in await driver.read_memory()] == (results[-1]["memory"] if results else case["samples"])
        assert driver.burst is None and driver.pending is None
    except BaseException:
        wire.dump(document("chunk_case", case), OUTPUT / "failure.case.json")
        wire.dump(trace, OUTPUT / "failure.expected.trace.json")
        raise
    finally:
        driver.close()


@cocotb.test()
async def conformance(dut):
    if os.environ.get("CHUNK_CASE"):
        payload = wire.load(os.environ["CHUNK_CASE"])["payload"]
        value = payload["value"]["case"] if payload["kind"] == "chunk_trace" else payload["value"]
        await compare(dut, [], 20261111, case=value)
        return
    slots, frames, _, _ = CAPACITIES
    operations = []
    for chunk in range(slots):
        operations += [{"kind": "write", "chunk": chunk, "offset": 0, "samples": [chunk*19-i for i in range(frames)]}]
    operations += [{"kind": "read", "chunk": 0, "offset": 0, "frames": frames},
                   {"kind": "prefetch", "chunk": slots-1},
                   {"kind": "write", "chunk": 0, "offset": frames-1, "samples": [-2147483648]},
                   {"kind": "retire", "chunk": 0},
                   {"kind": "write", "chunk": 0, "offset": 0, "samples": [2147483647]},
                   {"kind": "read", "chunk": 0, "offset": 0, "frames": frames},
                   {"kind": "read", "chunk": 0, "offset": frames, "frames": 0},
                   {"kind": "write", "chunk": 0, "offset": frames, "samples": []},
                   {"kind": "read", "chunk": 0, "offset": 0xffffffff, "frames": 0},
                   {"kind": "read", "chunk": 0, "offset": 0, "frames": 0xffffffff},
                   {"kind": "read", "chunk": 0xffffffff, "offset": 0, "frames": 0},
                   {"kind": "prefetch", "chunk": 0xffffffff}, {"kind": "retire", "chunk": slots}]
    await compare(dut, operations, 20261111)
    count = int(os.environ["CHUNK_RANDOM_CASES"])
    for seed in range(count):
        rng = random.Random(seed)
        operations = []
        for _ in range(100):
            kind = rng.choice(("read", "read", "write", "write", "prefetch", "retire", "fence"))
            op = {"kind": kind}
            if kind != "fence":
                op["chunk"] = rng.randrange(slots)
            if kind in ("read", "write"):
                op["offset"] = rng.randrange(frames+1)
                words = rng.randrange(frames-op["offset"]+1)
                if kind == "read":
                    op["frames"] = words
                else:
                    op["samples"] = [rng.randint(-(1 << 31), (1 << 31)-1) for _ in range(words)]
            operations.append(op)
        await compare(dut, operations, seed)
    (OUTPUT / "summary.json").write_text(json.dumps({"cases": count+1, "random_requests": count*100}, indent=2)+"\n")


@cocotb.test()
async def buffering_protocol(dut):
    slots, frames, _, _ = CAPACITIES
    driver = Driver(dut, CAPACITIES, 3)
    driver.stalls = False
    initial = list(range(slots*frames))
    await driver.reset(initial)
    driver.pause = True
    payload = [-71+i for i in range(frames)]
    assert await driver.command({"kind": "write", "chunk": 0, "offset": 0, "samples": payload}, 1, stalls=False) == (0, [])
    assert driver.writes == 0, "write completion waited for backing DDR"
    assert await driver.command({"kind": "read", "chunk": 0, "offset": 0, "frames": frames}, 2, stalls=False) == (0, payload)
    assert driver.reads == 0, "buffered read accessed DDR"
    for _ in range(frames+10):
        await driver.tick()
    await driver.hint(slots-1)
    # A fence must hold until the stalled writeback completes.
    driver.set(cmd_valid=1, cmd_opcode=3, cmd_id=77, cmd_chunk=0, cmd_offset=0, cmd_count=0, response_ready=0)
    for _ in range(1000):
        before, _ = await driver.tick()
        if before["cmd_ready"]:
            break
    else:
        raise AssertionError("fence not admitted")
    driver.set(cmd_valid=0)
    for _ in range(12):
        _, after = await driver.tick()
        assert not after["response_valid"]
    driver.pause = False
    for _ in range(10000):
        _, after = await driver.tick()
        if after["response_valid"]:
            break
    else:
        raise AssertionError("fence did not drain")
    for _ in range(12):
        before, _ = await driver.tick()
        assert before["response_valid"] and before["response_id"] == 77
    driver.set(response_ready=1)
    await driver.tick()
    driver.set(response_ready=0)
    assert [signed(x) for x in (await driver.read_memory())[:frames]] == payload
    assert await driver.command({"kind": "retire", "chunk": 0}, 3) == (0, [])
    assert await driver.command({"kind": "write", "chunk": 0, "offset": 0, "samples": [91]}, 4) == (0, [])
    assert await driver.command({"kind": "read", "chunk": 0, "offset": 0, "frames": frames}, 5) == (0, [91]+payload[1:])
    assert await driver.command({"kind": "fence"}, 6) == (0, [])
    await driver.hint(slots-1)
    for _ in range(10000):
        await driver.tick()
        if not int(dut.busy.value):
            break
    reads = driver.reads
    assert await driver.command({"kind": "read", "chunk": slots-1, "offset": 0, "frames": frames}, 7) == (0, initial[-frames:])
    assert driver.reads == reads, "prefetched chunk missed"

    # Supersede an accepted speculative read while its old response is withheld.
    await driver.reset(initial)
    driver.stalls = False
    await driver.hint(0)
    for _ in range(20):
        await driver.tick()
        if driver.pending is not None:
            break
    else:
        raise AssertionError("early prefetch did not bind the arena and start")
    driver.pause = True
    old_reads = driver.reads
    old_writes = driver.writes
    assert await driver.command({"kind": "write", "chunk": 0, "offset": 0, "samples": payload}, 70, stalls=False) == (0, [])
    assert await driver.command({"kind": "read", "chunk": 0, "offset": 0, "frames": frames}, 71, stalls=False) == (0, payload)
    replacement = [109-i for i in range(frames)]
    assert await driver.command({"kind": "write", "chunk": 0, "offset": 0, "samples": replacement}, 72, stalls=False) == (0, [])
    assert driver.reads == old_reads and driver.writes == old_writes
    driver.pause = False
    assert await driver.command({"kind": "fence"}, 73, stalls=False) == (0, [])
    assert await driver.command({"kind": "read", "chunk": 0, "offset": 0, "frames": frames}, 74, stalls=False) == (0, replacement)
    assert [signed(x) for x in (await driver.read_memory())[:frames]] == replacement

    # A completed prefetch is invalidated, and a queued one releases its credit.
    await driver.reset(initial)
    await driver.hint(0)
    for _ in range(10000):
        await driver.tick()
        if not int(dut.busy.value):
            break
    else:
        raise AssertionError("prefetch did not complete")
    old_reads = driver.reads
    driver.pause = True
    assert await driver.command({"kind": "write", "chunk": 0, "offset": 0, "samples": payload}, 75, stalls=False) == (0, [])
    assert await driver.command({"kind": "read", "chunk": 0, "offset": 0, "frames": frames}, 76, stalls=False) == (0, payload)
    assert driver.reads == old_reads
    driver.pause = False
    assert await driver.command({"kind": "fence"}, 77, stalls=False) == (0, [])

    await driver.reset(initial)
    await driver.hint(1)
    for _ in range(20):
        await driver.tick()
        if driver.pending is not None:
            break
    else:
        raise AssertionError("blocking transfer did not start")
    driver.pause = True
    await driver.hint(0)
    assert await driver.command({"kind": "write", "chunk": 0, "offset": 0, "samples": payload}, 78, stalls=False) == (0, [])
    await driver.hint(2)
    assert await driver.command({"kind": "read", "chunk": 0, "offset": 0, "frames": frames}, 79, stalls=False) == (0, payload)
    driver.pause = False
    assert await driver.command({"kind": "fence"}, 80, stalls=False) == (0, [])
    assert await driver.command({"kind": "retire", "chunk": 0}, 81, stalls=False) == (0, [])
    assert await driver.command({"kind": "write", "chunk": 0, "offset": 0, "samples": replacement}, 82, stalls=False) == (0, [])
    assert await driver.command({"kind": "read", "chunk": 0, "offset": 0, "frames": frames}, 83, stalls=False) == (0, replacement)

    # Retirement waits for the old allocation's in-flight writeback.
    await driver.reset(initial)
    driver.pause = True
    assert await driver.command({"kind": "write", "chunk": 0, "offset": 0, "samples": payload}, 8) == (0, [])
    for _ in range(frames+12):
        await driver.tick()
    await driver.hint(0)
    driver.set(cmd_valid=1, cmd_opcode=4, cmd_id=88, cmd_chunk=0, cmd_offset=0, cmd_count=0, response_ready=0)
    before, _ = await driver.tick()
    assert before["cmd_ready"]
    driver.set(cmd_valid=0)
    for _ in range(12):
        _, after = await driver.tick()
        assert not after["response_valid"]
    driver.pause = False
    for _ in range(10000):
        _, after = await driver.tick()
        if after["response_valid"]:
            break
    else:
        raise AssertionError("retirement did not drain the old writeback")
    assert after["response_id"] == 88 and after["response_error"] == 0
    driver.set(response_ready=1)
    await driver.tick()
    driver.set(response_ready=0)
    assert driver.pending is None and not int(dut.busy.value)
    assert await driver.command({"kind": "write", "chunk": 0, "offset": 0, "samples": [92]}, 9) == (0, [])
    assert await driver.command({"kind": "read", "chunk": 0, "offset": 0, "frames": frames}, 10) == (0, [92]+payload[1:])
    assert await driver.command({"kind": "fence"}, 11) == (0, [])

    # A full dirty pool backpressures a new chunk before command acceptance.
    await driver.reset(initial)
    driver.pause = True
    for chunk in range(driver.write_buffers):
        assert await driver.command({"kind": "write", "chunk": chunk, "offset": 0, "samples": payload}, 20+chunk) == (0, [])
    driver.set(cmd_valid=1, cmd_opcode=1, cmd_id=99, cmd_chunk=driver.write_buffers, cmd_offset=0, cmd_count=frames)
    for _ in range(12):
        before, _ = await driver.tick()
        assert not before["cmd_ready"] and not before["write_ready"]
    driver.set(cmd_valid=0)
    driver.pause = False
    assert await driver.command({"kind": "fence"}, 30) == (0, [])

    # Read data stays pinned while a different chunk is prefetched.
    await driver.reset(initial)
    driver.set(cmd_valid=1, cmd_opcode=0, cmd_id=100, cmd_chunk=0, cmd_offset=0, cmd_count=frames, read_ready=0)
    before, _ = await driver.tick()
    assert before["cmd_ready"]
    driver.set(cmd_valid=0)
    for _ in range(10000):
        _, after = await driver.tick()
        if after["read_valid"]:
            break
    else:
        raise AssertionError("read did not produce data")
    await driver.hint(slots-1)
    for _ in range(2*frames+10):
        _, after = await driver.tick()
        assert after["read_valid"] and signed(after["read_data"]) == initial[0]
    observed = []
    driver.set(read_ready=1, response_ready=1)
    for _ in range(10000):
        before, _ = await driver.tick()
        if before["read_valid"]:
            observed.append(signed(before["read_data"]))
        if before["response_valid"]:
            break
    else:
        raise AssertionError("pinned read did not finish")
    assert observed == initial[:frames]
    driver.set(read_ready=0, response_ready=0)
    assert await driver.command({"kind": "fence"}, 101) == (0, [])
    driver.close()


@cocotb.test()
async def faults_and_reset(dut):
    driver = Driver(dut, CAPACITIES, 4)
    await driver.reset([5]*len(driver.memory))
    driver.fail_next = True
    code, _ = await driver.command({"kind": "read", "chunk": 0, "offset": 0, "frames": 1}, 10)
    assert code == 5 and int(dut.fault.value) and not int(dut.cmd_ready.value)
    await driver.reset([5]*len(driver.memory))
    driver.pause = True
    assert await driver.command({"kind": "write", "chunk": 0, "offset": 0, "samples": [8]*driver.frames}, 11) == (0, [])
    driver.fail_next = True
    driver.pause = False
    for _ in range(10000):
        await driver.tick()
        if int(dut.fault.value):
            break
    else:
        raise AssertionError("posted writeback error was lost")
    assert not int(dut.cmd_ready.value), "fault admitted further work"
    await driver.reset([9]*len(driver.memory))
    assert await driver.command({"kind": "read", "chunk": 0, "offset": 0, "frames": 1}, 12) == (0, [9])
    driver.set(arena_base=driver.base+4)
    assert await driver.command({"kind": "read", "chunk": 0, "offset": 0, "frames": 1}, 13) == (4, [])
    driver.set(arena_base=driver.base)
    assert await driver.command({"kind": "read", "chunk": 0, "offset": 0, "frames": 1}, 14) == (0, [9])
    await driver.reset([9]*len(driver.memory))
    assert await driver.command({"kind": "prefetch", "chunk": 0}, 15) == (0, [])
    for _ in range(10000):
        await driver.tick()
        if driver.pending is not None:
            break
    else:
        raise AssertionError("prefetch did not start a transfer")
    driver.pause = True
    await driver.reset([11]*len(driver.memory))
    driver.pause = False
    assert await driver.command({"kind": "read", "chunk": 0, "offset": 0, "frames": 1}, 16) == (0, [11])
    driver.close()


@cocotb.test()
async def read_stream_protocol(dut):
    driver = Driver(dut, CAPACITIES, 791)
    driver.stalls = False
    initial = [signed((0x81234567+0x1020305*i) & 0xffffffff) for i in range(len(driver.memory))]

    async def read_span(offset, count, expected, stall_at=None):
        driver.set(cmd_valid=1, cmd_opcode=0, cmd_id=123, cmd_chunk=0,
                   cmd_offset=offset, cmd_count=count, read_ready=0, response_ready=0)
        before, _ = await driver.tick()
        assert before['cmd_ready']
        driver.set(cmd_valid=0, cmd_offset=0xffffffff, cmd_count=0xffffffff)
        samples, clocks, held = [], [], None
        stalled, hold = False, 0
        for _ in range(10000):
            if len(samples) == stall_at and not stalled:
                stalled, hold = True, 7
            ready = hold == 0
            hold = max(0, hold-1)
            driver.set(read_ready=int(ready))
            before, _ = await driver.tick()
            payload = (before['read_data'], before['read_last'])
            if held is not None:
                assert before['read_valid'] and payload == held, 'stalled read changed'
            held = payload if before['read_valid'] and not ready else None
            if before['read_valid'] and ready:
                samples.append(signed(before['read_data']))
                clocks.append(driver.cycles)
                assert bool(before['read_last']) == (len(samples) == count)
            if before['response_valid']:
                assert not before['read_valid'] and len(samples) == count
                break
        else:
            raise AssertionError('streamed read timed out')
        assert samples == expected
        if stall_at is None:
            assert all(b-a == 1 for a,b in zip(clocks, clocks[1:])), 'resident read did not sustain one sample per clock'
        driver.set(read_ready=0)
        for _ in range(5):
            before, _ = await driver.tick()
            assert before['response_valid'] and before['response_id'] == 123 and before['response_error'] == 0
            assert not before['read_valid'] and not before['cmd_ready']
        driver.set(response_ready=1)
        await driver.tick()
        driver.set(response_ready=0)

    for forwarded in (False, True):
        driver.pause = False
        await driver.reset(initial)
        expected = initial[:driver.frames]
        if forwarded:
            driver.pause = True
            expected = list(reversed(expected))
            assert await driver.command({'kind':'write', 'chunk':0, 'offset':0, 'samples':expected}, stalls=False) == (0, [])
        else:
            assert await driver.command({'kind':'read', 'chunk':0, 'offset':0, 'frames':driver.frames}, stalls=False) == (0, expected)
            driver.pause = True
        for offset, count in sorted({(0, 0), (0, 1), (0, driver.frames), (1, driver.frames-1)}):
            await read_span(offset, count, expected[offset:offset+count])
        for stall_at in range(driver.frames):
            await read_span(0, driver.frames, expected, stall_at)
        driver.pause = False
        assert await driver.command({'kind':'fence'}, stalls=False) == (0, [])

    for delay in range(min(driver.frames+6, 10)):
        await driver.reset(initial)
        assert await driver.command({'kind':'read', 'chunk':0, 'offset':0, 'frames':driver.frames}, stalls=False) == (0, initial[:driver.frames])
        driver.set(cmd_valid=1, cmd_opcode=0, cmd_id=456, cmd_chunk=0, cmd_offset=0,
                   cmd_count=driver.frames, read_ready=delay % 2, response_ready=0)
        assert (await driver.tick())[0]['cmd_ready']
        driver.set(cmd_valid=0)
        for _ in range(delay):
            await driver.tick()
        replacement = [signed((x+delay+91) & 0xffffffff) for x in initial]
        await driver.reset(replacement)
        assert not int(dut.read_valid.value) and not int(dut.response_valid.value)
        assert await driver.command({'kind':'read', 'chunk':0, 'offset':0, 'frames':driver.frames}, stalls=False) == (0, replacement[:driver.frames])
    driver.close()
