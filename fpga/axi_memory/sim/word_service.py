"""Adapt existing host drivers to the shared AXI slave without driving core wires."""
from options import create_memory
from axi_memory import summary


class WordService:
    def __init__(self, owner, dut, *, seed=0, output=None):
        self.owner = owner
        self.memory = create_memory(dut, owner.memory, seed=seed, output=output, word_only=True)
        self.records = 0
        self.transactions = 0
        self.chunk = None
        self.transfers = []

    def drive(self):
        owner, memory = self.owner, self.memory
        memory.stalls = owner.stalls
        memory.pause = getattr(owner, "pause", False)
        memory.pause_responses = getattr(owner, "pause_responses", False)
        memory.fail_next = owner.fail_next
        memory.drive()

    def sample(self):
        return self.memory.sample()

    def step(self, packet, reset):
        owner, memory = self.owner, self.memory
        memory.step(packet, reset)
        frames = getattr(owner, "frames", getattr(owner, "chunk_frames", 1))
        if reset:
            self.chunk = None
        for record in memory.records[self.records:]:
            word = (record["address"]-memory.base)//4
            if self.chunk is None:
                self.chunk = dict(chunk=word//frames, write=record["write"], start=record["start"], words=0)
            self.chunk["words"] += 1
            if self.chunk["words"] == frames:
                self.transfers.append({**self.chunk, "end": record["end"], "cycles": record["end"]-self.chunk["start"]})
                self.chunk = None
        owner.fail_next = memory.fail_next
        owner.pending = memory.pending
        owner.requests = memory.reads+memory.writes
        if hasattr(owner, "read_requests"):
            owner.read_requests, owner.write_requests = memory.reads, memory.writes
        else:
            owner.reads, owner.writes = memory.reads, memory.writes
        if hasattr(owner, "response_latencies"):
            owner.response_latencies.extend(record["latency"] for record in memory.records[self.records:])
        self.records = len(memory.records)
        if hasattr(owner, "burst"):
            if reset:
                owner.burst = None
            elif owner.requests != self.transactions:
                active = memory.active
                assert active is not None
                word = (active["address"]-memory.base)//4
                write = active["write"]
                if owner.burst is None:
                    assert word % owner.frames == 0, "transfer did not begin at a chunk boundary"
                    owner.burst = [write, word, 0]
                assert write == owner.burst[0] and word == owner.burst[1]+owner.burst[2], "chunk transfers interleaved"
                owner.burst[2] += 1
                if owner.burst[2] == owner.frames:
                    owner.burst = None
        self.transactions = owner.requests

    def report(self, *, final=False):
        result = self.memory.report()
        result["chunk_fill_cycles"] = summary([t["cycles"] for t in self.transfers if not t["write"]])
        result["chunk_writeback_cycles"] = summary([t["cycles"] for t in self.transfers if t["write"]])
        if hasattr(self.memory.timing, "report"):
            result["native"] = self.memory.timing.report(final=final)
        return result

    def close(self):
        self.memory.close()
