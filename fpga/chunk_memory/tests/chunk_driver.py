"""Independent word-memory slave and public chunk command transport."""
import random
import os
from cocotb.triggers import Timer
from direct_memory import MemoryHost, native_field

OPCODES = {"read": 0, "write": 1, "prefetch": 2, "fence": 3, "retire": 4}
ERRORS = {1: "invalid_view", 4: "invalid_shape", 5: "memory_fault"}
MASK = (1 << 32)-1


def signed(value):
    return value - (1 << 32) if value & (1 << 31) else value


class Driver(MemoryHost):
    reads = native_field('reads')
    writes = native_field('writes')
    burst = native_field('burst')
    OUTPUTS = ("cmd_ready", "write_ready", "read_valid", "read_data", "read_last", "response_valid",
               "response_id", "response_error", "prefetch_ready", "busy", "fault", "memory_valid",
               "memory_write", "memory_address", "memory_write_data", "memory_response_ready")

    def __init__(self, dut, capacities, seed=0):
        self.dut = dut
        self.slots, self.frames, self.read_buffers, self.write_buffers = capacities
        self.rng = random.Random(seed)
        self.memory = [0] * (self.slots*self.frames)
        self.base = 0x1234000000
        self.pending = None
        self.pause = False
        self.fail_next = False
        self.stalls = True
        self.requests = self.reads = self.writes = self.cycles = 0
        self.held_request = self.held_read = self.held_response = None
        self.burst = None
        self.axi = None
        if hasattr(dut, "m_axi_awvalid"):
            from word_service import create_service
            self.axi = create_service(self, dut, seed=seed, output=os.environ.get("CHUNK_OUTPUT"))

    def close(self):
        if self.axi is not None:
            self.axi.close()

    def set(self, **values):
        for name, value in values.items():
            getattr(self.dut, name).value = value

    def sample(self):
        return {name: int(getattr(self.dut, name).value) for name in self.OUTPUTS}

    async def tick(self):
        self.set(clk=0)
        if self.axi is not None:
            if not self.native_memory:
                self.axi.drive()
        else:
            response = self.pending is not None and self.pending[0] == 0 and not self.pause
            self.set(memory_ready=int(not self.pause and self.pending is None and (not self.stalls or self.rng.randrange(4) != 0)),
                     memory_response_valid=int(response), memory_response_error=int(response and self.pending[2]),
                     memory_read_data=self.pending[1] if response else 0)
        await Timer(5, units="ns")
        before = self.sample()
        if self.axi is not None:
            if not self.native_memory:
                self.axi.step(self.axi.sample(), bool(int(self.dut.reset.value)))
        elif int(self.dut.reset.value):
            self.pending = self.held_request = self.held_read = self.held_response = self.burst = None
        else:
            request = tuple(before[key] for key in ("memory_write", "memory_address", "memory_write_data"))
            if self.held_request is not None:
                assert before["memory_valid"] and request == self.held_request, "stalled DDR request changed"
            self.held_request = request if before["memory_valid"] and not int(self.dut.memory_ready.value) else None
            if self.held_read is not None:
                assert before["read_valid"] and (before["read_data"], before["read_last"]) == self.held_read
            self.held_read = (before["read_data"], before["read_last"]) if before["read_valid"] and not int(self.dut.read_ready.value) else None
            if self.held_response is not None:
                assert before["response_valid"] and (before["response_id"], before["response_error"]) == self.held_response
            self.held_response = (before["response_id"], before["response_error"]) if before["response_valid"] and not int(self.dut.response_ready.value) else None
            if response and before["memory_response_ready"]:
                self.pending = None
            elif self.pending is not None and self.pending[0] > 0:
                self.pending[0] -= 1
            if before["memory_valid"] and int(self.dut.memory_ready.value):
                assert self.pending is None
                write, absolute, data = request
                address = absolute-self.base
                assert address % 4 == 0 and 0 <= address < len(self.memory)*4
                word = address//4
                if self.burst is None:
                    assert word % self.frames == 0, "transfer did not begin at a chunk boundary"
                    self.burst = [write, word, 0]
                assert write == self.burst[0] and word == self.burst[1]+self.burst[2], "chunk transfers interleaved"
                self.burst[2] += 1
                if self.burst[2] == self.frames:
                    self.burst = None
                error = self.fail_next
                self.fail_next = False
                if write and not error:
                    self.memory[word] = data
                self.pending = [self.rng.randrange(8) if self.stalls else 0, self.memory[word], error]
                self.requests += 1
                self.writes += write
                self.reads += 1-write
        self.set(clk=1)
        await Timer(5, units="ns")
        self.set(clk=0)
        self.cycles += 1
        return before, self.sample()

    async def reset(self, samples=None):
        self.set(reset=1, cmd_valid=0, cmd_opcode=0, cmd_id=0, cmd_chunk=0, cmd_offset=0, cmd_count=0,
                 write_valid=0, write_data=0, read_ready=0, response_ready=0,
                 prefetch_valid=0, prefetch_chunk=0, arena_base=self.base)
        await self.tick()
        self.set(reset=0)
        await self.tick()
        if samples is not None:
            assert len(samples) == len(self.memory)
            self.memory[:] = [x & MASK for x in samples]
        if self.native_memory and (samples is not None or not self.axi.loaded):
            await self.load_memory()
        assert int(self.dut.cmd_ready.value) and not int(self.dut.fault.value)

    async def hint(self, chunk):
        self.set(prefetch_valid=1, prefetch_chunk=chunk)
        for _ in range(100000):
            before, _ = await self.tick()
            if before["prefetch_ready"]:
                self.set(prefetch_valid=0)
                return
        raise AssertionError("hint admission timed out")

    async def command(self, operation, request_id=0, *, stalls=True):
        source = operation.get("samples", [])
        count = len(source) if operation["kind"] == "write" else operation.get("frames", 0)
        self.set(cmd_valid=1, cmd_opcode=OPCODES[operation["kind"]], cmd_id=request_id,
                 cmd_chunk=operation.get("chunk", 0), cmd_offset=operation.get("offset", 0), cmd_count=count,
                 read_ready=0, write_valid=0, response_ready=0)
        for _ in range(100000):
            before, _ = await self.tick()
            if before["cmd_ready"]:
                break
        else:
            raise AssertionError("command admission timed out")
        self.set(cmd_valid=0, cmd_chunk=MASK, cmd_offset=MASK, cmd_count=MASK, cmd_id=MASK)
        sent, samples, offered = 0, [], False
        for _ in range(100000):
            offered = sent < len(source) and (offered or not stalls or self.rng.randrange(4) != 0)
            self.set(write_valid=int(offered), write_data=(source[sent] & MASK) if offered else 0,
                     read_ready=int(not stalls or self.rng.randrange(4) != 0),
                     response_ready=int(not stalls or self.rng.randrange(4) != 0))
            before, _ = await self.tick()
            if offered and before["write_ready"]:
                sent += 1
                offered = False
            if before["read_valid"] and int(self.dut.read_ready.value):
                samples.append(signed(before["read_data"]))
                assert bool(before["read_last"]) == (len(samples) == count)
            if before["response_valid"] and int(self.dut.response_ready.value):
                assert before["response_id"] == request_id
                code = before["response_error"]
                if code == 0:
                    assert sent == len(source)
                self.set(write_valid=0, read_ready=0, response_ready=0)
                return code, samples
        raise AssertionError(f"command timed out: {operation}")
