"""Drive the coordinator and sample/state ports without replacing its phase logic."""
import importlib.util
from pathlib import Path
from cocotb.triggers import FallingEdge, RisingEdge, Timer, with_timeout
import codec as tp
from driver import Driver

def load_module(name,path):
    spec=importlib.util.spec_from_file_location(name,path)
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    return module

sp=load_module('sp_abi',Path(__file__).resolve().parents[2]/'state_processor/tests/codec.py')
ERRORS=['invalid_graph','invalid_state','invalid_buffer','invalid_view','invalid_shape','queue_full','memory_full']

class Transport:
    def __init__(self,dut):self.raw=dut
    def __getattr__(self,name):return getattr(self.raw,'transition_busy' if name=='busy' else name)

class PipelineDriver(Driver):
    def __init__(self,dut,depths,seed=0):
        super().__init__(Transport(dut),depths[:3],seed)
        self.raw=dut;self.capacities=depths
        self.media=None
        self.stalls=True
        self.pending_snapshot=[]
    async def reset(self):
        self.set(run_valid=0,run_instruction_count=0,run_state_count=0,run_quantum=0, run_chunk_frames=0,run_job_budget=0,
            run_buffers=0,run_inputs=0,run_outputs=0,job_enable=1,drain_ready=0,
            cfg_valid=0,cfg_state=0,cfg_write=0,cfg_address=0,cfg_write_data=0,
            audio_cfg_valid=0,audio_cfg_space=0,audio_cfg_write=0,audio_cfg_address=0,audio_cfg_write_data=0,
            )
        self.set(arena_base=0,store_cmd_valid=0,store_cmd_opcode=0,store_cmd_view=0,store_cmd_start=0,
            store_cmd_length=0,store_cmd_count=0,store_write_valid=0,store_write_data=0,store_read_ready=0,
            store_cfg_valid=0,store_cfg_write=0,store_cfg_space=0,store_cfg_address=0,store_cfg_write_data=0,
            memory_ready=0,memory_response_valid=0,memory_response_error=0,memory_read_data=0)
        await super().reset()
    async def configure(self,address,value=None,*,program=False):
        await FallingEdge(self.raw.clk)
        assert int(self.raw.cfg_ready.value)
        self.set(cfg_valid=1,cfg_state=int(not program),cfg_write=int(value is not None),cfg_address=address,cfg_write_data=value or 0)
        await self.edge();self.set(cfg_valid=0)
        assert int(self.raw.cfg_response_valid.value) and not int(self.raw.cfg_response_error.value)
        result=int(self.raw.cfg_read_data.value)
        await self.edge()
        return result
    async def sample(self,space,address,value=None):
        await FallingEdge(self.raw.clk)
        assert int(self.raw.audio_cfg_ready.value)
        self.set(audio_cfg_valid=1,audio_cfg_space=space,audio_cfg_write=int(value is not None),audio_cfg_address=address,audio_cfg_write_data=(value or 0)&0xffffffff)
        await self.edge();self.set(audio_cfg_valid=0)
        assert int(self.raw.audio_cfg_response_valid.value) and not int(self.raw.audio_cfg_response_error.value)
        value=int(self.raw.audio_cfg_read_data.value);await self.edge()
        return sp.signed32(value)
    async def initialize(self,case,image):
        assert case['queue_capacity']==self.capacities[5],'case and physical FIFO capacities differ'
        assert len(case['program']['instructions'])<=self.capacities[3]
        assert len(case['initial_state']['memory'])<=self.capacities[4]
        assert case['buffers']<=self.capacities[7] and case['output_ports']<=self.capacities[9]
        await self.load(case,image)
        self.case=case
        self.pending_snapshot=[]
        for i,instruction in enumerate(case['program']['instructions']):
            raw=sp.encode_instruction(instruction)
            await self.configure(i,raw,program=True)
            assert await self.configure(i,program=True)==raw
        for i,value in enumerate(case['initial_state']['memory']):await self.configure(i,sp.encode_value(value))
        if self.media is not None:
            if case['media'] is None:
                _,slots,cf=self.media.views,self.media.slots,self.media.chunk_frames
                initial=dict(chunk_frames=cf,chunk_limit=slots,samples=[],free_chunks=[],views=[])
            else: initial=case['media']
            await self.media.load(initial)
        else:
            assert case['media'] is None,'media-bearing cases require a sample-memory slave'
    async def step(self, step):
        control=step['transition'];frame=(await self.status())['frame']
        collected=[]
        if control['collect_completed']:
            for request in await self.requests():
                if request and request['outcome']:collected.append(await self.collect(request['id']))
        for request in control['collect']:collected.append(await self.collect(request))
        cancelled=[]
        for request in control['cancel']:
            error,value=await self.command(tp.CANCEL,request['slot'],request['generation'])
            cancelled.append(tp.response(error,bool(value)))
        submitted=[]
        for command in control['commands']:
            error,value=await self.command(tp.SUBMIT,data=tp.encode_command(command))
            submitted.append(tp.response(error,tp.decode_identity(value)))
        for write in step['writes']:
            if not int(self.raw.faulted.value): await self.configure(write['address'],sp.encode_value(write['value']))
        q=self.case['program']['quantum'];inputs=step['work']['value']['inputs']
        assert step['work']['kind']=='audio'
        for port,samples in enumerate(inputs):
            for i,value in enumerate(samples): await self.sample(0,port*self.capacities[6]+i,value)
        emitted=[];remaining=list(self.pending_snapshot)
        if not int(self.raw.faulted.value):
            if self.media is not None:self.set(arena_base=self.media.base)
            await FallingEdge(self.raw.clk)
            assert int(self.raw.run_ready.value)
            self.set(run_valid=1,run_instruction_count=len(self.case['program']['instructions']),
                run_state_count=len(self.case['initial_state']['memory']),run_quantum=q,
                run_chunk_frames=self.case['program']['chunk_frames'],run_job_budget=self.case['program']['jobs_per_quantum'],
                run_buffers=self.case['buffers'],run_inputs=len(inputs),run_outputs=self.case['output_ports'])
            await self.edge();self.set(run_valid=0)
            finished=bool(int(self.raw.done.value))
            for _ in range(1000000):
                if finished:break
                await FallingEdge(self.raw.clk)
                self.set(job_enable=int(not self.stalls or self.random.randrange(5)!=0))
                await Timer(1,units='ns')
                if int(self.raw.monitor_valid.value) and int(self.raw.monitor_ready.value):
                    emitted.append(sp.decode_job(int(self.raw.monitor_data.value)))
                if int(self.raw.state_busy.value) or int(self.raw.jobs_busy.value):
                    assert not any(int(getattr(self.raw,n).value) for n in ('cfg_ready','audio_cfg_ready','store_cmd_ready','store_cfg_ready'))
                await self.edge();finished=bool(int(self.raw.done.value))
            assert finished,'coordinator timed out'
            self.set(job_enable=1)
            if int(self.raw.error_stage.value) in (2,3):
                remaining=[];self.set(drain_ready=1)
                for _ in range(2000):
                    await FallingEdge(self.raw.clk)
                    if int(self.raw.monitor_valid.value) and int(self.raw.monitor_ready.value):
                        job=sp.decode_job(int(self.raw.monitor_data.value));emitted.append(job);remaining.append(job)
                    await self.edge()
                    if int(self.raw.queued_jobs.value)==0:break
                self.set(drain_ready=0);self.pending_snapshot=remaining
        stage=int(self.raw.error_stage.value);code=int(self.raw.error_code.value)
        error=None
        if stage==1:error=dict(stage='transition',error=tp.ERRORS[code])
        elif stage==2:error=dict(stage='state',error=sp.decode_status(code)['value']['error'])
        elif stage==3:error=dict(stage='jobs',error={1:'invalid_buffer',2:'invalid_shape',3:'invalid_state',4:'invalid_state',5:'invalid_view',6:'memory_full',7:'invalid_state',8:'memory_fault'}[code])
        transition=dict(frame=frame,collected=collected,cancelled=cancelled,submitted=submitted,
            processed=tp.response(code if stage==1 else 0,None),states=await self.states(),
            requests=await self.requests(),events=await self.events(),next_frame=(await self.status())['frame'])
        state={'memory':[sp.decode_value(await self.configure(i)) for i in range(len(self.case['initial_state']['memory']))]}
        ready=int(self.raw.buffer_ready.value)
        buffers=[[await self.sample(2,i*self.capacities[6]+f) for f in range(q)] if ready&(1<<i) else None for i in range(self.case['buffers'])]
        outputs=[[await self.sample(1,i*self.capacities[6]+f) for f in range(q)] for i in range(self.case['output_ports'])]
        audio=dict(jobs=emitted,remaining_jobs=remaining,state=state,buffers=buffers,outputs=outputs,
            stage='jobs',status={'kind':'completed'} if error is None else {'kind':'failed','value':{'error':error['error']}},media=None)
        if self.case['media'] is not None and not (stage==3 and code==8): audio['media']=await self.media.snapshot()
        return dict(transition=transition,jobs=emitted,state=state,error=error,audio=audio)
