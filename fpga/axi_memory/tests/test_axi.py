import json
import os
from pathlib import Path
import random

import cocotb
from cocotb.triggers import Timer
from options import create_memory, metadata


class Driver:
    def __init__(self, dut, word=False):
        self.dut, self.word = dut, word
        self.rng = random.Random(731)
        self.words = [0x12345678 ^ i for i in range(8192)]
        self.memory = create_memory(dut, self.words, seed=57, output=os.environ["AXI_OUTPUT"])
        self.memory.stalls = True
        self.width = self.memory.width

    def set(self, **values):
        for name, value in values.items():
            getattr(self.dut, name).value = value

    async def tick(self):
        self.dut.clk.value = 0
        self.memory.drive()
        await Timer(5, units="ns")
        if self.word:
            names = ("memory_ready", "memory_response_valid", "memory_response_error", "memory_read_data")
        else:
            names = ("command_ready", "write_ready", "read_valid", "read_data", "read_error", "read_last",
                     "completion_valid", "completion_error")
        before = {key: int(getattr(self.dut, key).value) for key in names}
        self.memory.step(self.memory.sample(), bool(int(self.dut.reset.value)))
        self.dut.clk.value = 1
        await Timer(5, units="ns")
        self.dut.clk.value = 0
        return before

    async def reset(self):
        if self.word:
            self.set(memory_valid=0, memory_write=0, memory_address=0, memory_write_data=0,
                     memory_response_ready=0, arena_base=0x1234000000, arena_bytes=len(self.words)*4,
                     physical_base=self.memory.base)
        else:
            self.set(command_valid=0, command_write=0, command_address=0, command_length=0,
                     command_size=2, write_valid=0, write_data=0, write_strobe=0,
                     read_ready=0, completion_ready=0)
        self.set(reset=1)
        await self.tick()
        self.set(reset=0)
        await self.tick()

    async def transaction(self, address, size, count, data=None, strobes=None, error=False):
        self.set(command_valid=1, command_write=int(data is not None), command_address=address,
                 command_length=count-1, command_size=size)
        for _ in range(10000):
            if (await self.tick())["command_ready"]:
                break
        else:
            raise AssertionError("command not accepted")
        self.set(command_valid=0)
        sent, received = 0, []
        held = None
        for _ in range(20000):
            offered = data is not None and sent < count
            ready = int(self.rng.randrange(3) != 0)
            self.set(write_valid=int(offered), write_data=data[sent] if offered else 0,
                     write_strobe=strobes[sent] if offered else 0, read_ready=ready, completion_ready=ready)
            before = await self.tick()
            if offered and before["write_ready"]:
                sent += 1
            valid = before["completion_valid"] if data is not None else before["read_valid"]
            payload = ((before["completion_error"],) if data is not None else
                       (before["read_data"], before["read_error"], before["read_last"]))
            if held is not None:
                assert valid and payload == held, "stalled endpoint response changed"
            held = payload if valid and not ready else None
            if valid and ready:
                if data is not None:
                    assert bool(before["completion_error"]) == error
                    break
                assert bool(before["read_error"]) == error
                received.append(before["read_data"])
                if before["read_last"]:
                    assert error or len(received) == count
                    break
        else:
            raise AssertionError("transaction did not complete")
        self.set(write_valid=0, read_ready=0, completion_ready=0)
        return received


