"""Isolated reference-release batches; old sample payloads are unobserved."""
import hashlib
import json
import os
from pathlib import Path
import cocotb
from media_driver import Driver
from options import metadata


class TimingDriver(Driver):
    async def reset(self):
        if self.native_memory:self.axi.loaded=False
        await super().reset()

    async def load_memory(self):
        if not self.native_memory:return
        self.axi.image.write_text('00000000\n')
        await self.axi.exchange(1,self.tick)
        self.axi.loaded=True


@cocotb.test()
async def isolated_twenty_releases(dut):
    d=TimingDriver(dut,tuple(map(int,os.environ['MEDIA_CAPACITIES'].split(','))),20261113)
    rows=[];bf=d.chunk_frames*d.block_chunks;count=20
    assert d.blocks>=count
    try:
        for cold,owners,groups in ((False,1,1),(True,1,1),(False,1,20),(True,1,20),(True,2,1)):
            await d.reset();d.stalls=False
            per_table=count//groups
            for group in range(groups):
                for owner in range(owners):
                    view=group*owners+owner
                    for ordinal in range(per_table):
                        await d.configure(2,d.table_address(view,ordinal),group*per_table+ordinal)
                    await d.configure(1,view,1|((per_table*bf)<<64)|(per_table<<128))
            for block in range(count):await d.configure(3,block,owners)
            await d.configure(0,0,owners*groups)
            await d.configure(0,1,count)
            await d.configure(0,2,0)
            await d.fence()
            for group in range(groups):
                result=await d.command(dict(kind='clear',view=group*owners),stalls=False,drain=False,observe=False)
                assert result['status']['kind']=='completed'
            if cold:await d.configure(5,0,0)
            start=d.cycles;active_start=int(dut.reclaim_cycles.value)
            reads=d.read_requests;writes=d.write_requests
            await d.grant_maintenance(blocks=count,cycles=100000)
            for _ in range(100000):
                if int(dut.reclaim_blocks.value)==count and not int(dut.reclaim_active.value):break
                await d.tick()
            else:raise AssertionError('20-release timing batch did not complete')
            elapsed=d.cycles-start
            row=dict(cold_metadata=cold,owners_per_block=owners,retired_tables=groups,released_references=count,
                freed_blocks=int(dut.reclaim_freed.value),elapsed_cycles=elapsed,
                active_collector_cycles=int(dut.reclaim_cycles.value)-active_start,
                axi_read_bursts=d.read_requests-reads,axi_write_bursts=d.write_requests-writes,
                microseconds_at_100mhz=elapsed/100,quantum_fraction_at_48khz_q32=elapsed/100000000/(32/48000))
            assert row['freed_blocks']==(count if owners==1 else 0)
            assert int(dut.reclaim_pending.value)==0
            await d.fence()
            row.update(fence_cycles=d.cycles-start-elapsed,
                settled_cycles=d.cycles-start,settled_microseconds_at_100mhz=(d.cycles-start)/100,
                axi_read_bursts_including_fence=d.read_requests-reads,
                axi_write_bursts_including_fence=d.write_requests-writes)
            rows.append(row)
        root=Path(__file__).resolve().parents[3]
        paths=[*root.glob('fpga/media_store/rtl/*.sv'),*root.glob('fpga/chunk_memory/rtl/*.sv')]
        result=dict(scope='Isolated maintenance; no competing audio or dirty sample payloads; includes queue/page cleanup and grant issue overhead.',
            profiling_grant=dict(block_ceiling=count,cycles=100000),
            memory=metadata(),capacities=dict(views=d.views,physical_chunks=d.slots,blocks=d.blocks,
                chunk_frames=d.chunk_frames,block_chunks=d.block_chunks,metadata_pages=d.table_pages),
            sources={str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest() for p in paths},batches=rows)
        Path(os.environ['MEDIA_OUTPUT'],'reclaim-timing.json').write_text(json.dumps(result,indent=2)+'\n')
    finally:d.close()
