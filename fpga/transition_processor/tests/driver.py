"""Serialized command transport, checked handshakes and architectural readback."""
import random
from cocotb.triggers import FallingEdge, RisingEdge, Timer, with_timeout
from cocotb.utils import get_sim_time
import codec as abi


class Driver:
    def __init__(self, dut, depths, seed=0):
        self.dut, self.depths = dut, depths
        self.random = random.Random(seed)
        self.last_cycles = 0
        self.phase_cycles = []

    def set(self, **signals):
        for name, value in signals.items():
            getattr(self.dut, name).value = value

    async def edge(self):
        await RisingEdge(self.dut.clk)
        await Timer(1, units="ns")

    async def reset(self):
        await FallingEdge(self.dut.clk)
        self.set(reset=1, command_valid=0, command_opcode=0, command_index=0,
                 command_data=0, response_ready=0)
        await Timer(21, units="ns")
        assert not int(self.dut.command_ready.value)
        assert not int(self.dut.response_valid.value)
        assert not int(self.dut.busy.value)
        await FallingEdge(self.dut.clk)
        self.set(reset=0)
        await Timer(1, units="ns")

    async def command(self, opcode, index=0, data=0, *, stall=None, intrude=False):
        await FallingEdge(self.dut.clk)
        assert int(self.dut.command_ready.value), "command interface failed to become ready"
        self.set(command_valid=1, command_opcode=opcode, command_index=index, command_data=data, response_ready=0)
        await self.edge()
        accepted = get_sim_time(units="ns") - 1
        assert int(self.dut.busy.value) and not int(self.dut.command_ready.value)
        self.set(command_valid=int(intrude), command_opcode=255, command_index=0xffffffff, command_data=(1 << 640) - 1)
        if intrude:
            await Timer(20, units="ns")
            assert not int(self.dut.command_ready.value), "command accepted during outstanding operation"
            self.set(command_valid=0)
        if not int(self.dut.response_valid.value):
            await with_timeout(RisingEdge(self.dut.response_valid), 100_000_000, "ns")
            await Timer(1, units="ns")
        self.last_cycles = round((get_sim_time(units="ns") - 1 - accepted) / 10)
        assert not int(self.dut.busy.value), "response preceded operation completion"
        status = int(self.dut.response_error.value)
        value = int(self.dut.response_data.value)
        assert not int(self.dut.command_ready.value), "new command admitted with held response"
        if stall is None:
            stall = self.random.randrange(4)
        if stall:
            await Timer(stall * 10, units="ns")
            assert int(self.dut.response_valid.value)
            assert int(self.dut.response_error.value) == status
            assert int(self.dut.response_data.value) == value, "response changed under backpressure"
        await FallingEdge(self.dut.clk)
        self.set(response_ready=1)
        await self.edge()
        self.set(response_ready=0)
        assert not int(self.dut.response_valid.value), "response did not retire"
        assert int(self.dut.command_ready.value)
        if status:
            assert value == 0, "error response leaked stale data"
        return status, value

    async def ok(self, opcode, index=0, data=0):
        error, value = await self.command(opcode, index, data)
        assert error == 0, f"command {opcode} rejected: {abi.ERRORS[error]}"
        return value

    async def load(self, case, image):
        limits = case["plan"]["limits"]
        assert limits["loop_slots"] <= self.depths[0]
        assert limits["pending_requests"] <= self.depths[1]
        assert limits["events"] <= self.depths[2]
        self.limits = limits
        self.ids = sorted((s["id"] for s in image["description"]["loops"]), key=lambda v: v["slot"])
        await self.reset()
        specs = {s["id"]["slot"]: s for s in image["description"]["loops"]}
        for i in range(limits["loop_slots"]):
            await self.ok(abi.WRITE_SPEC, i, abi.encode_spec(specs.get(i)))
        offset = 0
        for component in image["components"]:
            for i, member in enumerate(component):
                await self.ok(abi.WRITE_MEMBER, offset, member["slot"] | (int(i == len(component) - 1) << 32))
                offset += 1
        data = (limits["loop_slots"] | (limits["pending_requests"] << 32)
                | (limits["events"] << 64) | (offset << 96)
                | (image["description"]["quantum"] << 128))
        await self.ok(abi.PREPARE, data=data)

    async def status(self):
        raw = await self.ok(abi.STATUS)
        return dict(frame=abi.bits(raw,0,64), fault=abi.bits(raw,64,4), phase=abi.bits(raw,128,2),
                    configured=bool(abi.bits(raw,136,1)), events=abi.bits(raw,192,32),
                    request_checks=abi.bits(raw,416,32), follower_visits=abi.bits(raw,448,32),
                    pulse_deliveries=abi.bits(raw,480,32))

    async def states(self):
        return [abi.decode_state(await self.ok(abi.READ_STATE, i["slot"]), i["slot"]) for i in self.ids]

    async def requests(self):
        return [abi.decode_request(await self.ok(abi.READ_REQUEST, i), i)
                for i in range(self.limits["pending_requests"])]

    async def events(self):
        status=await self.status()
        return [abi.decode_event(await self.ok(abi.READ_EVENT,i)) for i in range(status['events'])]

    async def collect(self, request):
        error, value = await self.command(abi.COLLECT, request["slot"], request["generation"])
        return {"request": request, "result": abi.response(error, abi.decode_completion(value, request["slot"]) if not error else None)}

    async def step(self, step):
        frame=(await self.status())['frame']
        collected=[]
        if step['collect_completed']:
            for request in await self.requests():
                if request and request['outcome']: collected.append(await self.collect(request['id']))
        for request in step['collect']: collected.append(await self.collect(request))
        cancelled=[]
        for request in step['cancel']:
            error,value=await self.command(abi.CANCEL,request['slot'],request['generation'])
            cancelled.append(abi.response(error,bool(value)))
        submitted=[]
        for command in step['commands']:
            error,value=await self.command(abi.SUBMIT,data=abi.encode_command(command))
            submitted.append(abi.response(error,abi.decode_identity(value)))
        error,_=await self.command(abi.PROCESS)
        self.phase_cycles.append(dict(process=self.last_cycles))
        return dict(frame=frame,collected=collected,cancelled=cancelled,submitted=submitted,
                    processed=abi.response(error,None),states=await self.states(),requests=await self.requests(),
                    events=await self.events(),next_frame=(await self.status())['frame'])
