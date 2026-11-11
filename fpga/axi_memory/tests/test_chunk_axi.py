import cocotb
from cocotb.triggers import Timer
from test_axi import Driver


class ChunkDriver(Driver):
    def __init__(self, dut):
        super().__init__(dut, word=True)
        self.error_index = None

    async def tick(self):
        self.dut.clk.value = 0
        self.memory.drive()
        active = self.memory.active
        if (self.error_index is not None and active and not active["write"]
                and active["index"] == self.error_index and self.memory.driven["rvalid"]):
            self.dut.m_axi_rresp.value = 2
        await Timer(5, units="ns")
        before = {key: int(getattr(self.dut, key).value) for key in (
            "memory_ready", "memory_write_ready", "memory_read_valid", "memory_read_data",
            "memory_response_valid", "memory_response_error")}
        self.memory.step(self.memory.sample(), bool(int(self.dut.reset.value)))
        self.dut.clk.value = 1
        await Timer(5, units="ns")
        self.dut.clk.value = 0
        return before

    async def reset(self):
        self.logical_base = 0x1234000000
        self.set(memory_count=0, memory_write_valid=0, memory_read_ready=0)
        await super().reset()

    async def begin(self, offset, count, write):
        self.set(memory_valid=1, memory_write=int(write), memory_address=self.logical_base+offset,
                 memory_count=count, memory_response_ready=0)
        for _ in range(10000):
            if (await self.tick())["memory_ready"]:
                break
        else:
            raise AssertionError("chunk descriptor not admitted")
        self.set(memory_valid=0, memory_count=0, memory_address=0xffffffffffffffff,
                 memory_write=int(not write))

    async def finish(self, count, data=None, error=False, stop_after=None, release_channels=False):
        sent, received, held, offered = 0, [], None, False
        for cycle in range(100000):
            if release_channels and cycle == 30:
                self.memory.hold_aw = self.memory.hold_w = False
            offered = offered or (data is not None and sent < len(data) and self.rng.randrange(3) != 0)
            ready = int(self.rng.randrange(3) != 0 and (stop_after is None or len(received) < stop_after))
            self.set(memory_write_valid=int(offered), memory_write_data=data[sent] if offered else 0,
                     memory_read_ready=ready)
            before = await self.tick()
            if offered and before["memory_write_ready"]:
                sent += 1
                offered = False
            if held is not None:
                assert before["memory_read_valid"] and before["memory_read_data"] == held
            held = before["memory_read_data"] if before["memory_read_valid"] and not ready else None
            if before["memory_read_valid"] and ready:
                received.append(before["memory_read_data"])
            if before["memory_response_valid"]:
                assert bool(before["memory_response_error"]) == error
                assert self.memory.active is None, "completion preceded AXI retirement"
                if not error:
                    assert (sent if data is not None else len(received)) == count
                break
        else:
            raise AssertionError("chunk transfer timed out")
        self.set(memory_write_valid=0, memory_read_ready=0)
        for _ in range(5):
            before = await self.tick()
            assert before["memory_response_valid"] and bool(before["memory_response_error"]) == error
            assert not before["memory_ready"]
        self.set(memory_response_ready=1)
        await self.tick()
        return received

    async def access(self, offset, count, data=None, **kwargs):
        await self.begin(offset, count, data is not None)
        return await self.finish(count, data, **kwargs)


@cocotb.test()
async def packing_splitting_and_backpressure(dut):
    driver = ChunkDriver(dut)
    await driver.reset()
    driver.logical_base = 0x1234000124
    driver.set(arena_base=driver.logical_base)
    oracle = list(driver.words)
    for offset, count in ((0, 1), (4, 2), (0, 3), (4, 16), (0, 17), (4, 31),
                          (0, 32), (0, 33), (4, 64), (4092, 64), (4096, 1031),
                          (len(oracle)*4-132, 33)):
        data = [driver.rng.getrandbits(32) for _ in range(count)]
        start = len(driver.memory.records)
        await driver.access(offset, count, data)
        oracle[offset//4:offset//4+count] = data
        assert driver.words == oracle, "write changed adjacent sample lanes"
        assert await driver.access(offset, count) == data
        for write in (False, True):
            records = [r for r in driver.memory.records[start:] if r["write"] == write]
            assert sum((r["length"]+1) << r["size"] for r in records) == count*4
            address = driver.memory.base+offset
            for record in records:
                assert record["address"] == address
                address += (record["length"]+1) << record["size"]
                assert record["length"] < 16
                assert record["address"]//4096 == (address-1)//4096
            if offset == 0 and count == 32:
                assert len(records) == (1 if driver.width == 8 else 2)
                assert all(r["length"] == 15 and r["size"] == (3 if driver.width == 8 else 2) for r in records)
    # Exercise independent AW/W backpressure with a full burst.
    for hold_address in (True, False):
        driver.memory.hold_aw = hold_address
        driver.memory.hold_w = not hold_address
        data = [driver.rng.getrandbits(32) for _ in range(32)]
        await driver.access(0, 32, data, release_channels=True)
        assert await driver.access(0, 32) == data
    driver.memory.close()


@cocotb.test()
async def bounds_fault_drain_and_reset(dut):
    driver = ChunkDriver(dut)
    await driver.reset()
    for offset, count in ((0, 0), (-4, 1), (1, 1), (len(driver.words)*4-4, 2), (0, 0xffffffff)):
        for write in (False, True):
            requests = driver.memory.reads+driver.memory.writes
            await driver.access(offset, count, [] if write else None, error=True)
            assert driver.memory.reads+driver.memory.writes == requests
    driver.set(physical_base=0xfffffff8)
    await driver.access(0, 3, error=True)
    await driver.reset()
    driver.set(arena_base=0xfffffffffffffffc, arena_bytes=16)
    driver.set(memory_valid=1, memory_write=0, memory_address=0xfffffffffffffffc, memory_count=2)
    assert (await driver.tick())["memory_ready"]
    driver.set(memory_valid=0)
    await driver.finish(2, error=True)
    await driver.reset()
    for write in (False, True):
        requests = driver.memory.reads+driver.memory.writes
        driver.memory.fail_next = True
        await driver.access(0, 64, [91]*64 if write else None, error=True, stop_after=0)
        assert driver.memory.reads+driver.memory.writes == requests+1, "issued bursts after a failed burst"
    # An error in the middle of a burst must drain R even if the sample consumer stops.
    driver.error_index = 3
    prefix = 3*(driver.width//4)
    assert await driver.access(0, 64, error=True, stop_after=prefix) == driver.words[:prefix]
    driver.error_index = None
    assert await driver.access(0, 32) == driver.words[:32]
    # Reset cancels an outstanding read, a half-packed write, and an unretired B response.
    for write, packed in ((False, False), (True, False), (True, True)):
        driver.memory.pause_responses = True
        await driver.begin(0, 32, write)
        sent = 0
        for _ in range(200):
            offer = write and (packed or sent == 0)
            driver.set(memory_write_valid=int(offer), memory_write_data=0xabcdef01)
            if (await driver.tick())["memory_write_ready"] and offer:
                sent += 1
        assert driver.memory.pending is not None
        await driver.reset()
        assert driver.memory.pending is None
        driver.memory.pause_responses = False
        data = [driver.rng.getrandbits(32) for _ in range(32)]
        await driver.access(0, 32, data)
        assert await driver.access(0, 32) == data
    driver.memory.close()
