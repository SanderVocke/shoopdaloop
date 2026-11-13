"""Independent scalar oracle, including histories that interleave across jobs."""
import random
import json
from pathlib import Path
import cocotb
from cocotb.triggers import Timer

MASK = (1 << 32) - 1
ONE = 1 << 24

class Driver:
    def __init__(self, dut):
        self.dut = dut
        self.ticks = 0
        self.last_cycles = 0

    def set(self, **values):
        for key, value in values.items():
            getattr(self.dut, key).value = value

    async def tick(self):
        self.ticks += 1
        self.dut.clk.value = 0
        await Timer(5, units='ns')
        ready = int(self.dut.job_ready.value)
        self.dut.clk.value = 1
        await Timer(5, units='ns')
        return ready

    async def reset(self):
        self.set(clk=0, reset=1, run_valid=0, job_valid=0, job_enable=1, job_data=0,
                 run_quantum=32, run_chunk_frames=1, run_buffers=8, run_inputs=1,
                 run_outputs=1, run_jobs=3, cfg_valid=0, cfg_space=0, cfg_write=0,
                 cfg_address=0, cfg_write_data=0, sample_input_data=0,
                 media_cmd_ready=0, media_done=0, media_error=0, media_fault=0,
                 media_write_ready=0, media_read_valid=0, media_read_data=0, media_hint_ready=0)
        await self.tick()
        self.set(reset=0)
        await self.tick()

    async def config(self, space, address, value=None):
        self.set(cfg_valid=1, cfg_space=space, cfg_address=address,
                 cfg_write=int(value is not None), cfg_write_data=(value or 0) & MASK)
        await self.tick()
        self.set(cfg_valid=0)
        result = int(self.dut.cfg_read_data.value)
        await self.tick()
        return result - (1 << 32) if result & (1 << 31) else result

    async def run(self, data, index, duration, playing=True, restart=False):
        for i, value in enumerate(data):
            await self.config(0, i, value)
        start = self.ticks
        self.set(run_quantum=len(data), run_valid=1)
        await self.tick()
        self.set(run_valid=0)
        jobs = [1 << 24, 10 | (1 << 8) | (2 << 24) | (index << 64) |
                (duration << 96) | (int(playing) << 128) | (int(restart) << 129), 6 | (2 << 8)]
        for packet in jobs:
            self.set(job_valid=1, job_data=packet)
            for _ in range(10000):
                if await self.tick():
                    break
            else:
                raise AssertionError('job admission timeout')
        self.set(job_valid=0)
        for cycles in range(10000):
            if int(self.dut.done.value):
                break
            await self.tick()
        else:
            raise AssertionError('smoothing timeout')
        self.last_cycles = self.ticks - start
        assert int(self.dut.error_code.value) == 0
        return [await self.config(1, i) for i in range(len(data))]

@cocotb.test()
async def histories_and_extremes(dut):
    driver = Driver(dut)
    rng = random.Random(983)
    measurements = []
    for reset_epoch in range(2):
        await driver.reset()
        history = {}
        for duration in [0, 1, 3, 31, 32, 33, 144, MASK]:
            for ordinal in range(12):
                index = [0, 7, 1][ordinal % 3]
                playing = ordinal != 5
                restart = ordinal < 3 or ordinal in (6, 10)
                # Updating the requested duration must not alter a ramp already running.
                requested = duration if restart else 17
                last, anchor, elapsed, active_duration = history.get(index, (0, 0, 0, 0))
                if not playing:
                    last = anchor = 0
                    elapsed = active_duration = requested
                elif restart:
                    anchor, elapsed, active_duration = last, 0, requested
                data = [-(1 << 31), (1 << 31) - 1, -1, 0, 1] + [rng.randint(-(1 << 31), (1 << 31)-1) for _ in range(27)]
                expected = []
                for raw in data:
                    if not playing:
                        result = 0
                    elif elapsed >= active_duration:
                        result = raw
                    else:
                        weight = (elapsed * ONE) // active_duration
                        product = (raw - anchor) * weight
                        result = anchor + (abs(product) // ONE) * (-1 if product < 0 else 1)
                    expected.append(result)
                    if playing:
                        last = result
                        elapsed = min(elapsed + 1, active_duration)
                history[index] = last, anchor, elapsed, active_duration
                actual = await driver.run(data, index, requested, playing, restart)
                assert actual == expected, (reset_epoch, duration, ordinal, actual, expected)
                assert driver.last_cycles < 1000, driver.last_cycles
                measurements.append(driver.last_cycles)
        # Continue the same ramp across short, irregular job boundaries.
        anchor = -987654
        assert await driver.run([anchor], 6, 0) == [anchor]
        elapsed = 0
        for ordinal, n in enumerate([1, 3, 17, 32]):
            expected = []
            for _ in range(n):
                weight = min(elapsed, 33) * ONE // 33
                expected.append(anchor + (12345 - anchor) * weight // ONE)
                elapsed += 1
            actual = await driver.run([12345] * n, 6, 33 if ordinal == 0 else 144,
                                      True, ordinal == 0)
            assert actual == expected, (reset_epoch, n, actual, expected)

    Path("smoothing-summary.json").write_text(json.dumps(dict(quanta=len(measurements), max_cycles=max(measurements), exact_samples=len(measurements)*32), indent=2)+"\n")
