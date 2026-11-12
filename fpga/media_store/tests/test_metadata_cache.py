import os
import random
import cocotb
from cocotb.clock import Clock
from cocotb.triggers import RisingEdge, Timer, Combine
from direct_memory import DirectMemory


@cocotb.test()
async def coherent_cache_and_transport(dut):
    def set(**values):
        for name,value in values.items():getattr(dut,name).value=value

    async def tick():
        await Timer(1,units='ns')
        await RisingEdge(dut.clk)
        await Timer(1,units='ns')

    set(reset=1)
    for i in range(2):
        set(**{f'c{i}_'+k:v for k,v in dict(request_valid=0,request_opcode=0,request_invalidate=0,
            request_address=0,request_data=0,response_ready=0).items()})
    clock=cocotb.start_soon(Clock(dut.clk,10,units='ns').start())
    memory=DirectMemory(dut,[0]*8192)
    await tick();set(reset=0);await tick()
    rng=random.Random(7654)
    image=[rng.getrandbits(32) for _ in range(8192)]
    await memory.load_words(image,tick)
    words=int(os.environ['METADATA_LINE_WORDS'])

    async def request(client,op,address=0,data=0,invalidate=False,error=False):
        prefix=f'c{client}_'
        def get(name):return int(getattr(dut,prefix+name).value)
        def drive(**values):set(**{prefix+k:v for k,v in values.items()})
        drive(request_valid=1,request_opcode=op,request_address=address,request_data=data,
              request_invalidate=int(invalidate),response_ready=0)
        for _ in range(20000):
            await Timer(1,units='ns')
            ready=get('request_ready')
            await tick()
            if ready:break
        else:raise AssertionError('request timeout')
        drive(request_valid=0)
        for _ in range(20000):
            if get('response_valid'):break
            await tick()
        else:raise AssertionError('response timeout')
        result=get('response_data')
        assert bool(get('response_error'))==error
        for _ in range(3):
            await tick();assert get('response_valid') and get('response_data')==result
        drive(response_ready=1);await tick();drive(response_ready=0)
        return result

    async def stream(client):
        start=client*2048
        for row in range(8):
            index=start+row*words
            for j in (0,words-1):
                assert await request(client,0,memory.base+4*(index+j))==image[index+j]
            value=rng.getrandbits(32)
            await request(client,1,memory.base+4*index,value)
            image[index]=value
            assert await request(client,0,memory.base+4*index)==value
        await request(client,3)
    await Combine(cocotb.start_soon(stream(0)),cocotb.start_soon(stream(1)))
    assert await memory.read_words(tick)==image
    fills=int(dut.c0_fills.value)
    for _ in range(5):assert await request(0,0,memory.base+4*(7*words))==image[7*words]
    assert int(dut.c0_fills.value)==fills
    await request(0,2,memory.base)
    image[:words]=[0]*words
    await request(0,3,invalidate=True)
    assert await memory.read_words(tick)==image
    memory.fail_next=True
    await request(0,0,memory.base,error=True)
    assert int(dut.c0_fault.value)
    set(reset=1);await tick();set(reset=0);await tick()
    assert not int(dut.c0_fault.value)
    assert await request(0,0,memory.base)==0
    assert await memory.read_words(tick)==image
    clock.kill()
