"""Large preloaded ownership tables; old payloads are deliberately unobserved."""
import json
import os
import hashlib
import subprocess
from pathlib import Path
import cocotb
from cocotb.triggers import RisingEdge, FallingEdge, Timer, with_timeout
from cocotb.utils import get_sim_time
from media_driver import Driver
from options import metadata


class OwnershipDriver(Driver):
    SIGNALS=('busy','done','error_code','cmd_ready','result_view','result_empty',
        'write_ready','read_valid','read_data','cfg_ready','cfg_response_valid','cfg_response_error','cfg_read_data')

    def set(self,**values):
        values.pop('clk',None)
        super().set(**values)

    def clock_position(self):
        self.cycles=int(get_sim_time(units='ns'))//10

    async def tick(self):
        await FallingEdge(self.dut.clk)
        before=self.sample()
        await RisingEdge(self.dut.clk)
        await Timer(1,units='ns')
        self.clock_position()
        return before,self.sample()

    async def configure(self,space,address,value=None,*,expect_error=False):
        self.set(cfg_valid=1,cfg_space=space,cfg_address=address,cfg_write=int(value is not None),
            cfg_write_data=0 if value is None else value)
        await Timer(1,units='ns')
        if not int(self.dut.cfg_ready.value):
            await with_timeout(RisingEdge(self.dut.cfg_ready),100000,'ns')
        await RisingEdge(self.dut.clk);await Timer(1,units='ns')
        self.set(cfg_valid=0)
        if not int(self.dut.cfg_response_valid.value):
            await with_timeout(RisingEdge(self.dut.cfg_response_valid),100000,'ns')
        await Timer(1,units='ns')
        assert bool(int(self.dut.cfg_response_error.value))==expect_error
        result=int(self.dut.cfg_read_data.value)
        if expect_error or value is not None:assert result==0
        await RisingEdge(self.dut.clk);await Timer(1,units='ns')
        self.clock_position()
        return result

    async def drain_reclamation(self):
        if await self.configure(6,0)==0:return
        limit=(int(self.dut.reclaim_references.value)*2048+100000)*10
        self.set(cmd_valid=1,cmd_opcode=10,cmd_view=0,cmd_start=0,cmd_length=0,cmd_count=0)
        await Timer(1,units='ns')
        if not int(self.dut.cmd_ready.value):await with_timeout(RisingEdge(self.dut.cmd_ready),100000,'ns')
        await RisingEdge(self.dut.clk);await Timer(1,units='ns')
        self.set(cmd_valid=0)
        if not int(self.dut.done.value):await with_timeout(RisingEdge(self.dut.done),limit,'ns')
        await Timer(1,units='ns')
        assert int(self.dut.error_code.value)==0
        await RisingEdge(self.dut.clk);await Timer(1,units='ns')
        self.clock_position()

    async def load_memory(self):
        assert self.native_memory
        self.axi.image.write_text('00000000\n')
        await self.axi.exchange(1,self.tick)
        self.axi.loaded=True


