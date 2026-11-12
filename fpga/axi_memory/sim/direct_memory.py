"""Checkpoint administration for the clock-driven SV functional memory."""
from pathlib import Path
import os


class DirectMemory:
    is_native = True
    timing = None

    def __init__(self, dut, words, **unused):
        self.dut = dut
        self.capacity = len(words)
        self.width = len(dut.m_axi_wdata)//8
        self.base = int(os.environ.get('AXI_PHYSICAL_BASE', str(0x01000000)))
        self.image = Path('sim-memory.hex').resolve()
        self.loaded = False
        self.control = self.failure = 0
        self.read_origin = int(dut.sim_reads.value)
        self.write_origin = int(dut.sim_writes.value)
        self.memory = self
        dut.sim_image_request.value = 0
        dut.sim_control.value = 0
        dut.sim_fail_token.value = 0

    def flag(bit):
        def get(self):
            return bool(self.control & (1 << bit))
        def set(self, enabled):
            self.control = (self.control & ~(1 << bit)) | (int(bool(enabled)) << bit)
            self.dut.sim_control.value = self.control
        return property(get, set)

    pause = flag(0)
    pause_responses = flag(1)
    hold_aw = flag(2)
    hold_w = flag(3)
    hold_ar = flag(4)

    @property
    def fail_next(self):
        return self.failure != int(self.dut.sim_fail_seen.value)

    @fail_next.setter
    def fail_next(self, enabled):
        seen = int(self.dut.sim_fail_seen.value)
        self.failure = (seen+1) & 0xffffffff if enabled else seen
        self.dut.sim_fail_token.value = self.failure

    @property
    def reads(self):
        return int(self.dut.sim_reads.value)-self.read_origin

    @property
    def writes(self):
        return int(self.dut.sim_writes.value)-self.write_origin

    @property
    def requests(self):
        return self.reads+self.writes

    @property
    def active(self):
        if not int(self.dut.sim_active.value):
            return None
        return dict(write=bool(int(self.dut.sim_write.value)), error=bool(int(self.dut.sim_error.value)),
                    index=int(self.dut.sim_index.value))

    @property
    def pending(self):
        return [0, 0, self.active['error']] if self.active is not None else None

    @property
    def burst(self):
        return True if int(self.dut.sim_busy.value) else None

    async def exchange(self, operation, tick):
        assert not int(self.dut.sim_busy.value), 'image exchange requires drained memory'
        self.dut.sim_image_request.value = operation
        for _ in range(10):
            await tick()
            if int(self.dut.sim_image_done.value):
                break
        else:
            raise AssertionError('native memory image exchange timed out')
        self.dut.sim_image_request.value = 0
        await tick()

    async def load_words(self, words, tick):
        assert len(words) == self.capacity
        assert self.capacity == int(self.dut.sim_capacity.value), 'image and SV memory dimensions differ'
        self.image.write_text(''.join(f'{word & 0xffffffff:08x}\n' for word in words))
        await self.exchange(1, tick)
        self.loaded = True

    async def read_words(self, tick):
        await self.exchange(2, tick)
        words = [int(line, 16) for line in self.image.read_text().splitlines()]
        assert len(words) == self.capacity
        return words

    def report(self, **unused):
        return dict(backend='systemverilog', purpose='functional', reads=self.reads, writes=self.writes)

    def close(self):
        pass


def native_field(name, target=None, control=False):
    def get(self):
        service = self.__dict__.get('axi')
        if service is not None and getattr(service, 'is_native', False):
            return getattr(service, target or name)
        return self.__dict__.get('_memory_'+name)
    def set(self, value):
        self.__dict__['_memory_'+name] = value
        service = self.__dict__.get('axi')
        if control and service is not None and getattr(service, 'is_native', False):
            setattr(service, target or name, value)
    return property(get, set)


class MemoryHost:
    pause = native_field('pause', control=True)
    pause_responses = native_field('pause_responses', control=True)
    fail_next = native_field('fail_next', control=True)
    pending = native_field('pending')
    requests = native_field('requests')

    @property
    def native_memory(self):
        return self.axi is not None and getattr(self.axi, 'is_native', False)

    async def load_memory(self):
        if self.native_memory:
            await self.axi.load_words(self.memory, self.tick)

    async def read_memory(self):
        if self.native_memory:
            return await self.axi.read_words(self.tick)
        return list(self.memory)
