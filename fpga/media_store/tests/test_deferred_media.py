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
        if os.environ['AXI_MEMORY_MODEL']!='dramsim3':
            assert max(x['clear_cycles'] for x in rows)-min(x['clear_cycles'] for x in rows)<32,rows
        Path(os.environ['MEDIA_OUTPUT'],'deferred-timing.json').write_text(json.dumps(rows,indent=2)+'\n')
    finally:d.close()


@cocotb.test()
async def clear_cancels_an_outstanding_metadata_hint(dut):
    d=driver(dut);bf=d.chunk_frames*d.block_chunks
    try:
        await empty(d)
        await complete(d,'append',view=0,samples=[1901]*(2*bf))
        await d.configure(5,0,0)
        d.pause_responses=True
        d.set(hint_valid=1,hint_view=0,hint_position=0,hint_length=2*bf,hint_count=d.chunk_frames)
        for _ in range(10000):
            ready=int(dut.hint_ready.value);await d.tick()
            if ready:break
        else:raise AssertionError('hint admission timeout')
        d.set(hint_valid=0)
        for _ in range(10000):
            await d.tick()
            if (d.axi if d.native_memory else d.axi.memory).active is not None:break
        else:raise AssertionError('hint did not reach memory')
        d.set(cmd_valid=1,cmd_opcode=2,cmd_view=0,cmd_start=0,cmd_length=0,cmd_count=0)
        for _ in range(10000):
            before,after=await d.tick()
            if before['cmd_ready']:break
        else:raise AssertionError('clear admission timeout')
        d.set(cmd_valid=0)
        for _ in range(10):await d.tick()
        d.pause_responses=False
        for _ in range(100000):
            _,after=await d.tick()
            if after['done']:break
        else:raise AssertionError('clear did not drain the old hint')
        assert after['error_code']==0
        await d.tick()
        await complete(d,'append',view=0,samples=[7777]*d.chunk_frames)
        await d.drain_reclamation()
        result=await complete(d,'read',view=0,position=0,length=d.chunk_frames,frames=d.chunk_frames)
        assert result['value']==dict(kind='samples',value=[7777]*d.chunk_frames)
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
        await empty(d)
        for ordinal in range(2):await d.configure(2,d.table_address(0,ordinal),0)
        await d.configure(1,0,3|((2*bf)<<64)|(2<<128))
        await d.configure(3,0,2)
        await d.configure(0,1,1)
        await d.command(dict(kind='clear',view=0),drain=False,observe=False)
        await d.drain_reclamation()
        assert await d.configure(0,2)==1,'duplicate entries released the same physical block twice'
        assert await d.configure(3,0)==0 and await d.configure(6,11)==0
    finally:d.close()


@cocotb.test()
async def reset_discards_active_cleanup_and_old_responses(dut):
    d=driver(dut);bf=d.chunk_frames*d.block_chunks
    try:
        await empty(d)
        await complete(d,'append',view=0,samples=[1901]*(2*bf))
        await complete(d,'clear',view=0)
        await d.configure(5,0,0)
        d.pause_responses=True
        await d.grant_maintenance(blocks=20,cycles=100000)
        for _ in range(10000):
            await d.tick()
            if (d.axi if d.native_memory else d.axi.memory).active is not None:break
        else:raise AssertionError('cleanup did not issue a transfer')
        assert int(dut.reclaim_active.value)
        d.pause_responses=False
        await empty(d)
        assert int(dut.reclaim_pending.value)==0 and not int(dut.reclaim_active.value)
        await complete(d,'append',view=0,samples=[7777]*d.chunk_frames)
        result=await complete(d,'read',view=0,position=0,length=d.chunk_frames,frames=d.chunk_frames)
        assert result['value']==dict(kind='samples',value=[7777]*d.chunk_frames)
    finally:d.close()


