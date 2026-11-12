"""Block ownership, partial-range sharing, reuse and snapshot restore."""
import json
import os
from pathlib import Path
import random
import cocotb
from media_driver import Driver


@cocotb.test()
async def block_ownership(dut):
    capacities=tuple(map(int,os.environ['MEDIA_CAPACITIES'].split(',')))
    bc=int(os.environ['AXI_BLOCK_CHUNKS']);cf=capacities[2];bf=bc*cf
    output=Path(os.environ['MEDIA_OUTPUT'])
    initial=dict(block_chunks=bc,chunk_frames=cf,chunk_limit=capacities[1],samples=[],free_chunks=[],views=[])
    driver=Driver(dut,capacities,20261112)
    commands=0
    async def completed(kind,**fields):
        nonlocal commands
        commands+=1
        result=await driver.command(dict(kind=kind,**fields))
        assert result['status']==dict(kind='completed'),result['status']
        return result
    async def read_equals(view,data,position=0,count=None):
        count=len(data) if count is None else count
        result=await completed('read',view=view,position=position,length=len(data),frames=count)
        assert result['value']==dict(kind='samples',value=[data[(position+i)%len(data)] for i in range(count)])
    try:
        await driver.load(initial)
        await completed('create')
        values=[i*701-91000 for i in range(2*bf+3)]
        await completed('append',view=0,samples=values)
        assert (await completed('slice',source=0,start=bf-3,frames=bf+6))['value']==dict(kind='view',value=1)
        assert (await completed('slice',source=1,start=2,frames=bf+3))['value']==dict(kind='view',value=2)
        await completed('clear',view=0)
        await completed('append',view=0,samples=[1901]*cf)
        await read_equals(1,values[bf-3:2*bf+3],position=bf+5,count=3*cf+7)
        saved=await driver.snapshot()
        await driver.load(saved)
        assert await driver.snapshot()==saved
        await read_equals(0,[1901]*cf)
        await completed('remove',view=1)
        for data,finished in [(values[bf-1:2*bf],False),(values[bf-1:bf],False),([],True)]:
            assert (await completed('reclaim',view=2))['value']==dict(kind='empty',value=finished)
            if data:await read_equals(2,data,count=2*cf)
        await completed('remove',view=2)
        await completed('clear',view=0)
        snapshot=await driver.snapshot()
        assert len(snapshot['samples'])==4*bf,'retained blocks must not be reused by the writer'
        assert sorted(snapshot['free_chunks'])==list(range(4*bc))
        rng=random.Random(20261112)
        for _ in range(8):
            data=[rng.randrange(-1000000,1000000) for _ in range(rng.randrange(1,bf+3))]
            await completed('append',view=0,samples=data)
            await read_equals(0,data)
            await completed('clear',view=0)
        data=[i-12000 for i in range(capacities[1]*cf)]
        await completed('append',view=0,samples=data)
        await read_equals(0,data)
        before=await driver.snapshot()
        result=await driver.command(dict(kind='append',view=0,samples=[1]));commands+=1
        assert result['status']==dict(kind='failed',value=dict(error='memory_full'))
        assert result['media']==before,'refused append changed ownership or samples'
        await completed('clear',view=0)
        (output/'block-summary.json').write_text(json.dumps(dict(commands=commands,result='passed'))+'\n')
    finally:driver.close()
