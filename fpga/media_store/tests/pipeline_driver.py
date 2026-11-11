"""Drive state/jobs/storage together while an independent slave serves sample memory."""
from audio_driver import AudioDriver
from codec import decode_job, encode_value
from media_driver import Driver as MediaDriver


class StorePorts:
    def __init__(self, dut):
        self.dut = dut

    def __getattr__(self, name):
        shared = name in ("clk", "reset", "arena_base") or name.startswith(("memory_", "m_axi_"))
        return getattr(self.dut, name if shared else "store_" + name)


class PipelineDriver(AudioDriver):
    def __init__(self, dut, capacities, media_capacities, seed):
        super().__init__(dut, capacities, seed)
        self.media = MediaDriver(StorePorts(dut), media_capacities, seed+17)
        self.media.before_clock = self.before_clock
        self.timing_probe = None

    def before_clock(self):
        if self.timing_probe is not None:
            self.timing_probe.observe(self.media.cycles)
        before = self.sample()
        self.before_audio = before
        if int(self.dut.reset.value):
            self.held_job = None
            return
        assert before["queued_jobs"] <= self.job_depth
        if self.held_job is not None:
            assert before["monitor_valid"] and before["monitor_data"] == self.held_job
        self.held_job = before["monitor_data"] if before["monitor_valid"] and not before["monitor_ready"] else None
        if before["busy"]:
            assert not (before["cfg_ready"] or before["audio_cfg_ready"] or before["run_ready"])
        if before["monitor_valid"] and before["monitor_ready"]:
            (self.consumed if before["jobs_busy"] else self.drained).append(decode_job(before["monitor_data"]))

    async def tick(self):
        await self.media.tick()
        self.cycles = self.media.cycles
        return self.before_audio, self.sample()

    async def load(self, case):
        self.media.set(cmd_valid=0, cfg_valid=0, write_valid=0, read_ready=0)
        await super().load(case)
        await self.media.load(case["media"])

    async def quantum_audio(self, step, *, stalls=True):
        self.media.stalls = stalls
        for action in step.get("media_actions", []):
            result = await self.media.command(action, stalls=stalls)
            if result["status"]["kind"] != "completed":
                buffers, outputs = await self.read_samples()
                return {"jobs": [], "remaining_jobs": [], "state": await self.snapshot(),
                        "buffers": buffers, "outputs": outputs, "stage": "media",
                        "status": result["status"], "media": result["media"]}
        for write in step["writes"]:
            await self.configure(True, write["address"], encode_value(write["value"]))
        stage, code = await self.execute_audio(step["inputs"], stalls=stalls)
        errors = {1: "queue_full", 2: "invalid_state"} if stage == 1 else {
            1: "invalid_buffer", 2: "invalid_shape", 5: "invalid_view", 6: "memory_full", 7: "invalid_state"}
        status = {"kind": "completed"} if code == 0 else {"kind": "failed", "value": {"error": errors[code]}}
        buffers, outputs = await self.read_samples()
        return {"jobs": self.consumed+self.drained, "remaining_jobs": self.drained,
                "state": await self.snapshot(), "buffers": buffers, "outputs": outputs,
                "stage": "state" if stage == 1 else "jobs", "status": status,
                "media": await self.media.snapshot()}
