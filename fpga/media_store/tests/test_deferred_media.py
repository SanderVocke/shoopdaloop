"""Deferred ownership, foreground latency, bounded maintenance and shared data."""
import json
import os
from pathlib import Path
import cocotb
from media_driver import Driver


def driver(dut):
    return Driver(dut,tuple(map(int,os.environ['MEDIA_CAPACITIES'].split(','))),20261112)


async def empty(d):
    await d.load(dict(block_chunks=d.block_chunks,chunk_frames=d.chunk_frames,
        chunk_limit=d.slots,samples=[],free_chunks=[],views=[]))
    await d.command(dict(kind='create'))


async def complete(d,kind,**fields):
    result=await d.command(dict(kind=kind,**fields),stalls=False,drain=False)
    assert result['status']==dict(kind='completed'),result
    return result


@cocotb.test()
async def detachment_and_maintenance_budgets(dut):
    d=driver(dut);rows=[]
    bf=d.chunk_frames*d.block_chunks
    try:
        for count in (1,17):
            await empty(d)
            await complete(d,'append',view=0,samples=[101]*(count*bf))
            walks=(int(dut.core.metadata.directory_reads.value),int(dut.core.metadata.table_reads.value))
            await complete(d,'clear',view=0)
            rows.append(dict(blocks=count,frames=count*bf,clear_cycles=d.timing))
            assert (int(dut.core.metadata.directory_reads.value),int(dut.core.metadata.table_reads.value))==walks
            assert await d.configure(6,0)==1 and await d.configure(6,1)==count
            assert await d.configure(0,2)==0
            if count>3:
                guard=int(os.environ['AXI_RECLAIM_GUARD'])
                await d.grant_maintenance(blocks=3,cycles=guard+1)
                for _ in range(100):await d.tick()
                assert int(dut.reclaim_blocks.value)==0
                await d.grant_maintenance(blocks=3,cycles=100000)
                for _ in range(100000):
                    if int(dut.reclaim_blocks.value)==3 and not int(dut.reclaim_active.value):break
                    await d.tick()
                else:raise AssertionError('bounded maintenance made no progress')
                for _ in range(100):await d.tick()
                assert int(dut.reclaim_blocks.value)==3
                assert await d.configure(6,1)==count-3
                assert await d.configure(0,2)==3
                await d.snapshot()
            await d.drain_reclamation()
            assert await d.configure(6,0)==0 and await d.configure(6,1)==0
            assert await d.configure(0,2)==count
        assert max(x['clear_cycles'] for x in rows)-min(x['clear_cycles'] for x in rows)<32,rows
        Path(os.environ['MEDIA_OUTPUT'],'deferred-timing.json').write_text(json.dumps(rows,indent=2)+'\n')
    finally:d.close()


@cocotb.test()
async def queue_full_preserves_the_live_recording(dut):
    d=driver(dut);bf=d.chunk_frames*d.block_chunks
    try:
        await empty(d)
        capacity=await d.configure(6,8)
        assert capacity==2,'run this gate with --retired-tables 2'
        for value in range(capacity):
            await complete(d,'append',view=0,samples=[value+1]*bf)
            await complete(d,'clear',view=0)
        await complete(d,'append',view=0,samples=[1901]*bf)
        before=await d.snapshot()
        result=await d.command(dict(kind='clear',view=0),drain=False)
        assert result['status']==dict(kind='failed',value=dict(error='memory_full'))
        assert result['media']==before
        assert await d.configure(6,0)==capacity
        await d.drain_reclamation()
        result=await complete(d,'read',view=0,position=0,length=bf,frames=bf)
        assert result['value']==dict(kind='samples',value=[1901]*bf)
        await complete(d,'remove',view=0)
        await d.drain_reclamation()
        assert await d.configure(0,2)==capacity+1
    finally:d.close()


@cocotb.test()
async def retained_ranges_and_repeated_view_reuse(dut):
    d=driver(dut);bf=d.chunk_frames*d.block_chunks
    data=[i*37-13000 for i in range(2*bf+7)]
    try:
        await empty(d)
        await complete(d,'append',view=0,samples=data)
        await complete(d,'slice',source=0,start=bf-3,frames=bf+6)
        await complete(d,'clear',view=0)
        await complete(d,'append',view=0,samples=[9001]*d.chunk_frames)
        assert await d.configure(0,1)==4
        await d.drain_reclamation()
        assert await d.configure(0,2)==0,'shared blocks became reusable'
        result=await complete(d,'read',view=1,position=0,length=bf+6,frames=bf+6)
        assert result['value']==dict(kind='samples',value=data[bf-3:2*bf+3])
        await complete(d,'remove',view=1)
        await d.drain_reclamation()
        assert await d.configure(0,2)==3
        for value in range(6):
            await complete(d,'clear',view=0)
            await d.drain_reclamation()
            await complete(d,'append',view=0,samples=[value+1]*d.chunk_frames)
        result=await complete(d,'read',view=0,position=0,length=d.chunk_frames,frames=d.chunk_frames)
        assert result['value']==dict(kind='samples',value=[6]*d.chunk_frames)
        await complete(d,'clear',view=0)
        await d.drain_reclamation()
        assert await d.configure(0,2)==await d.configure(0,1)
    finally:d.close()
