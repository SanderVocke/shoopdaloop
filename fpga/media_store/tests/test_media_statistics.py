"""Read-only allocator counters, sharing, exhaustion, and descriptor reuse."""
import os
import cocotb
from media_cases import command
from media_driver import Driver


def words(value):
    return tuple((value >> (32 * i)) & 0xffffffff for i in range(5))


@cocotb.test()
async def storage_statistics(dut):
    capacities = tuple(map(int, os.environ['MEDIA_CAPACITIES'].split(',')))
    driver = Driver(dut, capacities, int(os.environ['MEDIA_SEED']))
    reuse = bool(int(os.environ['MEDIA_REUSE_VIEWS']))
    assert driver.table_pages == 2 and driver.blocks <= 16
    await driver.reset()

    async def stats():
        return words(await driver.configure(6, 0))

    async def completed(kind, **fields):
        result = await driver.command(command(kind, **fields))
        assert result['status'] == {'kind': 'completed'}, result
        return result

    assert await stats() == (0, driver.blocks, 0, 2, 0)
    geometry = words(await driver.configure(6, 1))
    assert geometry[2:] == (driver.chunk_frames, driver.block_chunks, driver.views)
    assert (geometry[0] | geometry[1] << 32) == len(driver.memory) * 4
    await driver.configure(0, 3, 2)
    await completed('create')
    assert await stats() == (0, 2, 0, 2, 1)
    await completed('append', view=0, samples=[117])
    assert await stats() == (1, 2, 1, 2, 1)
    await completed('slice', source=0, start=0, frames=1)
    assert await stats() == (1, 2, 2, 2, 2)

    before = await driver.snapshot()
    refused = await driver.command(command('slice', source=0, start=0, frames=1))
    assert refused['status'] == {'kind': 'failed', 'value': {'error': 'memory_full'}}
    assert await stats() == (1, 2, 2, 2, 2)
    await driver.configure(6, 0, 0, expect_error=True)
    await driver.configure(6, 2, expect_error=True)
    assert await driver.snapshot() == before

    await completed('clear', view=0)
    assert await stats() == (1, 2, 1, 2, 2)
    await completed('remove', view=1)
    assert await stats() == (0, 2, 0, 2, 1 if reuse else 2)
    await completed('remove', view=0)
    assert await stats() == (0, 2, 0, 2, 0 if reuse else 2)
    await completed('create')
    assert await stats() == (0, 2, 0, 2, 1 if reuse else 3)
    await driver.reset()
    assert await stats() == (0, driver.blocks, 0, 2, 0)

    for view in range(4):
        await driver.configure(1, view, int(view == 3))
    await driver.configure(0, 0, 4)
    assert await stats() == (0, driver.blocks, 0, 2, 1 if reuse else 4)
    await driver.configure(1, 3, 0)
    assert await stats() == (0, driver.blocks, 0, 2, 0 if reuse else 4)
