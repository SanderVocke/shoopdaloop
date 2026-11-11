import cocotb
from test_axi import Driver


@cocotb.test()
async def word_adapter(dut):
    driver = Driver(dut, word=True)
    driver.memory.word_only = True
    await driver.reset()

    async def access(offset, data=None, error=False):
        driver.set(memory_valid=1, memory_write=int(data is not None),
                   memory_address=0x1234000000+offset, memory_write_data=data or 0,
                   memory_response_ready=0)
        for _ in range(10000):
            if (await driver.tick())["memory_ready"]:
                break
        else:
            raise AssertionError("word not admitted")
        driver.set(memory_valid=0)
        for _ in range(20000):
            before = await driver.tick()
            if before["memory_response_valid"]:
                break
        else:
            raise AssertionError("word response timed out")
        for _ in range(4):
            held = await driver.tick()
            assert held["memory_response_valid"] and held["memory_read_data"] == before["memory_read_data"]
        assert bool(before["memory_response_error"]) == error
        driver.set(memory_response_ready=1)
        await driver.tick()
        return before["memory_read_data"]

    for offset in (0, 4, 12, 28, 4092, 4096, len(driver.words)*4-4):
        value = driver.rng.getrandbits(32)
        await access(offset, value)
        assert await access(offset) == value
    for write in (False, True):
        driver.memory.fail_next = True
        await access(4, 71 if write else None, error=True)
    for offset in (-4, 1, len(driver.words)*4):
        requests = driver.memory.reads+driver.memory.writes
        await access(offset, error=True)
        assert driver.memory.reads+driver.memory.writes == requests
    driver.set(physical_base=0xfffffffc)
    await access(4, error=True)
    driver.memory.close()
