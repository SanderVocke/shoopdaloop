"""Sample-memory service on the free-running pipeline clock and host store ports."""
import cocotb
from cocotb.triggers import FallingEdge, RisingEdge, Timer
from media_driver import Driver as StoreDriver


class StorePorts:
    def __init__(self, dut):
        self.dut = dut

    def __getattr__(self, name):
        shared = name in ('clk', 'reset', 'arena_base') or name.startswith('memory_')
        return getattr(self.dut, name if shared else 'store_' + name)


class LoopMemory(StoreDriver):
    def __init__(self, dut, capacities, seed):
        super().__init__(StorePorts(dut), capacities, seed)
        self.raw = dut
        self.initial_override = None
        self.pause_responses = False
        self.task = cocotb.start_soon(self.serve())

    async def reset(self):
        self.set(arena_base=self.base, cmd_valid=0, cmd_opcode=0, cmd_view=0,
                 cmd_start=0, cmd_length=0, cmd_count=0, write_valid=0,
                 write_data=0, read_ready=0, cfg_valid=0, cfg_write=0,
                 cfg_space=0, cfg_address=0, cfg_write_data=0)
        await self.tick()
        assert not int(self.raw.store_busy.value)

    async def load(self, media):
        await super().load(self.initial_override if self.initial_override is not None else media)

    async def tick(self):
        await FallingEdge(self.raw.clk)
        await Timer(2, units='ns')
        before = self.sample()
        await RisingEdge(self.raw.clk)
        await Timer(1, units='ns')
        return before, self.sample()

    async def serve(self):
        while True:
            await FallingEdge(self.raw.clk)
            response = self.pending is not None and self.pending[0] == 0 and not self.pause_responses
            self.set(memory_ready=int(self.pending is None and (not self.stalls or self.rng.randrange(4) != 0)),
                     memory_response_valid=int(response), memory_read_data=self.pending[1] if response else 0,
                     memory_response_error=int(response and self.pending[2]))
            await Timer(1, units='ns')
            request = (int(self.raw.memory_write.value), int(self.raw.memory_address.value),
                       int(self.raw.memory_write_data.value))
            valid = int(self.raw.memory_valid.value)
            ready = int(self.raw.memory_ready.value)
            response_ready = int(self.raw.memory_response_ready.value)
            resetting = int(self.raw.reset.value)
            if not resetting and self.held_request is not None:
                assert valid and request == self.held_request, 'stalled memory request changed'
            if self.before_clock is not None and not resetting:
                self.before_clock()
            await RisingEdge(self.raw.clk)
            self.cycles += 1
            if resetting:
                self.pending = self.held_request = None
                continue
            self.held_request = request if valid and not ready else None
            if response and response_ready:
                self.response_latencies.append(self.cycles - self.request_cycle)
                self.pending = None
            elif self.pending is not None and self.pending[0] > 0:
                self.pending[0] -= 1
            if valid and ready:
                assert self.pending is None, 'multiple memory requests outstanding'
                write, absolute, data = request
                address = absolute - self.base
                assert address % 4 == 0 and 0 <= address < len(self.memory) * 4, 'sample request outside admitted arena'
                index = address // 4
                error = self.fail_next
                self.fail_next = False
                if write and not error:
                    self.memory[index] = data
                self.pending = [self.rng.randrange(6) if self.stalls else 0, self.memory[index], error]
                self.requests += 1
                self.write_requests += write
                self.read_requests += 1 - write
                self.request_cycle = self.cycles

    def close(self):
        self.task.kill()
