"""Reusable live IDs retain independent old roots and transactional slice admission."""
import os
import cocotb
from media_driver import Driver


def driver(dut):
    return Driver(dut,tuple(map(int,os.environ['MEDIA_CAPACITIES'].split(','))),20261113)


async def empty(d):
    await d.load(dict(block_chunks=d.block_chunks,chunk_frames=d.chunk_frames,
        chunk_limit=d.slots,samples=[],free_chunks=[],views=[]))


async def complete(d,kind,**fields):
    result=await d.command(dict(kind=kind,**fields),stalls=False,drain=False)
    assert result['status']==dict(kind='completed'),result['status']
    return result


@cocotb.test()
async def removed_view_reuse_keeps_the_retired_root_independent(dut):
    d=driver(dut);bf=d.chunk_frames*d.block_chunks
    try:
        await empty(d)
        await complete(d,'create')
        await complete(d,'append',view=0,samples=[1111]*bf)
        await complete(d,'remove',view=0)
        assert await d.configure(6,0)==1
        result=await complete(d,'create')
        assert result['value']==dict(kind='view',value=0)
        assert await d.configure(0,0)==1
        await complete(d,'append',view=0,samples=[7777]*bf)
        await d.drain_reclamation()
        result=await complete(d,'read',view=0,position=0,length=bf,frames=bf)
        assert result['value']==dict(kind='samples',value=[7777]*bf)
        assert await d.configure(0,2)==1
        await complete(d,'remove',view=0)
        await d.drain_reclamation()
        assert await d.configure(0,2)==2 and await d.configure(6,11)==0
        await d.snapshot()
    finally:d.close()


@cocotb.test()
async def full_view_space_slice_checks_all_pages_before_copying(dut):
    d=driver(dut);bf=d.chunk_frames*d.block_chunks
    try:
        await empty(d)
        assert d.table_pages==7
        await complete(d,'create')
        await complete(d,'append',view=0,samples=[1111]*bf)
        await complete(d,'create')
        await complete(d,'append',view=1,samples=[7777]*(17*bf))
        for _ in range(2,d.views):await complete(d,'create')
        await complete(d,'remove',view=0)
        before=await d.snapshot()
        assert await d.configure(6,11)==5 and await d.configure(6,12)==2
        result=await d.command(dict(kind='slice',source=1,start=0,frames=17*bf),drain=False)
        assert result['status']==dict(kind='failed',value=dict(error='memory_full'))
        assert result['media']==before
        assert await d.configure(6,11)==5,'failed slice leaked provisional pages'
        await d.drain_reclamation()
        result=await complete(d,'slice',source=1,start=0,frames=17*bf)
        assert result['value']==dict(kind='view',value=0)
        assert await d.configure(0,0)==d.views
        result=await complete(d,'read',view=0,position=0,length=17*bf,frames=17*bf)
        assert result['value']==dict(kind='samples',value=[7777]*(17*bf))
        await complete(d,'remove',view=0)
        await complete(d,'remove',view=1)
        await d.drain_reclamation()
        assert await d.configure(0,2)==18 and await d.configure(6,11)==0
        await d.snapshot()
    finally:d.close()
