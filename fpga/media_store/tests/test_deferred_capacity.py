"""Reclamation progress without spare pages/blocks, and failed ownership transfer."""
import os
import cocotb
from media_driver import Driver


def driver(dut):
    return Driver(dut,tuple(map(int,os.environ['MEDIA_CAPACITIES'].split(','))),20261112)


@cocotb.test()
async def full_pools_and_pause_after_reference_release(dut):
    d=driver(dut);bf=d.chunk_frames*d.block_chunks
    try:
        assert d.table_pages==3,'run with --metadata-pages 3'
        await d.load(dict(block_chunks=d.block_chunks,chunk_frames=d.chunk_frames,
            chunk_limit=17*d.block_chunks,samples=[],free_chunks=[],views=[]))
        await d.command(dict(kind='create'))
        result=await d.command(dict(kind='append',view=0,samples=[123]*(17*bf)))
        assert result['status']['kind']=='completed'
        assert int(dut.core.metadata.high_water.value)==3
        assert int(dut.core.metadata.free_pages.value)==0
        await d.command(dict(kind='clear',view=0),drain=False)
        assert await d.configure(0,2)==0 and await d.configure(6,1)==17
        result=await d.command(dict(kind='append',view=0,samples=[456]*d.chunk_frames),drain=False)
        assert result['status']==dict(kind='failed',value=dict(error='memory_full'))
        await d.grant_maintenance(blocks=17,cycles=1000000)
        for _ in range(100000):
            await d.tick()
            if int(dut.reclaim_released.value):break
        else:raise AssertionError('no reference release with full pools')
        d.set(maintenance_enable=0)
        await d.snapshot()
        assert await d.configure(6,1)==17,'paused cleanup lost its table cursor'
        assert await d.configure(0,2)==1
        d.set(maintenance_enable=1)
        await d.drain_reclamation()
        assert await d.configure(0,2)==17
        assert int(dut.core.metadata.free_pages.value)==3
        result=await d.command(dict(kind='append',view=0,samples=[456]*(17*bf)))
        assert result['status']['kind']=='completed'
        assert int(dut.core.metadata.high_water.value)==3,'recovered pages were not reused'
    finally:d.close()


@cocotb.test()
async def failed_detachment_preserves_ownership_until_reset(dut):
    d=driver(dut);bf=d.chunk_frames*d.block_chunks
    initial=dict(block_chunks=d.block_chunks,chunk_frames=d.chunk_frames,
        chunk_limit=d.slots,samples=[],free_chunks=[],views=[])
    try:
        await d.load(initial)
        await d.command(dict(kind='create'))
        await d.command(dict(kind='append',view=0,samples=[1901]*bf))
        descriptor=await d.configure(1,0)
        d.fail_next=True
        result=await d.command(dict(kind='clear',view=0),drain=False,observe=False)
        assert result['status']['kind']=='failed'
        assert int(dut.memory_fault.value)
        assert await d.configure(1,0)==descriptor
        assert int(dut.reclaim_pending.value)==0
        await d.load(initial)
        await d.command(dict(kind='create'))
        result=await d.command(dict(kind='append',view=0,samples=[7777]*bf))
        assert result['status']['kind']=='completed'
        await d.command(dict(kind='clear',view=0),drain=False)
        await d.drain_reclamation()
        assert await d.configure(0,2)==1
    finally:d.close()


@cocotb.test()
async def published_retirement_fault_requires_a_coordinated_reset(dut):
    d=driver(dut);bf=d.chunk_frames*d.block_chunks
    initial=dict(block_chunks=d.block_chunks,chunk_frames=d.chunk_frames,
        chunk_limit=d.slots,samples=[],free_chunks=[],views=[])
    try:
        await d.load(initial)
        await d.command(dict(kind='create'))
        await d.command(dict(kind='append',view=0,samples=[1901]*bf))
        await d.command(dict(kind='clear',view=0),drain=False,observe=False)
        assert (await d.configure(1,0))>>128==0
        assert int(dut.reclaim_pending.value)==1 and int(dut.reclaim_references.value)==1
        d.fail_next=True
        d.set(cmd_valid=1,cmd_opcode=7,cmd_view=0,cmd_start=0,cmd_length=0,cmd_count=0)
        for _ in range(100000):
            before,after=await d.tick()
            if before['cmd_ready']:break
        else:raise AssertionError('fence admission timeout')
        d.set(cmd_valid=0)
        for _ in range(100000):
            if after['done']:break
            _,after=await d.tick()
        else:raise AssertionError('faulted fence completion timeout')
        assert after['error_code']==5 and int(dut.memory_fault.value)
        assert int(dut.reclaim_pending.value)==1 and int(dut.reclaim_freed.value)==0
        await d.tick()
        assert not int(dut.cmd_ready.value)
        await d.load(initial)
        assert int(dut.reclaim_pending.value)==0
        await d.command(dict(kind='create'))
        result=await d.command(dict(kind='append',view=0,samples=[7777]*bf))
        assert result['status']['kind']=='completed'
    finally:d.close()
