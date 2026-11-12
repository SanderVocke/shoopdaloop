"""Retire one allocation block while cached data and transfers remain live."""
import os
import cocotb
from chunk_driver import Driver


@cocotb.test()
async def block_retirement(dut):
    capacities=tuple(map(int,os.environ['CHUNK_CAPACITIES'].split(',')))
    bc=int(os.environ['CHUNK_RETIRE_CHUNKS']);slots,cf,reads,_=capacities
    driver=Driver(dut,capacities,20261112);driver.stalls=False
    source=[i*19-1000 for i in range(slots*cf)]
    try:
        await driver.reset(source)
        for chunk in (1,bc-1):
            assert await driver.command(dict(kind='read',chunk=chunk,frames=cf),stalls=False)==(0,source[chunk*cf:(chunk+1)*cf])
        for chunk,value in ((0,101),(bc-1,131),(bc,202)):
            assert await driver.command(dict(kind='write',chunk=chunk,offset=0,samples=[value]*cf),stalls=False)==(0,[])
        driver.pause=True
        driver.set(cmd_valid=1,cmd_opcode=4,cmd_id=73,cmd_chunk=0,cmd_offset=0,cmd_count=0,response_ready=0)
        for _ in range(1000):
            before,_=await driver.tick()
            if before['cmd_ready']:break
        else:raise AssertionError('retirement admission timeout')
        driver.set(cmd_valid=0)
        for _ in range(20):await driver.tick()
        assert not int(dut.response_valid.value),'retirement published reuse before dirty data drained'
        driver.pause=False
        for _ in range(100000):
            if int(dut.response_valid.value):break
            await driver.tick()
        else:raise AssertionError('retirement drain timeout')
        for _ in range(4):
            assert int(dut.response_valid.value) and int(dut.response_id.value)==73 and int(dut.response_error.value)==0
            await driver.tick()
        driver.set(response_ready=1);await driver.tick();driver.set(response_ready=0)
        assert await driver.command(dict(kind='fence'),stalls=False)==(0,[])
        image=await driver.read_memory()
        assert image[:cf]==[101]*cf and image[(bc-1)*cf:bc*cf]==[131]*cf
        assert image[bc*cf:(bc+1)*cf]==[202]*cf
        for chunk in (1,bc-1):
            image[chunk*cf:(chunk+1)*cf]=[9000+chunk]*cf
        driver.memory[:]=image;await driver.load_memory()
        for chunk in (1,bc-1):
            assert await driver.command(dict(kind='read',chunk=chunk,frames=cf),stalls=False)==(0,[9000+chunk]*cf)
        assert await driver.command(dict(kind='retire',chunk=1),stalls=False)==(4,[])

        await driver.reset(source)
        driver.pause=True
        await driver.hint(2)
        for _ in range(5):await driver.tick()
        driver.set(cmd_valid=1,cmd_opcode=4,cmd_id=74,cmd_chunk=0,cmd_offset=0,cmd_count=0,response_ready=0)
        for _ in range(1000):
            before,_=await driver.tick()
            if before['cmd_ready']:break
        else:raise AssertionError('prefetch retirement admission timeout')
        driver.set(cmd_valid=0)
        for _ in range(10):await driver.tick()
        assert not int(dut.response_valid.value)
        driver.pause=False
        for _ in range(100000):
            if int(dut.response_valid.value):break
            await driver.tick()
        else:raise AssertionError('in-flight prefetch retirement timeout')
        driver.set(response_ready=1);await driver.tick();driver.set(response_ready=0)
        image=await driver.read_memory();image[2*cf:3*cf]=[7777]*cf
        driver.memory[:]=image;await driver.load_memory()
        assert await driver.command(dict(kind='read',chunk=2,frames=cf),stalls=False)==(0,[7777]*cf)

        await driver.reset(source)
        assert await driver.command(dict(kind='write',chunk=0,samples=[42]*cf),stalls=False)==(0,[])
        driver.fail_next=True
        assert await driver.command(dict(kind='retire',chunk=0),stalls=False)==(5,[])
        assert int(dut.fault.value)
        await driver.reset(source)
        assert await driver.command(dict(kind='read',chunk=0,frames=cf),stalls=False)==(0,source[:cf])
    finally:driver.close()
