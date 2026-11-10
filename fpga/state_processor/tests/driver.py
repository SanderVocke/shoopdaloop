"""Cycle-level transport and handshake checks; architectural behavior comes from Rust."""

import random

from cocotb.triggers import Timer

from codec import decode_job, decode_status, decode_value, encode_instruction, encode_value


class Driver:
    SIGNALS = ("busy", "done", "error_code", "instruction_pc", "run_ready",
               "cfg_ready", "cfg_response_valid", "cfg_response_error", "cfg_read_data",
               "job_valid", "job_ready", "job_data", "queued_jobs")

    def __init__(self, dut, depths, seed):
        self.dut = dut
        self.program_depth, self.state_depth, self.job_depth = depths
        self.rng = random.Random(seed)
        self.held_job = None
        self.cycles = 0
        self.last_run_cycles = None

    def set(self, **signals):
        for name, value in signals.items():
            getattr(self.dut, name).value = value

    def sample(self):
        return {name: int(getattr(self.dut, name).value) for name in self.SIGNALS}

    async def tick(self):
        self.dut.clk.value = 0
        await Timer(5, units="ns")
        before = self.sample()
        resetting = int(self.dut.reset.value)
        if resetting:
            self.held_job = None
        else:
            assert before["queued_jobs"] <= self.job_depth, "FIFO occupancy overflow"
            if self.held_job is not None:
                assert before["job_valid"] and before["job_data"] == self.held_job, \
                    "stalled job disappeared or changed"
            self.held_job = (before["job_data"]
                             if before["job_valid"] and not before["job_ready"] else None)
            if before["busy"]:
                assert not (before["cfg_ready"] or before["run_ready"] or before["job_valid"]), \
                    "configuration, restart, or FIFO drain admitted while busy"
        self.dut.clk.value = 1
        await Timer(5, units="ns")
        after = self.sample()
        self.dut.clk.value = 0
        self.cycles += 1
        return before, after

    async def reset(self):
        self.set(clk=0, reset=1, run_valid=0, run_instruction_count=0, run_state_count=0,
                 run_quantum=0, run_job_budget=0, cfg_valid=0, cfg_state=0,
                 cfg_write=0, cfg_address=0, cfg_write_data=0, job_ready=0,
                 loop_read_ready=0, loop_response_valid=0, loop_response_error=0, loop_response_data=0)
        await self.tick()
        _, after = await self.tick()
        assert not after["busy"] and after["queued_jobs"] == 0
        assert not after["cfg_response_valid"] and not after["job_valid"]
        self.set(reset=0)
        await self.tick()

    async def configure(self, state, address, value=None, *, expect_error=False):
        self.set(cfg_valid=1, cfg_state=int(state), cfg_write=int(value is not None),
                 cfg_address=address, cfg_write_data=0 if value is None else value)
        for _ in range(100):
            before, after = await self.tick()
            if before["cfg_ready"]:
                break
        else:
            raise AssertionError("configuration acceptance timed out")
        self.set(cfg_valid=0)
        assert after["cfg_response_valid"], "missing synchronous RAM response"
        assert after["cfg_response_error"] == expect_error, "incorrect configuration status"
        data = after["cfg_read_data"]
        if value is not None or expect_error:
            assert data == 0, "write/error response was not zero"
        _, cleared = await self.tick()
        assert not cleared["cfg_response_valid"], "configuration response did not pulse"
        return data

    async def load(self, case):
        assert case["queue_capacity"] == self.job_depth, "case and RTL FIFO capacities differ"
        self.program = case["program"]
        self.state_count = len(case["initial_state"]["memory"])
        assert len(self.program["instructions"]) <= self.program_depth
        assert self.state_count <= self.state_depth
        encoded = [encode_instruction(item) for item in self.program["instructions"]]
        await self.reset()
        for index, value in enumerate(encoded):
            await self.configure(False, index, value)
            assert await self.configure(False, index) == value, "program RAM readback differs"
        for index, value in enumerate(case["initial_state"]["memory"]):
            packed = encode_value(value)
            await self.configure(True, index, packed)
            assert await self.configure(True, index) == packed, "state RAM readback differs"

    async def snapshot(self):
        return {"memory": [decode_value(await self.configure(True, index))
                           for index in range(self.state_count)]}

    def run_inputs(self, **overrides):
        values = dict(run_valid=1, run_instruction_count=len(self.program["instructions"]),
                      run_state_count=self.state_count, run_quantum=self.program["quantum"],
                      run_job_budget=self.program["jobs_per_quantum"])
        values.update(overrides)
        self.set(**values)

    async def start(self, **overrides):
        self.run_inputs(**overrides)
        for _ in range(100):
            before, after = await self.tick()
            if before["run_ready"]:
                self.set(run_valid=0)
                return after
        raise AssertionError("run acceptance timed out")

    async def execute(self, **overrides):
        self.set(job_ready=0)
        after = await self.start(**overrides)
        accepted_cycle = self.cycles
        for _ in range(20 * self.program_depth + 20):
            if after["done"]:
                break
            self.set(job_ready=self.rng.randrange(2))
            _, after = await self.tick()
        else:
            raise AssertionError("execution completion timed out")
        self.last_run_cycles = self.cycles - accepted_cycle
        self.set(job_ready=0)
        assert not after["busy"], "done asserted before engine became idle"
        status = after["error_code"]
        if status == 0:
            assert after["instruction_pc"] == len(self.program["instructions"])
        _, cleared = await self.tick()
        assert not cleared["done"], "done did not pulse"
        assert cleared["error_code"] == status, "result status was not retained"
        return status

    async def drain(self):
        jobs = []
        expected_count = int(self.dut.queued_jobs.value)
        self.set(job_ready=0)
        for _ in range(3):
            _, after = await self.tick()
            assert after["queued_jobs"] == expected_count, "FIFO changed while stalled"
        for cycle in range(12 * self.job_depth + 30):
            self.set(job_ready=int(cycle % 7 == 6 or self.rng.randrange(3) != 0))
            before, after = await self.tick()
            accepted = before["job_valid"] and before["job_ready"]
            if accepted:
                jobs.append(decode_job(before["job_data"]))
            assert after["queued_jobs"] == before["queued_jobs"] - int(accepted), \
                "FIFO count did not follow accepted transfers"
            if after["queued_jobs"] == 0:
                break
        else:
            raise AssertionError("FIFO drain timed out")
        assert len(jobs) == expected_count, "lost or extra FIFO transfers"
        self.set(job_ready=1)
        for _ in range(5):
            before, after = await self.tick()
            assert not before["job_valid"] and not after["job_valid"], "extra job after drain"
        self.set(job_ready=0)
        return jobs

    async def quantum(self, writes):
        self.last_run_cycles = None
        for write in writes:
            if write["address"] >= self.state_count:
                return {"jobs": [], "state": await self.snapshot(),
                        "status": decode_status(2)}
            await self.configure(True, write["address"], encode_value(write["value"]))
        status = await self.execute()
        jobs = await self.drain()
        return {"jobs": jobs, "state": await self.snapshot(), "status": decode_status(status)}