async def campaign(dut,loops,channels):
    capacities=tuple(map(int,os.environ['MEDIA_CAPACITIES'].split(',')))
    d=OwnershipDriver(dut,capacities,20261112)
    seconds=int(os.environ.get('MEDIA_STRESS_SECONDS','30'))
    count=loops*channels;frames=seconds*48000;bf=d.chunk_frames*d.block_chunks
    blocks=(frames+bf-1)//bf;total=count*blocks
    report=dict(loops=loops,channels_per_loop=channels,recording_frames=frames,
        sample_rate=48000,recording_seconds=seconds,chunk_frames=d.chunk_frames,
        block_chunks=d.block_chunks,blocks_per_take=blocks,released_references=total,
        view_capacity=d.views,physical_chunk_capacity=d.slots,allocation_block_capacity=d.blocks,
        metadata_page_capacity=d.table_pages,
        scope='media-store ownership stress; old payloads unobserved; new captures/readback checked')
    output=Path(os.environ['MEDIA_OUTPUT'])
    report['configuration']=metadata()
    report['model_sha256']=hashlib.sha256((output/'build/media_store_axi').read_bytes()).hexdigest()
    report['generated_top_sha256']=hashlib.sha256((output/'media_store_axi.sv').read_bytes()).hexdigest()
    report['verilator']=subprocess.check_output(['verilator','--version'],text=True).strip()
    root=Path(__file__).resolve().parents[3]
    paths=[*root.glob('fpga/media_store/rtl/*.sv'),*root.glob('fpga/chunk_memory/rtl/*.sv'),
        *root.glob('fpga/axi_memory/rtl/*.sv'),root/'fpga/axi_memory/tests/direct/media_store_axi.sv',
        root/'fpga/axi_memory/tests/axi_direct_memory.sv']
    report['sources']={str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}
    def write_progress(stage,**values):
        (output/f'progress-{loops}x{channels}.json').write_text(json.dumps(dict(stage=stage,**values))+'\n')
    try:
        await d.reset();d.stalls=False
        for view in range(count):
            for ordinal in range(blocks):
                block=view*blocks+ordinal
                await d.configure(2,d.table_address(view,ordinal),block)
                await d.configure(3,block,1)
            await d.configure(1,view,1|(frames<<64)|(blocks<<128))
            if view%4==3:write_progress('preload',views=view+1)
        await d.configure(0,0,count)
        await d.configure(0,1,total)
        await d.configure(0,2,0)
        assert await d.configure(6,11)>0
        before=(int(dut.core.metadata.directory_reads.value),int(dut.core.metadata.table_reads.value))
        times=[]
        clear_start=d.cycles
        for view in range(count):
            result=await d.command(dict(kind='clear',view=view),stalls=False,drain=False,observe=False)
            assert result['status']['kind']=='completed'
            times.append(d.timing)
        assert (int(dut.core.metadata.directory_reads.value),int(dut.core.metadata.table_reads.value))==before
        assert int(dut.reclaim_pending.value)==count
        assert int(dut.reclaim_references.value)==total
        report['clear_cycles']=times
        report['mass_clear_cycles']=d.cycles-clear_start
        report['logical_bytes_retired']=count*frames*4
        report['allocated_bytes_retired']=total*bf*4
        report['block_id_bytes_retired']=total*4
        report['queue_record_bytes']=count*64
        report['metadata_pages_before_cleanup']=await d.configure(6,11)
        progress=[]
        for iteration in range(40):
            await d.grant_maintenance(blocks=20,cycles=10000)
            for view in range(count):
                result=await d.command(dict(kind='append',view=view,samples=[view+1]*32),
                    stalls=False,drain=False,observe=False)
                assert result['status']['kind']=='completed',(iteration,view,result)
            for _ in range(100):await d.tick()
            progress.append(dict(iteration=iteration,free_blocks=await d.configure(0,2),
                pending_references=await d.configure(6,1),freed_blocks=int(dut.reclaim_freed.value)))
            write_progress('recording',**progress[-1])
        assert int(dut.reclaim_freed.value)>count,'cleanup failed to sustain new block allocations'
        report['recording_progress']=progress
        write_progress('draining',pending_references=int(dut.reclaim_references.value))
        await d.drain_reclamation()
        assert int(dut.reclaim_pending.value)==0
        assert int(dut.reclaim_blocks.value)==total
        report['maintenance_cycles']=int(dut.reclaim_cycles.value)
        report['metadata_pages_after_cleanup']=await d.configure(6,11)
        for view in range(count):
            result=await d.command(dict(kind='read',view=view,position=0,length=1280,frames=1280),
                stalls=False,drain=False,observe=False)
            assert result['value']==dict(kind='samples',value=[view+1]*1280)
            await d.command(dict(kind='clear',view=view),stalls=False,drain=False,observe=False)
        await d.drain_reclamation()
        assert await d.configure(6,11)==0
        allocated=await d.configure(0,1)
        assert await d.configure(0,2)==allocated
        recovered={await d.configure(4,index) for index in range(allocated)}
        assert recovered==set(range(allocated)),'free stack has duplicate or missing block IDs'
        report['recovered_unique_blocks']=len(recovered)
        report['result']='passed'
        Path(os.environ['MEDIA_OUTPUT'],f'stress-{loops}x{channels}.json').write_text(json.dumps(report,indent=2)+'\n')
    finally:d.close()


@cocotb.test()
async def thirty_second_mono_takes(dut):
    await campaign(dut,32,1)


@cocotb.test()
async def thirty_second_stereo_takes(dut):
    await campaign(dut,16,2)
