"""Drive the combined state/sample pipeline through its public interfaces."""

from cocotb.triggers import Timer

from codec import decode_job, encode_value
from driver import Driver


def signed(value):
    return value - (1 << 32) if value & (1 << 31) else value


class AudioDriver(Driver):
    SIGNALS = ("busy", "done", "error_code", "error_stage", "run_ready", "cfg_ready",
               "cfg_response_valid", "cfg_response_error", "cfg_read_data", "queued_jobs",
               "state_busy", "state_done", "jobs_busy", "jobs_done", "buffer_ready",
               "audio_cfg_ready", "audio_cfg_response_valid", "audio_cfg_response_error",
               "audio_cfg_read_data", "monitor_valid", "monitor_ready", "monitor_data", "drain_valid")

    def __init__(self, dut, capacities, seed):
        super().__init__(dut, capacities[:3], seed)
        self.max_frames, self.max_buffers, self.input_ports, self.output_ports = capacities[3:]
        self.consumed = []
        self.drained = []
        self.timing = {}
        self.sample_observer = None

    async def tick(self):
        self.dut.clk.value = 0
        await Timer(5, units="ns")
        before = self.sample()
        if int(self.dut.reset.value):
            self.held_job = None
        else:
            assert before["queued_jobs"] <= self.job_depth
            if self.held_job is not None:
                assert before["monitor_valid"] and before["monitor_data"] == self.held_job, "stalled descriptor changed"
            self.held_job = before["monitor_data"] if before["monitor_valid"] and not before["monitor_ready"] else None
            if before["busy"]:
                assert not (before["cfg_ready"] or before["audio_cfg_ready"] or before["run_ready"])
            if before["monitor_valid"] and before["monitor_ready"]:
                target = self.consumed if before["jobs_busy"] else self.drained
                target.append(decode_job(before["monitor_data"]))
        if self.sample_observer is not None:
            self.sample_observer(self.cycles)
        self.dut.clk.value = 1
        await Timer(5, units="ns")
        after = self.sample()
        self.dut.clk.value = 0
        self.cycles += 1
        return before, after

    async def reset(self):
        self.set(clk=0, reset=1, run_valid=0, run_instruction_count=0, run_state_count=0,
                 run_quantum=0, run_chunk_frames=0, run_job_budget=0, run_buffers=0, run_inputs=0, run_outputs=0,
                 cfg_valid=0, cfg_state=0, cfg_write=0, cfg_address=0, cfg_write_data=0,
                 audio_cfg_valid=0, audio_cfg_space=0, audio_cfg_write=0, audio_cfg_address=0,
                 audio_cfg_write_data=0, job_enable=1, drain_ready=0,
                 loop_read_ready=0,loop_response_valid=0,loop_response_error=0,loop_response_data=0,
                 )
        if hasattr(self.dut, "media_fault"):
            self.set(media_fault=0, media_hint_ready=0)
        await self.tick()
        _, after = await self.tick()
        assert not after["busy"] and not after["queued_jobs"] and not after["buffer_ready"]
        self.set(reset=0)
        await self.tick()

    async def audio_configure(self, space, address, value=None, *, expect_error=False):
        self.set(audio_cfg_valid=1, audio_cfg_space=space, audio_cfg_write=int(value is not None),
                 audio_cfg_address=address, audio_cfg_write_data=0 if value is None else value & 0xffffffff)
        for _ in range(100):
            before, after = await self.tick()
            if before["audio_cfg_ready"]:
                break
        else:
            raise AssertionError("sample configuration timed out")
        self.set(audio_cfg_valid=0)
        assert after["audio_cfg_response_valid"] and after["audio_cfg_response_error"] == expect_error
        result = after["audio_cfg_read_data"]
        if expect_error or value is not None:
            assert result == 0
        _, after = await self.tick()
        assert not after["audio_cfg_response_valid"]
        return signed(result)

    async def load(self, case):
        self.logical_buffers = case["buffers"]
        self.logical_outputs = case["output_ports"]
        self.frames = case["program"]["quantum"]
        assert 1 <= self.frames <= self.max_frames
        assert self.logical_buffers <= self.max_buffers and self.logical_outputs <= self.output_ports
        await super().load(case)
        for port in range(self.logical_outputs):
            for frame in range(self.frames):
                await self.audio_configure(1, port * self.max_frames + frame, 0)

    async def read_samples(self):
        ready = int(self.dut.buffer_ready.value)
        buffers = []
        for index in range(self.logical_buffers):
            buffers.append([await self.audio_configure(2, index * self.max_frames + frame)
                            for frame in range(self.frames)] if ready & (1 << index) else None)
        outputs = [[await self.audio_configure(1, port * self.max_frames + frame)
                    for frame in range(self.frames)] for port in range(self.logical_outputs)]
        return buffers, outputs

    async def execute_audio(self, inputs, *, stalls=True):
        assert len(inputs) <= self.input_ports and all(len(port) == self.frames for port in inputs)
        for port, samples in enumerate(inputs):
            for frame, sample in enumerate(samples):
                await self.audio_configure(0, port * self.max_frames + frame, sample)
        self.consumed, self.drained = [], []
        self.set(run_buffers=self.logical_buffers, run_inputs=len(inputs), run_outputs=self.logical_outputs,
                 job_enable=1, drain_ready=0)
        after = await self.start()
        accepted = self.cycles
        state_end = job_start = job_end = None
        for index in range(200 * self.job_depth + 40 * self.max_frames * self.program_depth + 100):
            if after["state_done"]:
                state_end = self.cycles
            if after["jobs_busy"] and job_start is None:
                job_start = self.cycles
            if after["jobs_done"]:
                job_end = self.cycles
            if after["done"]:
                break
            self.set(job_enable=int(not stalls or index % 11 == 10 or self.rng.randrange(4) != 0))
            _, after = await self.tick()
        else:
            raise AssertionError("combined pipeline timed out")
        self.set(job_enable=1)
        assert not after["busy"]
        self.timing = {"total": self.cycles - accepted,
                       "state": None if state_end is None else state_end - accepted,
                       "jobs": None if job_end is None else job_end - job_start,
                       "stalls": stalls}
        stage, code = after["error_stage"], after["error_code"]
        _, after = await self.tick()
        assert not after["done"]
        for _ in range(12 * self.job_depth + 20):
            if int(self.dut.queued_jobs.value) == 0:
                break
            self.set(drain_ready=self.rng.randrange(2))
            await self.tick()
        else:
            raise AssertionError("remaining-job drain timed out")
        self.set(drain_ready=0)
        await self.tick()
        assert not int(self.dut.drain_valid.value), "extra job after drain"
        return stage, code

    async def quantum_audio(self, step, *, stalls=True):
        for write in step["writes"]:
            assert write["address"] < self.state_count
            await self.configure(True, write["address"], encode_value(write["value"]))
        stage, code = await self.execute_audio(step["inputs"], stalls=stalls)
        if stage == 1:
            errors = {1: "queue_full", 2: "invalid_state"}
        else:
            assert stage == 2
            errors = {1: "invalid_buffer", 2: "invalid_shape"}
        status = {"kind": "completed"} if code == 0 else {"kind": "failed", "value": {"error": errors[code]}}
        buffers, outputs = await self.read_samples()
        return {"jobs": self.consumed + self.drained, "remaining_jobs": self.drained,
                "state": await self.snapshot(), "buffers": buffers, "outputs": outputs,
                "stage": "state" if stage == 1 else "jobs", "status": status}