@cocotb.test()
async def invalid_retired_block_faults_before_reuse(dut):
    d=driver(dut);bf=d.chunk_frames*d.block_chunks
    try:
        await empty(d)
        await complete(d,'append',view=0,samples=[1901]*bf)
        await d.configure(2,d.table_address(0,0),d.blocks-1)
        result=await d.command(dict(kind='clear',view=0),drain=False,observe=False)
        assert result['status']['kind']=='completed'
        await d.grant_maintenance(blocks=20,cycles=100000)
        for _ in range(10000):
            await d.tick()
            if int(dut.memory_fault.value):break
        else:raise AssertionError('invalid ownership did not fault')
        assert int(dut.reclaim_freed.value)==0
        assert int(dut.reclaim_pending.value)==1
        assert await d.configure(0,2)==0
        await empty(d)
        await complete(d,'append',view=0,samples=[7777]*d.chunk_frames)
    finally:d.close()


@cocotb.test()
async def status_reads_do_not_deadlock_atomic_cleanup(dut):
    d=driver(dut);bf=d.chunk_frames*d.block_chunks
    try:
        await empty(d)
        await complete(d,'append',view=0,samples=[1901]*(2*bf))
        await complete(d,'clear',view=0)
        await d.grant_maintenance(blocks=20,cycles=100000)
        for _ in range(10000):
            await d.tick()
            if int(dut.core.gc_lock_held.value):break
        else:raise AssertionError('collector did not acquire ownership')
        assert await d.configure(6,0)==1
        await d.drain_reclamation()
        assert await d.configure(0,2)==2
        assert not int(dut.memory_fault.value)
    finally:d.close()


@cocotb.test()
async def pause_at_ownership_and_checkpoint_boundaries(dut):
    d=driver(dut);bf=d.chunk_frames*d.block_chunks
    try:
        for target in range(2,18):
            await empty(d)
            await complete(d,'append',view=0,samples=[1901]*bf)
            shared=target in (6,7)
            if shared:await complete(d,'slice',source=0,start=0,frames=bf)
            await complete(d,'clear',view=0)
            await d.grant_maintenance(blocks=20,cycles=1000000)
            for _ in range(100000):
                if int(dut.core.reclaimer.phase.value)==target:break
                await d.tick()
            else:raise AssertionError(f'collector never reached phase {target}')
            d.set(maintenance_enable=0)
            await d.snapshot()
            pending=int(dut.reclaim_references.value)
            for _ in range(20):await d.tick()
            assert int(dut.reclaim_references.value)==pending,'paused collector advanced'
            d.set(maintenance_enable=1)
            await d.drain_reclamation()
            assert await d.configure(0,2)==int(not shared)
            if shared:
                result=await complete(d,'read',view=1,position=0,length=bf,frames=bf)
                assert result['value']==dict(kind='samples',value=[1901]*bf)
                await complete(d,'remove',view=1)
                await d.drain_reclamation()
            assert await d.configure(0,2)==1 and await d.configure(6,11)==0
    finally:d.close()


@cocotb.test()
async def offline_drain_survives_budget_counter_expiration(dut):
    d=driver(dut);bf=d.chunk_frames*d.block_chunks
    try:
        await empty(d)
        await complete(d,'append',view=0,samples=[1901]*(3*bf))
        await complete(d,'clear',view=0)
        d.set(maintenance_enable=0,cmd_valid=1,cmd_opcode=10,cmd_view=0,
            cmd_start=0,cmd_length=0,cmd_count=0)
        for _ in range(10000):
            before,after=await d.tick()
            if before['cmd_ready']:break
        else:raise AssertionError('offline drain admission timeout')
        d.set(cmd_valid=0)
        await d.tick()
        dut.core.reclaimer.cycles_left.value=0
        dut.core.reclaimer.blocks_left.value=0
        for _ in range(100000):
            _,after=await d.tick()
            if after['done']:break
        else:raise AssertionError('offline drain stopped when its counter expired')
        assert after['error_code']==0
        await d.tick()
        assert int(dut.reclaim_pending.value)==0
        assert await d.configure(0,2)==3 and await d.configure(6,11)==0
        d.set(maintenance_enable=1)
        await complete(d,'append',view=0,samples=[7777]*bf)
        await complete(d,'clear',view=0)
        for _ in range(1000):await d.tick()
        assert int(dut.reclaim_pending.value)==1,'offline drain left an unlimited automatic grant'
        await d.drain_reclamation()
    finally:d.close()
