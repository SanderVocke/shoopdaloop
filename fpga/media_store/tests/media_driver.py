"""Public command/configuration ports and an independent stalled sample-memory slave."""
from collections import Counter
import os
import random
from cocotb.triggers import Timer
from direct_memory import MemoryHost, native_field

OPCODES = {"create": 0, "slice": 1, "clear": 2, "remove": 3, "reclaim": 4, "append": 5, "read": 6}
ERRORS = {1: "invalid_view", 2: "invalid_state", 3: "memory_full", 4: "invalid_shape"}
MASK32 = (1 << 32) - 1


def signed(value):
    return value - (1 << 32) if value & (1 << 31) else value


class Driver(MemoryHost):
    read_requests = native_field('read_requests','reads')
    write_requests = native_field('write_requests','writes')
    SIGNALS = ("busy", "done", "error_code", "cmd_ready", "result_view", "result_empty",
               "write_ready", "read_valid", "read_data", "memory_valid", "memory_write",
               "memory_address", "memory_write_data", "memory_response_ready", "cfg_ready",
               "cfg_response_valid", "cfg_response_error", "cfg_read_data")

    def __init__(self, dut, capacities, seed):
        self.dut = dut
        self.views, self.slots, self.chunk_frames = capacities
        self.rng = random.Random(seed)
        self.base = 0x1234000000
        self.memory = [0xa5a5a5a5] * (self.slots * self.chunk_frames)
        self.pending = None
        self.held_request = self.held_read = None
        self.cycles = self.requests = 0
        self.read_requests = self.write_requests = 0
        self.request_cycle = None
        self.response_latencies = []
        self.stalls = True
        self.fail_next = False
        self.chunk_limit = None
        self.before_clock = None
        self.axi = None
        if hasattr(dut, "m_axi_awvalid"):
            from word_service import create_service
            self.axi = create_service(self, dut, seed=seed, output=os.environ.get("MEDIA_OUTPUT", os.environ.get("LP_OUTPUT")))

    def close(self):
        if self.axi is not None:
            self.axi.close()

    def set(self, **values):
        for name, value in values.items():
            getattr(self.dut, name).value = value

    def sample(self):
        return {name: int(getattr(self.dut, name).value) for name in self.SIGNALS}

    async def tick(self):
        self.dut.clk.value = 0
        if self.axi is not None:
            if not self.native_memory:
                self.axi.drive()
        else:
            response = self.pending is not None and self.pending[0] == 0
            self.set(memory_ready=int(self.pending is None and (not self.stalls or self.rng.randrange(4) != 0)),
                     memory_response_valid=int(response),
                     memory_read_data=self.pending[1] if response else 0,
                     memory_response_error=int(response and self.pending[2]))
        await Timer(5, units="ns")
        before = self.sample()
        if self.axi is not None:
            if not self.native_memory:
                self.axi.step(self.axi.sample(), bool(int(self.dut.reset.value)))
        elif int(self.dut.reset.value):
            self.pending = self.held_request = self.held_read = None
            self.request_cycle = None
        else:
            packet = tuple(before[key] for key in ("memory_write", "memory_address", "memory_write_data"))
            if self.held_request is not None:
                assert before["memory_valid"] and packet == self.held_request, "stalled memory request changed"
            self.held_request = packet if before["memory_valid"] and not int(self.dut.memory_ready.value) else None
            if self.held_read is not None:
                assert before["read_valid"] and before["read_data"] == self.held_read, "stalled sample changed"
            self.held_read = before["read_data"] if before["read_valid"] and not int(self.dut.read_ready.value) else None
            if before["busy"]:
                assert not before["cmd_ready"] and not before["cfg_ready"]
            if response and before["memory_response_ready"]:
                self.response_latencies.append(self.cycles - self.request_cycle)
                self.pending = None
                self.request_cycle = None
            elif self.pending is not None and self.pending[0] > 0:
                self.pending[0] -= 1
            if before["memory_valid"] and int(self.dut.memory_ready.value):
                assert self.pending is None, "more than one memory request outstanding"
                address = before["memory_address"] - self.base
                assert address % 4 == 0 and 0 <= address < len(self.memory) * 4, "sample address outside arena"
                index = address // 4
                error = self.fail_next
                self.fail_next = False
                if before["memory_write"] and not error:
                    self.memory[index] = before["memory_write_data"]
                self.pending = [self.rng.randrange(6) if self.stalls else 0, self.memory[index], error]
                self.requests += 1
                self.write_requests += before["memory_write"]
                self.read_requests += 1 - before["memory_write"]
                self.request_cycle = self.cycles
        if self.before_clock is not None:
            self.before_clock()
        self.dut.clk.value = 1
        await Timer(5, units="ns")
        after = self.sample()
        self.dut.clk.value = 0
        self.cycles += 1
        return before, after

    async def reset(self):
        if hasattr(self.dut, "hint_valid"):
            self.set(hint_valid=0, hint_view=0, hint_position=0, hint_length=0, hint_count=0)
        self.set(clk=0, reset=1, arena_base=self.base, cmd_valid=0, cmd_opcode=0, cmd_view=0,
                 cmd_start=0, cmd_length=0, cmd_count=0, write_valid=0, write_data=0, read_ready=0,
                 cfg_valid=0, cfg_write=0, cfg_space=0, cfg_address=0, cfg_write_data=0)
        await self.tick()
        self.set(reset=0)
        _, after = await self.tick()
        assert after["cmd_ready"] and not after["busy"]
        if self.native_memory:
            if self.axi.loaded:
                self.memory[:] = await self.read_memory()
            else:
                await self.load_memory()

    async def configure(self, space, address, value=None, *, expect_error=False):
        self.set(cfg_valid=1, cfg_space=space, cfg_address=address,
                 cfg_write=int(value is not None), cfg_write_data=0 if value is None else value)
        for _ in range(100):
            before, after = await self.tick()
            if before["cfg_ready"]:
                break
        else:
            raise AssertionError("configuration timed out")
        self.set(cfg_valid=0)
        assert after["cfg_response_valid"] and bool(after["cfg_response_error"]) == expect_error
        result = after["cfg_read_data"]
        if expect_error or value is not None:
            assert result == 0
        _, after = await self.tick()
        assert not after["cfg_response_valid"]
        return result

    async def load(self, media):
        await self.reset()
        assert media["chunk_frames"] == self.chunk_frames
        slots = len(media["samples"]) // self.chunk_frames
        assert slots <= self.slots and len(media["views"]) <= self.views
        self.chunk_limit = media.get("chunk_limit")
        await self.configure(0, 3, self.slots if self.chunk_limit is None else self.chunk_limit)
        self.memory[:len(media["samples"])] = [value & MASK32 for value in media["samples"]]
        await self.load_memory()
        refs = Counter()
        for index, view in enumerate(media["views"]):
            descriptor = 0
            if view is not None:
                descriptor = (1 | (int("range" in view) << 1)
                              | (view.get("range", {}).get("offset", 0) << 32)
                              | (view["frames"] << 64) | (len(view["chunks"]) << 128))
                for ordinal, chunk in enumerate(view["chunks"]):
                    await self.configure(2, index * self.slots + ordinal, chunk)
                    refs[chunk] += 1
            await self.configure(1, index, descriptor)
        for chunk in range(slots):
            await self.configure(3, chunk, refs[chunk])
        for index, chunk in enumerate(media["free_chunks"]):
            await self.configure(4, index, chunk)
        for index, value in enumerate((len(media["views"]), slots, len(media["free_chunks"]))):
            await self.configure(0, index, value)

    async def snapshot(self):
        await self.fence()
        self.memory[:] = await self.read_memory()
        views, slots, free = [await self.configure(0, index) for index in range(3)]
        assert views <= self.views and free <= slots <= self.slots
        result = {"chunk_frames": self.chunk_frames, "samples": [signed(x) for x in self.memory[:slots*self.chunk_frames]],
                  "free_chunks": [await self.configure(4, index) for index in range(free)], "views": []}
        if self.chunk_limit is not None:
            result["chunk_limit"] = self.chunk_limit
        refs = Counter()
        for index in range(views):
            descriptor = await self.configure(1, index)
            view = None
            if descriptor & 1:
                chunks = [await self.configure(2, index * self.slots + ordinal) for ordinal in range(descriptor >> 128)]
                view = {"chunks": chunks, "frames": (descriptor >> 64) & ((1 << 64) - 1)}
                if descriptor & 2:
                    view["range"] = {"offset": (descriptor >> 32) & MASK32}
                refs.update(chunks)
            result["views"].append(view)
        for chunk in range(slots):
            assert await self.configure(3, chunk) == refs[chunk], "reference count differs from view ownership"
        return result

    async def fence(self):
        self.set(cmd_valid=1, cmd_opcode=7, cmd_view=0, cmd_start=0, cmd_length=0, cmd_count=0,
                 write_valid=0, read_ready=0)
        for _ in range(100000):
            before, after = await self.tick()
            if before["cmd_ready"]:
                break
        else:
            raise AssertionError("media fence admission timed out")
        self.set(cmd_valid=0)
        for _ in range(100000):
            if after["done"]:
                break
            _, after = await self.tick()
        else:
            raise AssertionError("media fence completion timed out")
        assert after["error_code"] == 0, "media fence reported a memory fault"
        await self.tick()

    async def command(self, command, *, stalls=True):
        self.stalls = stalls
        kind = command["kind"]
        source = command.get("samples", [])
        self.set(cmd_valid=1, cmd_opcode=OPCODES[kind], cmd_view=command.get("source", command.get("view", 0)),
                 cmd_start=command.get("start", command.get("position", 0)),
                 cmd_length=command.get("frames", 0) if kind == "slice" else command.get("length", 0),
                 cmd_count=len(source) if kind == "append" else command.get("frames", 0) if kind == "read" else 0)
        before, after = await self.tick()
        assert before["cmd_ready"], "command not admitted"
        accepted = self.cycles
        self.set(cmd_valid=0)
        sent, samples, offered = 0, [], False
        for _ in range(2000 + 64 * (self.chunk_frames*self.slots + len(source) + command.get("frames", 0))):
            if after["done"]:
                break
            offered = sent < len(source) and (offered or not stalls or self.rng.randrange(4) != 0)
            self.set(write_valid=int(offered), write_data=source[sent] & MASK32 if offered else 0,
                     read_ready=int(not stalls or self.rng.randrange(4) != 0))
            before, after = await self.tick()
            if offered and before["write_ready"]:
                sent += 1
                offered = False
            if before["read_valid"] and int(self.dut.read_ready.value):
                samples.append(signed(before["read_data"]))
        else:
            raise AssertionError("media command timed out")
        self.timing = self.cycles - accepted
        code = after["error_code"]
        value = {"kind": "none"}
        if code == 0:
            if kind in ("create", "slice"):
                value = {"kind": "view", "value": after["result_view"]}
            elif kind == "reclaim":
                value = {"kind": "empty", "value": bool(after["result_empty"])}
            elif kind == "read":
                assert len(samples) == command["frames"]
                value = {"kind": "samples", "value": samples}
            if kind == "append":
                assert sent == len(source)
        self.set(write_valid=0, read_ready=0)
        _, after = await self.tick()
        assert not after["done"] and not after["busy"]
        status = {"kind": "completed"} if code == 0 else {"kind": "failed", "value": {"error": ERRORS[code]}}
        return {"status": status, "value": value, "media": await self.snapshot()}
