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
    async def reset(self):
        self.set(run_valid=0,run_instruction_count=0,run_state_count=0,run_quantum=0,run_job_budget=0,
            run_buffers=0,run_inputs=0,run_outputs=0,job_enable=1,drain_ready=0,
            cfg_valid=0,cfg_state=0,cfg_write=0,cfg_address=0,cfg_write_data=0,
            audio_cfg_valid=0,audio_cfg_space=0,audio_cfg_write=0,audio_cfg_address=0,audio_cfg_write_data=0,
            run_external_jobs=0,external_job_ready=0,external_done_valid=0,external_error=0,
            preflight_ready=0,preflight_retry=0)
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
        for i,instruction in enumerate(case['program']['instructions']):
            raw=sp.encode_instruction(instruction)
            await self.configure(i,raw,program=True)
            assert await self.configure(i,program=True)==raw
        for i,value in enumerate(case['initial_state']['memory']):await self.configure(i,sp.encode_value(value))
    async def observations(self,values):
        await self.ok(tp.OBS_CLEAR)
        for value in values:await self.ok(tp.OBS_ADD,data=tp.encode_observation(value))
    async def preflight(self):
        if not int(self.raw.preflight_valid.value):
            await with_timeout(RisingEdge(self.raw.preflight_valid),10_000_000,'ns');await Timer(1,units='ns')
        assert int(self.raw.debug_frame.value)==self.frame
        error=int(self.raw.preflight_error.value);token=int(self.raw.boundary_token.value)
        decisions=None if error else [tp.decode_decision(await self.ok(tp.READ_DECISION,i['slot'],token),i['slot']) for i in self.ids]
        return tp.response(error,decisions)
    async def accept(self,retry=False):
        await FallingEdge(self.raw.clk);self.set(preflight_ready=1,preflight_retry=int(retry))
        await self.edge();self.set(preflight_ready=0,preflight_retry=0)
    async def step(self,step):
        control=step['transition'];self.frame=(await self.status())['frame']
        collected=[]
        if control['collect_completed']:
            for request in await self.requests():
                if request and request['outcome']:collected.append(await self.collect(request['id']))
        for request in control['collect']:collected.append(await self.collect(request))
        cancelled=[]
        for request in control['cancel']:
            error,value=await self.command(tp.CANCEL,request['slot'],request['generation']);cancelled.append(tp.response(error,bool(value)))
        submitted=[]
        for command in control['commands']:
            error,value=await self.command(tp.SUBMIT,data=tp.encode_command(command));submitted.append(tp.response(error,tp.decode_identity(value)))
        for write in step['writes']:await self.configure(write['address'],sp.encode_value(write['value']))
        external=step['work']['kind']=='external';q=self.case['program']['quantum']
        inputs=[] if external else step['work']['value']['inputs']
        assert len(inputs)<=self.capacities[8] and all(len(samples)==q for samples in inputs)
        if not external:
            assert q<=self.capacities[6] and self.case['media'] is None,'BRAM profile has bounded quanta and no sample media'
        for port,samples in enumerate(inputs):
            for i,value in enumerate(samples):await self.sample(0,port*self.capacities[6]+i,value)
        before=(await self.states(),await self.requests(),await self.events())
        await self.observations(control['previews'][0])
        await FallingEdge(self.raw.clk)
        assert int(self.raw.run_ready.value)
        self.set(run_valid=1,run_instruction_count=len(self.case['program']['instructions']),run_state_count=len(self.case['initial_state']['memory']),
            run_quantum=q,run_job_budget=self.case['program']['jobs_per_quantum'],run_external_jobs=int(external),
            run_buffers=self.case['buffers'],run_inputs=len(inputs),run_outputs=self.case['output_ports'])
        await self.edge();self.set(run_valid=0,run_instruction_count=0xffffffff,run_state_count=0xffffffff,
            run_quantum=0,run_job_budget=0xffffffff,run_external_jobs=int(not external),run_buffers=0,run_inputs=0,run_outputs=0)
        previews=[]
        for i,observations in enumerate(control['previews']):
            if i:
                await self.observations(observations);await self.accept(retry=True)
            previews.append(await self.preflight())
            assert (await self.states(),await self.requests(),await self.events())==before,'preview published a candidate'
        committed=previews[-1]['kind']=='accepted'
        await self.accept()
        emitted=[];shared_reads=0;completion_delay=0;finished=bool(int(self.raw.done.value));held=None
        for _ in range(100000):
            if finished:break
            await FallingEdge(self.raw.clk)
            done_ready=int(self.raw.external_done_ready.value)
            if done_ready:completion_delay+=1
            status=step['work']['value'] if external else {'kind':'completed'}
            injected=0 if status['kind']=='completed' else ERRORS.index(status['value']['error'])+1
            self.set(external_job_ready=int(self.random.randrange(4)!=0),job_enable=int(self.random.randrange(5)!=0),
                external_done_valid=int(external and completion_delay>4),external_error=injected)
            await Timer(1,units='ns')
            if held is not None:
                assert int(self.raw.external_job_valid.value) and int(self.raw.monitor_data.value)==held,'external descriptor changed while stalled'
            held=int(self.raw.monitor_data.value) if int(self.raw.external_job_valid.value) and not int(self.raw.external_job_ready.value) else None
            if int(self.raw.monitor_valid.value) and int(self.raw.monitor_ready.value):emitted.append(sp.decode_job(int(self.raw.monitor_data.value)))
            shared_reads+=int(self.raw.debug_shared_read.value)
            if external and not (completion_delay>4):assert int(self.raw.debug_frame.value)==self.frame,'clock advanced before job completion'
            await self.edge()
            if int(self.raw.done.value):finished=True;break
        assert finished,'coordinator timed out'
        self.set(external_done_valid=0,external_job_ready=0,job_enable=1)
        error=None
        stage=int(self.raw.error_stage.value);code=int(self.raw.error_code.value)
        if stage==2:error={'stage':'state','error':sp.decode_status(code)['value']['error']}
        if stage==3:error={'stage':'jobs','error':ERRORS[code-1] if external else {1:'invalid_buffer',2:'invalid_shape'}[code]}
        if error:assert not int(self.raw.run_ready.value),'failed quantum admitted a restart'
        if stage==1:assert not committed
        remaining=[]
        if stage==2 or (stage==3 and not external):
            self.set(drain_ready=1)
            for _ in range(2000):
                await FallingEdge(self.raw.clk)
                if int(self.raw.monitor_valid.value) and int(self.raw.monitor_ready.value):
                    job=sp.decode_job(int(self.raw.monitor_data.value));emitted.append(job);remaining.append(job)
                await self.edge()
                if int(self.raw.queued_jobs.value)==0:break
            self.set(drain_ready=0)
        after_finish=await self.states()
        states=[decision['state'] for decision in previews[-1]['value']] if committed else before[0]
        transition=dict(frame=self.frame,collected=collected,cancelled=cancelled,submitted=submitted,previews=previews,committed=committed,
            states=states,requests=await self.requests(),events=await self.events(),after_finish=after_finish,next_frame=(await self.status())['frame'])
        state={'memory':[sp.decode_value(await self.configure(i)) for i in range(len(self.case['initial_state']['memory']))]}
        audio=None
        if not external and committed and stage!=2:
            ready=int(self.raw.buffer_ready.value)
            buffers=[[await self.sample(2,i*self.capacities[6]+f) for f in range(q)] if ready&(1<<i) else None for i in range(self.case['buffers'])]
            outputs=[[await self.sample(1,i*self.capacities[6]+f) for f in range(q)] for i in range(self.case['output_ports'])]
            audio=dict(jobs=emitted,remaining_jobs=remaining,state=state,buffers=buffers,outputs=outputs,stage='jobs',status={'kind':'completed'} if not error else {'kind':'failed','value':{'error':error['error']}})
        if committed and stage!=2:
            assert shared_reads==sum(i['kind'] in ('process_logical_playback','process_logical_capture') for i in self.case['program']['instructions'])
        return dict(transition=transition,jobs=emitted,state=state,error=error,audio=audio)
