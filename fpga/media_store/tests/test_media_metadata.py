import cocotb
import os
from cocotb.clock import Clock
from cocotb.triggers import Timer, RisingEdge
from direct_memory import DirectMemory


@cocotb.test()
async def indexed_metadata_reuse_and_limits(dut):
    def set(**values):
        for name,value in values.items():getattr(dut,name).value=value
    async def tick():
        await Timer(1,units='ns');await RisingEdge(dut.clk);await Timer(1,units='ns')
    set(reset=1,request_valid=0,request_opcode=0,request_view=0,request_index=0,request_data=0,response_ready=0)
    clock=cocotb.start_soon(Clock(dut.clk,10,units='ns').start())
    memory=DirectMemory(dut,[0]*8192)
    await tick();set(reset=0);await tick();await memory.load_words([0]*8192,tick)
    async def request(op,view=0,index=0,data=0,error=0):
        set(request_valid=1,request_opcode=op,request_view=view,request_index=index,request_data=data,response_ready=0)
        for _ in range(100000):
            await Timer(1,units='ns');ready=int(dut.request_ready.value);await tick()
            if ready:break
        else:raise AssertionError('metadata admission timeout')
        set(request_valid=0)
        for _ in range(100000):
            if int(dut.response_valid.value):break
            await tick()
        else:raise AssertionError(f'metadata timeout op={op} view={view} index={index}')
        assert int(dut.response_error.value)==error
        value=int(dut.response_data.value)
        set(response_ready=1);await tick();set(response_ready=0)
        return value
    await request(7,0,0)
    await request(0,0,0,error=1)
    await request(1,0,int(os.environ.get('METADATA_SERVICE_SLOTS','256')),123,error=1)
    slots=int(os.environ.get('METADATA_SERVICE_SLOTS','256'))
    levels=((slots-1).bit_length()+3)//4
    for i in range(32):await request(1,0,i,100+i)
    for i in range(16):await request(1,1,i,200+i)
    high=int(dut.metadata.high_water.value)
    await request(7,2,slots,error=3)
    await request(1,2,slots-1,123,error=3)
    assert int(dut.metadata.high_water.value)==high
    for view,count in [(0,32),(1,16)]:
        for i in reversed(range(count)):assert await request(0,view,i)==100*(view+1)+i
    for i in range(70):
        await request(3,index=i,data=i+1)
        await request(5,index=i,data=69-i)
    for i in reversed(range(70)):
        assert await request(2,index=i)==i+1
        assert await request(4,index=i)==69-i
    await request(6,0,17)
    assert int(dut.metadata.free_pages.value)==0
    await request(6,0,16)
    assert int(dut.metadata.free_pages.value)==1
    await request(6,0,0)
    assert int(dut.metadata.free_pages.value)==levels+1
    for i in range(32):await request(1,2,i,300+i)
    assert int(dut.metadata.high_water.value)==high
    for i in range(32):assert await request(0,2,i)==300+i
    if slots>256:
        for i in range(272):await request(1,3,i,400+i)
        for i in (0,15,16,255,256,271):assert await request(0,3,i)==400+i
        before=int(dut.metadata.free_pages.value)
        high=int(dut.metadata.high_water.value)
        await request(6,3,256)
        assert int(dut.metadata.free_pages.value)==before+2
        assert await request(0,3,255)==655
        await request(6,3,240)
        assert int(dut.metadata.free_pages.value)==before+3
        await request(6,3,0)
        for i in range(16):await request(1,3,i,700+i)
        assert int(dut.metadata.high_water.value)==high
    await request(9)
    image=await memory.read_words(tick)
    assert image[slots:slots+70]==list(range(1,71))
    assert image[2*slots:2*slots+70]==list(reversed(range(70)))
    memory.fail_next=True
    await request(0,2,0,error=5)
    assert int(dut.fault.value)
    for op in (2,4,3,5):
        set(reset=1);await tick();set(reset=0);await tick()
        if op in (3,5):
            await request(op,index=0,data=123)
            memory.fail_next=True
            await request(8,error=5)
        else:
            memory.fail_next=True
            await request(op,index=0,error=5)
        assert int(dut.fault.value)
    clock.kill()
