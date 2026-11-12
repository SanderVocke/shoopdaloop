import random
import cocotb
from cocotb.triggers import Timer
from direct_memory import DirectMemory


@cocotb.test()
async def functional_memory(dut):
    def set(**values):
        for name, value in values.items():
            getattr(dut, name).value = value

    async def tick():
        set(clk=0)
        await Timer(5, units='ns')
        before = {name:int(getattr(dut, 'm_axi_'+name).value) for name in
                  ('awready','wready','bvalid','bresp','arready','rvalid','rdata','rresp','rlast')}
        set(clk=1)
        await Timer(5, units='ns')
        set(clk=0)
        return before

    set(reset=1, m_axi_awid=0, m_axi_awaddr=0, m_axi_awlen=0, m_axi_awsize=0,
        m_axi_awburst=1, m_axi_awvalid=0, m_axi_wdata=0, m_axi_wstrb=0,
        m_axi_wid=0, m_axi_wlast=0, m_axi_wvalid=0, m_axi_bready=0, m_axi_arid=0,
        m_axi_araddr=0, m_axi_arlen=0, m_axi_arsize=0, m_axi_arburst=1,
        m_axi_arvalid=0, m_axi_rready=0)
    memory = DirectMemory(dut, [0]*2048)
    await tick()
    set(reset=0)
    await tick()
    rng = random.Random(913)
    words = [rng.getrandbits(32) for _ in range(2048)]
    await memory.load_words(words, tick)
    assert await memory.read_words(tick) == words
    oracle = bytearray(b''.join(word.to_bytes(4,'little') for word in words))

    async def write(address, size, data, strobes, error=False, hold_address=False, hold_data=False):
        memory.hold_aw, memory.hold_w = hold_address, hold_data
        set(m_axi_awaddr=address, m_axi_awlen=len(data)-1, m_axi_awsize=size,
            m_axi_awvalid=1, m_axi_bready=0)
        sent, admitted = 0, False
        for cycle in range(1000):
            if cycle == 12:
                memory.hold_aw = memory.hold_w = False
            offered = sent < len(data)
            set(m_axi_wvalid=int(offered), m_axi_wdata=data[sent] if offered else 0,
                m_axi_wstrb=strobes[sent] if offered else 0, m_axi_wlast=int(sent == len(data)-1))
            before = await tick()
            if not admitted and before['awready']:
                admitted = True
                set(m_axi_awvalid=0)
            if offered and before['wready']:
                sent += 1
            if before['bvalid']:
                assert admitted and sent == len(data) and bool(before['bresp']) == error
                break
        else:
            raise AssertionError('native write timed out')
        set(m_axi_wvalid=0)
        for _ in range(5):
            before = await tick()
            assert before['bvalid'] and bool(before['bresp']) == error
        set(m_axi_bready=1)
        await tick()
        set(m_axi_bready=0)

    async def read(address, size, count, error=False):
        set(m_axi_araddr=address, m_axi_arlen=count-1, m_axi_arsize=size, m_axi_arvalid=1)
        for _ in range(1000):
            if (await tick())['arready']:
                break
        else:
            raise AssertionError('native read address timed out')
        set(m_axi_arvalid=0)
        received, held = [], None
        for cycle in range(1000):
            ready = cycle % 5 == 0
            set(m_axi_rready=int(ready))
            before = await tick()
            payload = tuple(before[name] for name in ('rdata','rresp','rlast'))
            if held is not None:
                assert before['rvalid'] and payload == held
            held = payload if before['rvalid'] and not ready else None
            if before['rvalid'] and ready:
                assert bool(before['rresp']) == error
                received.append(before['rdata'])
                assert bool(before['rlast']) == (len(received) == count)
                if before['rlast']:
                    set(m_axi_rready=0)
                    return received
        raise AssertionError('native read timed out')

    for size in range(memory.width.bit_length()):
        for count in (1,2,16):
            start = memory.base+4096+(memory.width-(1 << size) if count == 1 else 0)
            data = [rng.getrandbits(memory.width*8) for _ in range(count)]
            strobes = []
            for index,payload in enumerate(data):
                address = start+(index << size)
                lane = address % memory.width
                strobe = (((1 << (1 << size))-1) << lane) & rng.getrandbits(memory.width)
                strobes.append(strobe)
                offset = address-memory.base-lane
                for byte in range(memory.width):
                    if strobe & (1 << byte):
                        oracle[offset+byte] = (payload >> (byte*8)) & 255
            await write(start,size,data,strobes,hold_address=count == 2,hold_data=count == 16)
            actual = await read(start,size,count)
            for index,payload in enumerate(actual):
                offset = start+(index << size)-memory.base
                offset -= offset % memory.width
                assert payload == int.from_bytes(oracle[offset:offset+memory.width],'little')
    assert b''.join(w.to_bytes(4,'little') for w in await memory.read_words(tick)) == oracle
    for writing in (False,True):
        memory.fail_next = True
        if writing:
            await write(memory.base,2,[123],[15],error=True)
        else:
            await read(memory.base,2,16,error=True)
        assert not memory.fail_next
    await read(memory.base+len(words)*4,2,1,error=True)
    assert b''.join(w.to_bytes(4,'little') for w in await memory.read_words(tick)) == oracle
    set(m_axi_araddr=memory.base, m_axi_arlen=15, m_axi_arsize=2, m_axi_arvalid=1, m_axi_rready=0)
    assert (await tick())['arready']
    set(m_axi_arvalid=0)
    for _ in range(5):
        await tick()
    assert int(dut.m_axi_rvalid.value)
    set(reset=1)
    await tick()
    set(reset=0)
    for _ in range(5):
        await tick()
        assert not int(dut.m_axi_rvalid.value) and not int(dut.sim_busy.value)
    assert b''.join(w.to_bytes(4,'little') for w in await memory.read_words(tick)) == oracle