@cocotb.test()
async def words_and_bursts(dut):
    driver = Driver(dut)
    await driver.reset()
    oracle = bytearray(b"".join(word.to_bytes(4, "little") for word in driver.words))
    for count in (1, 2, 16):
        for size in range(driver.width.bit_length()):
            address = driver.memory.base+4080 if count*(1 << size) <= 16 else driver.memory.base+4096
            for shift in (0, 128):
                start = address+shift
                # Include narrow beats in the upper lanes of a 64-bit bus.
                if count == 1 and (1 << size) < driver.width:
                    start += driver.width-(1 << size)
                data, strobes = [], []
                for index in range(count):
                    lane = (start+(index << size)) % driver.width
                    mask = ((1 << (1 << size))-1) << lane
                    strobe = mask & driver.rng.getrandbits(driver.width)
                    payload = driver.rng.getrandbits(driver.width*8)
                    data.append(payload)
                    strobes.append(strobe)
                    offset = start+(index << size)-driver.memory.base-lane
                    for byte in range(driver.width):
                        if strobe & (1 << byte):
                            oracle[offset+byte] = (payload >> (8*byte)) & 255
                await driver.transaction(start, size, count, data, strobes)
                result = await driver.transaction(start, size, count)
                for index, actual in enumerate(result):
                    offset = start+(index << size)-driver.memory.base
                    aligned = offset-offset % driver.width
                    assert actual == int.from_bytes(oracle[aligned:aligned+driver.width], "little")
    assert b"".join(word.to_bytes(4, "little") for word in driver.words) == oracle
    for write in (False, True):
        driver.memory.fail_next = True
        await driver.transaction(driver.memory.base, 2, 1, [123] if write else None, [15] if write else None, error=True)
        assert not driver.memory.fail_next
    for address, size, count in ((driver.memory.base+1, 2, 1), (driver.memory.base+4092, 2, 2),
                                 (driver.memory.base, 2, 17), (driver.memory.base, 4, 1)):
        for write in (False, True):
            requests = driver.memory.reads+driver.memory.writes
            await driver.transaction(address, size, count, [0]*count if write else None,
                                     [0]*count if write else None, error=True)
            assert driver.memory.reads+driver.memory.writes == requests
    memory = driver.memory
    for write in (False, True):
        expected = 0
        for record in memory.records:
            if record["write"] == write:
                first = record["address"]//memory.timing.native_bytes
                last = (record["address"]+((record["length"]+1) << record["size"])-1)//memory.timing.native_bytes
                expected += last-first+1
                assert len(record["beat_cycles"]) == (1 if write else record["length"]+1)
        assert expected == (memory.native_writes if write else memory.native_reads)
    report = driver.memory.report()
    if hasattr(driver.memory.timing, "report"):
        report["native"] = driver.memory.timing.report(final=True)
    report["configuration"] = metadata()
    Path(os.environ["AXI_OUTPUT"]).joinpath("axi-statistics.json").write_text(json.dumps(report, indent=2)+"\n")
    driver.memory.close()


@cocotb.test()
async def independent_channels_and_reset(dut):
    driver = Driver(dut)
    await driver.reset()
    driver.memory.stalls = False
    # W can arrive before AW, or AW before W. Neither may depend on the other.
    for hold_address in (True, False):
        driver.memory.hold_aw = hold_address
        driver.memory.hold_w = not hold_address
        driver.set(command_valid=1, command_write=1, command_address=driver.memory.base,
                   command_size=2, command_length=0)
        assert (await driver.tick())["command_ready"]
        driver.set(command_valid=0, write_valid=1, write_data=0xdeadbeef, write_strobe=15, completion_ready=0)
        sent = False
        for _ in range(8):
            if (await driver.tick())["write_ready"]:
                sent = True
                driver.set(write_valid=0)
        assert sent == hold_address
        assert (driver.memory.active is None) == hold_address
        driver.memory.hold_aw = driver.memory.hold_w = False
        driver.set(write_valid=int(not sent))
        for _ in range(1000):
            before = await driver.tick()
            if before["write_ready"]:
                driver.set(write_valid=0)
            if before["completion_valid"]:
                break
        else:
            raise AssertionError("independent AW/W stalled")
        for _ in range(5):
            assert (await driver.tick())["completion_valid"]
        driver.set(completion_ready=1)
        await driver.tick()
    driver.memory.pause_responses = True
    driver.set(completion_ready=0, command_valid=1, command_write=0)
    assert (await driver.tick())["command_ready"]
    driver.set(command_valid=0)
    for _ in range(8):
        await driver.tick()
    assert driver.memory.pending is not None
    await driver.reset()
    assert driver.memory.pending is None
    driver.memory.pause_responses = False
    assert (await driver.transaction(driver.memory.base, 2, 1))[0] & 0xffffffff == 0xdeadbeef
    driver.memory.close()
