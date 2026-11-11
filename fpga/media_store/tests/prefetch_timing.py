"""Record RTL execution and replay speculative, same-quantum chunk transfers."""
from collections import Counter

from codec import decode_job


def value(signal):
    return int(signal.value)


class PrefetchTiming:
    def __init__(self, dut, media, chunk_frames):
        self.dut = dut
        self.state = dut.pipeline.state_engine
        self.memory = dut.media.sample_memory
        self.media = media
        self.chunk_frames = chunk_frames
        self.enabled = False

    def begin(self):
        self.start = self.total = None
        self.emitted, self.jobs, self.transfers = [], [], []
        self.active_transfer = None
        self.counts = Counter(dict.fromkeys(("demand_fill_wait", "transfer_active", "transfer_during_state", "transfer_during_nonwaiting_jobs"), 0))
        self.enabled = True

    def observe(self, cycle):
        if not self.enabled:
            return
        if self.start is None:
            if value(self.dut.run_valid) and value(self.dut.run_ready):
                self.start = cycle + 1
            return
        time = cycle - self.start
        if value(self.dut.done):
            self.total = time
            self.enabled = False
            return
        if value(self.state.engine_job_valid) and value(self.state.engine_job_ready):
            self.emitted.append({"cycle": time, "job": decode_job(value(self.state.engine_job_data))})
        if value(self.dut.monitor_valid) and value(self.dut.monitor_ready):
            if self.jobs:
                self.jobs[-1]["end"] = time
            job = decode_job(value(self.dut.monitor_data))
            assert job == self.emitted[len(self.jobs)]["job"]
            item = {"job": job, "start": time, "emitted": self.emitted[len(self.jobs)]["cycle"], "fill_wait": 0}
            if job["kind"] == "read_view":
                view = self.media["views"][job["view"]]
                assert job["length_chunks"] > 0
                assert view.get("range", {}).get("offset", 0) == 0
                item["chunk"] = view["chunks"][job["position_chunks"]]
            self.jobs.append(item)
        if value(self.dut.jobs_done) and self.jobs:
            self.jobs[-1].setdefault("end", time)
        waiting = value(self.memory.phase) == 3 and value(self.memory.opcode) == 0
        if waiting:
            assert self.jobs[-1]["job"]["kind"] == "read_view"
            self.jobs[-1]["fill_wait"] += 1
            self.counts["demand_fill_wait"] += 1
        if value(self.memory.phase) == 2 and value(self.memory.opcode) == 0 and self.jobs:
            self.jobs[-1].setdefault("lookup", time)
        active = value(self.memory.dma_phase) != 0
        if active:
            self.counts["transfer_active"] += 1
            if value(self.dut.state_busy):
                self.counts["transfer_during_state"] += 1
            elif value(self.dut.jobs_busy) and not waiting:
                self.counts["transfer_during_nonwaiting_jobs"] += 1
            if self.active_transfer is None:
                assert not value(self.memory.dma_write)
                self.active_transfer = {"chunk": value(self.memory.dma_chunk), "start": time}
        elif self.active_transfer is not None:
            self.active_transfer["end"] = time
            self.transfers.append(self.active_transfer)
            self.active_transfer = None

    def replay(self, service_cycles, *, resolver_cycles=8, early=True):
        available, transfers = {}, []
        resolver_free = memory_free = 0
        reads = [job for job in self.jobs if "chunk" in job]
        if early:
            for job in reads:
                resolver_free = max(resolver_free, job["emitted"]) + resolver_cycles
                start = max(resolver_free, memory_free)
                memory_free = start + service_cycles
                available[job["chunk"]] = memory_free
                transfers.append({"chunk": job["chunk"], "start": start, "end": memory_free})
        time = self.jobs[0]["start"]
        waits, deadlines = [], []
        for job in self.jobs:
            wait = 0
            if "chunk" in job:
                deadline = time + job["lookup"] - job["start"] + 1
                if not early:
                    available[job["chunk"]] = deadline + service_cycles
                wait = max(0, available[job["chunk"]] - deadline)
                waits.append(wait)
                deadlines.append({"chunk": job["chunk"], "needed": deadline,
                                  "ready": available[job["chunk"]], "wait": wait})
            time += job["end"] - job["start"] - job["fill_wait"] + wait
        time += self.total - self.jobs[-1]["end"]
        budget = len(reads) * service_cycles
        return {"total_cycles": time, "exposed_wait_cycles": sum(waits),
                "serialized_transfer_cycles": budget, "resolver_cycles_per_read": resolver_cycles if early else 0,
                "hidden_transfer_fraction": 1 - sum(waits) / budget,
                "reads_waiting": sum(wait > 0 for wait in waits),
                "transfer_schedule": transfers, "read_deadlines": deadlines}

    def finish(self, total, response_cycles):
        assert self.total == total, (self.total, total)
        assert self.active_transfer is None
        assert len(self.jobs) == len(self.emitted) == 192
        reads = [job for job in self.jobs if "chunk" in job]
        assert len(reads) == len({job["chunk"] for job in reads}) == len(self.transfers) == 64
        assert {item["chunk"] for item in self.transfers} == {job["chunk"] for job in reads}
        service = {item["end"] - item["start"] for item in self.transfers}
        assert len(service) == 1, service
        service = service.pop() + 1
        assert service == self.chunk_frames * (response_cycles + 1) + 1, service
        floor = self.total - self.counts["demand_fill_wait"]
        assert self.jobs[0]["start"] + sum(job["end"] - job["start"] for job in self.jobs) + self.total - self.jobs[-1]["end"] == self.total
        return {"measured": {"total_cycles": self.total, "non_fill_wait_cycles": floor,
                             "chunk_service_cycles_including_dispatch": service, **self.counts},
                "early_prefetch": self.replay(service),
                "instant_resolution": self.replay(service, resolver_cycles=0),
                "no_prefetch_estimate": self.replay(service, early=False),
                "service_sensitivity": [{"response_cycles": cycles, **self.replay(self.chunk_frames * (cycles + 1) + 1)}
                                        for cycles in (1, 4, 7, 13, 25)],
                "jobs": self.jobs, "transfers": self.transfers}
