"""No Rust state/jobs are supplied to RTL: only the compiled image and common inputs."""
import json
import os
from pathlib import Path
import cocotb
from cocotb.triggers import FallingEdge, Timer
import rt_backend_testbench as wire
from pipeline_driver import PipelineDriver
from loop_memory import LoopMemory
from media_results import logical_media
import codec as tp
from pipeline_driver import sp
from execution_timing import ExecutionTiming
from options import metadata
from block_images import reblock

OUTPUT=Path(os.environ['UNIFY_OUTPUT'])
DEPTHS=tuple(map(int,os.environ['LP_DEPTHS'].split(',')))
MEDIA=tuple(map(int,os.environ['LP_MEDIA_CAPACITIES'].split(',')))
BLOCK_CHUNKS=int(os.environ.get('AXI_BLOCK_CHUNKS','1'))

def physical_media(image):
    return {**reblock(image['media'],BLOCK_CHUNKS,MEDIA[1]),'chunk_limit':MEDIA[1]}

def identity(value):return value['slot'],value['generation']

def completion(value,names,submitted,q):
    name=names[value['sequence']]
    requested,at=submitted[name]
    # Delivery is the first eligible quantum in this fixed-quantum profile.
    delivered=max(at,((requested+q-1)//q)*q)
    if value['outcome']['kind']=='cancelled' and value['outcome']['frame']<delivered:delivered=None
    outcome=value['outcome']['kind']
    if outcome=='failed':outcome+=':'+value['outcome']['reason']
    return dict(name=name,requested=requested,delivered=delivered,terminal=value['outcome']['frame'],outcome=outcome)

async def collect(driver,names,submitted,q):
    result=[]
    for request in await driver.requests():
        if request and request['outcome']:
            row=await driver.collect(request['id'])
            assert row['result']['kind']=='accepted'
            result.append(completion(row['result']['value'],names,submitted,q))
    return result

@cocotb.test()
async def production_scenarios(dut):
    timings=[]
    for index in range(int(os.environ['UNIFY_CASES'])):
        (OUTPUT/f'{index}.partial.actual.json').unlink(missing_ok=True)
        scenario=wire.load(OUTPUT/f'{index}.scenario.json')
        image=wire.load(OUTPUT/f'{index}.image.json','image')
        expected=wire.load(OUTPUT/f'{index}.expected.json','result')
        physical=physical_media(image)
        (OUTPUT/f'{index}.physical-media.json').write_text(json.dumps(physical,indent=2)+'\n')
        case=dict(name=scenario['name'],plan=dict(description=image['transition']['description'],limits=image['transition']['limits']),
            program=image['program'],initial_state=image['initial_state'],buffers=image['buffers'],output_ports=image['output_ports'],queue_capacity=DEPTHS[5],media=physical)
        driver=PipelineDriver(dut,DEPTHS,20261112);driver.stalls=os.environ['UNIFY_STALLS']=='1'
        memory=LoopMemory(dut,MEDIA,20261112);memory.stalls=driver.stalls;driver.media=memory
        probe=ExecutionTiming(dut,memory);memory.before_clock=probe.observe
        timing=dict(name=scenario['name'],quanta=[])
        actual=dict(format_version=1,name=scenario['name'],numeric='q824',quanta=[],media=[])
        names={};submitted={};sequence=0
        scheduled_gains=[];held_gains=[];request_ids={};snapshots=[];held_admissions=[];ingress=[]
        try:
            await driver.initialize(case,image['transition'])
            for number,step in enumerate(scenario['steps']):
                frame=number*scenario['quantum']
                outcomes=await collect(driver,names,submitted,scenario['quantum']) if step['collect_completed'] else []
                if step['collect_completed']:
                    outcomes.extend(held_gains);held_gains=[]
                outcomes.extend(held_admissions);held_admissions=[]
                commands=[]
                for action in step['actions']:
                    sequence+=1
                    names[sequence]=action['name'];submitted[action['name']]=(action.get('at',frame),frame)
                    if action['kind']=='transition':
                        command=dict(target=action['target'],sequence=sequence,at=action['at'],timing=action['timing'],operation=action['operation'])
                        error,value=await driver.command(tp.SUBMIT,data=tp.encode_command(command))
                        ingress.append(dict(name=action['name'],port='transition_submit',frame=frame,error=tp.ERRORS[error]))
                        if error==2:
                            held_admissions.append(dict(name=action['name'],requested=action['at'],delivered=None,terminal=frame,outcome='rejected:full'))
                        elif error:
                            held_gains.append(dict(name=action['name'],requested=action['at'],delivered=frame,terminal=frame,outcome='failed:'+tp.ERRORS[error]))
                        else:request_ids[action['name']]=tp.decode_identity(value)
                    elif action['kind']=='gain':
                        scheduled_gains.append(action)
                    elif action['kind']=='cancel':
                        request=request_ids[action['request']]
                        error,_=await driver.command(tp.CANCEL,request['slot'],request['generation'])
                        held_gains.append(dict(name=action['name'],requested=frame,delivered=frame,terminal=frame,outcome='failed:'+tp.ERRORS[error] if error else 'applied'))
                    elif action['kind']=='export':
                        binding=next(v for v in image['loop_views'] if v['id']==action['target'])
                        retained=[]
                        for view in binding['views']:
                            descriptor=await memory.configure(1,view)
                            frames=(descriptor>>64)&((1<<64)-1)
                            response=await memory.command(dict(kind='slice',source=view,start=0,frames=frames),stalls=driver.stalls,drain=False)
                            assert response['status']['kind']=='completed',response['status']
                            retained.append(response['value']['value'])
                        snapshots.append(dict(name=action['name'],views=retained))
                        held_gains.append(dict(name=action['name'],requested=frame,delivered=frame,terminal=frame,outcome='applied'))
                    else:raise AssertionError('action adapter not implemented: '+action['kind'])
                for action in scheduled_gains[:]:
                    if action['at']>frame:continue
                    binding=next(b for b in image['gains'] if b['id']==action['node'])
                    for address in binding['ramps']:
                        old=sp.decode_value(await driver.configure(address))['value']
                        duration=old['duration']
                        delta=(old['target']-old['start'])*min(old['elapsed'],duration)
                        offset=(abs(delta)//duration)*(-1 if delta<0 else 1) if duration else 0
                        start=old['start']+offset if duration else old['target']
                        value=dict(kind='ramp',value=dict(start=start,target=action['target'],duration=action['duration'],elapsed=0))
                        await driver.configure(address,sp.encode_value(value))
                    held_gains.append(dict(name=action['name'],requested=action['at'],delivered=frame,terminal=frame,outcome='applied'))
                    scheduled_gains.remove(action)
                probe.begin()
                row=await driver.step(dict(transition=dict(collect_completed=False,collect=[],cancel=[],commands=commands),writes=[],work=dict(kind='audio',value=dict(inputs=step['inputs']))))
                assert row['error'] is None,row['error']
                timing['quanta'].append(probe.finish())
                states=[]
                for logical in scenario['loops']:
                    value=next(s for s in row['transition']['states'] if s['id']==logical['id'])
                    states.append({k:v for k,v in value.items() if k!='revision_watermark'})
                pending=[dict(name=names[r['command']['sequence']],requested=r['command']['at'],effective=r['effective'],remaining_ticks=r['remaining_ticks'])
                         for r in row['transition']['requests'] if r and r['outcome'] is None and r['effective']<=frame]
                pending.sort(key=lambda r:r['name'])
                outcomes.sort(key=lambda r:r['name'])
                actual['quanta'].append(dict(frame=frame,next_frame=row['transition']['next_frame'],outputs=dict(encoding='q824',channels=row['audio']['outputs']),states=states,outcomes=outcomes,events=row['transition']['events'],pending=pending))
            actual['quanta'][-1]['outcomes'].extend(await collect(driver,names,submitted,scenario['quantum']))
            actual['quanta'][-1]['outcomes'].extend(held_gains)
            actual['quanta'][-1]['outcomes'].extend(held_admissions)
            actual['quanta'][-1]['outcomes'].sort(key=lambda r:r['name'])
            actual['media']=logical_media(row['audio']['media'],image['loop_views'],states)
            for retained in snapshots:
                channels=[]
                for view in retained['views']:
                    frames=(await memory.configure(1,view)>>64)&((1<<64)-1)
                    response=await memory.command(dict(kind='read',view=view,position=0,length=frames,frames=frames),stalls=driver.stalls,drain=False)
                    assert response['status']['kind']=='completed',response['status']
                    channels.append(response['value']['value'])
                actual['media'].append(dict(name=retained['name'],frames=len(channels[0]),samples=dict(encoding='q824',channels=channels)))
            actual['media'].sort(key=lambda m:m['name'])
            (OUTPUT/f'{index}.ingress.json').write_text(json.dumps(ingress,indent=2)+'\n')
            wire.dump(actual,OUTPUT/f'{index}.actual.json','result')
            wire.assert_exact(actual,expected)
            timings.append(timing)
        except BaseException:
            wire.dump(actual,OUTPUT/f'{index}.partial.actual.json','result')
            raise
        finally:memory.close()
    (OUTPUT/'timing.json').write_text(json.dumps(dict(memory=metadata(),scenarios=timings),indent=2)+'\n')

@cocotb.test()
async def early_iterations_preempt_cleanup_without_waiting_for_its_allowance(dut):
    image=wire.load(OUTPUT/'0.image.json','image')
    program=dict(image['program'],instructions=[],jobs_per_quantum=0)
    case=dict(name='cleanup-handoff',plan=dict(description=image['transition']['description'],limits=image['transition']['limits']),
        program=program,initial_state=dict(memory=[]),buffers=0,output_ports=0,queue_capacity=DEPTHS[5],
        media=dict(chunk_frames=MEDIA[2],block_chunks=BLOCK_CHUNKS,chunk_limit=MEDIA[1],samples=[],free_chunks=[],views=[]))
    results=[]
    for settle in (False,True):
        driver=PipelineDriver(dut,DEPTHS,20261113);memory=LoopMemory(dut,MEDIA,20261113);driver.media=memory
        core=dut.core.core;gc=core.audio.media.reclaimer
        async def admission(host=False):
            await FallingEdge(dut.clk)
            if host:driver.set(command_valid=1,command_opcode=16,command_index=0,command_data=0,response_ready=1)
            else:driver.set(run_valid=1,run_instruction_count=0,run_state_count=0,run_quantum=program['quantum'],
                run_chunk_frames=program['chunk_frames'],run_job_budget=0,run_buffers=0,run_inputs=0,run_outputs=0)
            for wait in range(10000):
                await Timer(1,units='ns')
                if not int(gc.lock_held.value):
                    assert not (int(gc.lock_request.value) and int(gc.lock_grant.value)),'cleanup reacquired a lock ahead of a waiting iteration'
                    assert not (int(gc.request_valid.value) and int(gc.request_ready.value)),'cleanup admitted metadata ahead of a waiting iteration'
                    assert not (int(gc.retire_valid.value) and int(gc.retire_ready.value)),'cleanup admitted retirement ahead of a waiting iteration'
                ready=int((dut.command_ready if host else dut.run_ready).value)
                remaining=int(gc.cycles_left.value)
                await driver.edge()
                if ready:break
                await FallingEdge(dut.clk)
            else:raise AssertionError('cleanup starved iteration admission')
            driver.set(**({'command_valid':0} if host else {'run_valid':0}))
            assert remaining>int(os.environ['AXI_RECLAIM_GUARD']),'iteration waited for the cleanup allowance to expire'
            return wait+1
        try:
            await driver.initialize(case,image['transition'])
            memory.set(maintenance_enable=0);blocks=min(25,memory.blocks);bf=memory.chunk_frames*memory.block_chunks
            for block in range(blocks):
                await memory.configure(2,memory.table_address(0,block),block)
                await memory.configure(3,block,1)
            await memory.configure(1,0,1|((blocks*bf)<<64)|(blocks<<128))
            await memory.configure(0,0,1);await memory.configure(0,1,blocks)
            result=await memory.command(dict(kind='clear',view=0),stalls=False,drain=False,observe=False)
            assert result['status']['kind']=='completed'
            await memory.grant_maintenance(cycles=100000)
            memory.set(maintenance_enable=1)
            for _ in range(10000):
                await memory.tick()
                if int(gc.phase.value)==(4 if settle else 1):break
            else:raise AssertionError('cleanup did not reach the handoff boundary')
            waits=[await admission(settle)]
            if settle:
                for _ in range(10000):
                    await driver.edge()
                    if int(core.phase.value)==9:break
                else:raise AssertionError('host settlement did not complete')
                driver.set(response_ready=0)
                remaining=int(gc.cycles_left.value)
                waits.append(await admission())
                assert int(gc.cycles_left.value)<=remaining,'audio admission replenished an already settled iteration'
            for run in range(2):
                elapsed=0
                while not int(dut.done.value) and elapsed<10000:
                    await driver.edge();elapsed+=1
                assert int(dut.done.value) and not int(dut.error_code.value)
                assert elapsed<int(os.environ['AXI_RECLAIM_CYCLES']),'empty processing unexpectedly waited for the cleanup budget'
                if run==0:waits.append(await admission())
            results.append(dict(host_settlement=settle,admission_cycles=waits,final_processing_cycles=elapsed))
        finally:memory.close()
    (OUTPUT/'cleanup-handoff.json').write_text(json.dumps(results,indent=2)+'\n')

if os.environ.get('UNIFY_FAULTS')=='1':
    @cocotb.test()
    async def fatal_processing_faults_silence_hold_and_reset(dut):
        from copy import deepcopy
        from rt_backend_testbench.cases import base,step,action,loaded
        root=Path(__file__).resolve().parents[3]
        executable=root/'target/debug/rt-backend-testbench'
        def physical(image):
            return dict(name=image['name'],plan=dict(description=image['transition']['description'],limits=image['transition']['limits']),
                program=image['program'],initial_state=image['initial_state'],buffers=image['buffers'],output_ports=image['output_ports'],queue_capacity=DEPTHS[5],
                media=physical_media(image))
        def row(inputs,commands=()):
            return dict(transition=dict(collect_completed=True,collect=[],cancel=[],commands=list(commands)),writes=[],work=dict(kind='audio',value=dict(inputs=inputs)))
        def record_command():return dict(target=dict(slot=0,generation=1),sequence=1,at=0,timing=dict(kind='at'),operation=dict(kind='record'))
        scenario=base('fault-arena-exhaustion',cf=MEDIA[2])
        scenario['steps']=[step(ordinal=i) for i in range(3)]
        image=wire.run(scenario,executable=executable,command='compile')
        # Make a nonzero output visible before the failing capture reaches JP.
        image['program']['instructions'].insert(1,dict(kind='process_output',source=0,port=0))
        image['program']['jobs_per_quantum']+=1
        driver=PipelineDriver(dut,DEPTHS,20261112);memory=LoopMemory(dut,MEDIA,20261112);driver.media=memory
        try:
            capacity=max(BLOCK_CHUNKS,scenario['quantum']//MEDIA[2])
            filled_quanta=capacity*MEDIA[2]//scenario['quantum']
            failed_frame=filled_quanta*scenario['quantum']
            case=physical(image);case['media']['chunk_limit']=capacity
            await driver.initialize(case,image['transition'])
            assert (await driver.step(row(scenario['steps'][0]['inputs'],[record_command()])))['error'] is None
            for _ in range(filled_quanta-1):
                assert (await driver.step(row(scenario['steps'][1]['inputs'])))['error'] is None
            failed=await driver.step(row(scenario['steps'][1]['inputs']))
            assert failed['error']==dict(stage='jobs',error='memory_full')
            assert failed['audio']['outputs']==[[0]*32]
            assert failed['transition']['next_frame']==failed_frame
            assert int(dut.faulted.value) and not int(dut.run_ready.value)
            held=await driver.step(row(scenario['steps'][2]['inputs']))
            assert held['error']==failed['error'] and held['transition']['next_frame']==failed_frame
            assert held['audio']['outputs']==[[0]*32]
            case=physical(image)
            await driver.initialize(case,image['transition'])
            assert (await driver.step(row(scenario['steps'][0]['inputs'],[record_command()])))['error'] is None
        finally:memory.close()

        scenario=loaded(base('fault-late-memory-response',cf=MEDIA[2]),64)
        scenario['steps']=[step()]
        image=wire.run(scenario,executable=executable,command='compile')
        driver=PipelineDriver(dut,DEPTHS,20261112);memory=LoopMemory(dut,MEDIA,20261112);driver.media=memory
        command=dict(target=dict(slot=0,generation=1),sequence=1,at=0,timing=dict(kind='at'),operation=dict(kind='play',position=0,retrigger=False))
        try:
            await driver.initialize(physical(image),image['transition']);memory.fail_next=True
            failed=await driver.step(row(scenario['steps'][0]['inputs'],[command]))
            assert failed['error']==dict(stage='jobs',error='memory_fault')
            assert failed['audio']['outputs']==[[0]*32] and failed['transition']['next_frame']==0
            held=await driver.step(row(scenario['steps'][0]['inputs']))
            assert held['error']==failed['error'] and held['transition']['next_frame']==0
            await driver.initialize(physical(image),image['transition'])
            assert (await driver.step(row(scenario['steps'][0]['inputs'],[command])))['error'] is None
        finally:memory.close()
        (OUTPUT/'fault-containment.json').write_text(json.dumps(dict(arena_exhaustion=True,partial_output_silenced=True,
            late_memory_error=True,sticky_silence=True,reset_reload=True),indent=2)+'\n')

        scenario=base('fault-state-generation',cf=MEDIA[2]);scenario['steps']=[step()]
        image=wire.run(scenario,executable=executable,command='compile')
        stale=deepcopy(image)
        next(i for i in stale['program']['instructions'] if i['kind']=='process_logical_playback')['loop_id']['generation']+=1
        driver=PipelineDriver(dut,DEPTHS,20261112);memory=LoopMemory(dut,MEDIA,20261112);driver.media=memory
        try:
            await driver.initialize(physical(stale),stale['transition'])
            failed=await driver.step(row(scenario['steps'][0]['inputs']))
            assert failed['error']==dict(stage='state',error='invalid_state')
            assert failed['audio']['outputs']==[[0]*32] and failed['transition']['next_frame']==0
            await driver.initialize(physical(image),image['transition'])
            assert (await driver.step(row(scenario['steps'][0]['inputs'])))['error'] is None
        finally:memory.close()

        scenario=base('fault-descriptor-exhaustion',cf=MEDIA[2]);scenario['steps']=[step()]
        image=wire.run(scenario,executable=executable,command='compile')
        capture=next(i for i in image['program']['instructions'] if i['kind']=='process_logical_capture')
        image['initial_state']['memory'][capture['view']]['value']=MEDIA[0]
        image['media']['views']=[dict(chunks=[],frames=0) for _ in range(MEDIA[0])]
        driver=PipelineDriver(dut,DEPTHS,20261112);memory=LoopMemory(dut,MEDIA,20261112);driver.media=memory
        try:
            await driver.initialize(physical(image),image['transition'])
            failed=await driver.step(row(scenario['steps'][0]['inputs'],[record_command()]))
            assert failed['error']==dict(stage='jobs',error='memory_full')
            assert failed['audio']['outputs']==[[0]*32] and failed['transition']['next_frame']==0
            image['initial_state']['memory'][capture['view']]['value']=0
            await driver.initialize(physical(image),image['transition'])
            assert (await driver.step(row(scenario['steps'][0]['inputs'],[record_command()])))['error'] is None
            # BeginCapture also creates its stable view when no descriptor was preloaded.
            image['media']['views']=[]
            await driver.initialize(physical(image),image['transition'])
            assert (await driver.step(row(scenario['steps'][0]['inputs'],[record_command()])))['error'] is None
        finally:memory.close()

        scenario=loaded(base('fault-cold-metadata-and-reset',cf=MEDIA[2]),64);scenario['steps']=[step()]
        image=wire.run(scenario,executable=executable,command='compile')
        driver=PipelineDriver(dut,DEPTHS,20261112);memory=LoopMemory(dut,MEDIA,20261112);driver.media=memory
        command=dict(target=dict(slot=0,generation=1),sequence=1,at=0,timing=dict(kind='at'),operation=dict(kind='play',position=0,retrigger=False))
        try:
            await driver.initialize(physical(image),image['transition'])
            await memory.fence();await memory.configure(5,0,1);memory.fail_next=True
            failed=await driver.step(row(scenario['steps'][0]['inputs'],[command]))
            assert failed['error']==dict(stage='jobs',error='memory_fault') and failed['transition']['next_frame']==0
            await driver.initialize(physical(image),image['transition'])
            memory.pause_responses=True
            running=cocotb.start_soon(driver.step(row(scenario['steps'][0]['inputs'],[command])))
            for _ in range(10000):
                await driver.edge()
                if memory.pending is not None:break
            else:raise AssertionError('no outstanding memory request')
            running.kill();await driver.reset()
            assert memory.pending is None
            memory.pause_responses=False
            await driver.initialize(physical(image),image['transition'])
            assert (await driver.step(row(scenario['steps'][0]['inputs'],[command])))['error'] is None
        finally:memory.close()

        scenario=base('fault-posted-writeback',cf=MEDIA[2]);scenario['steps']=[step()]
        image=wire.run(scenario,executable=executable,command='compile')
        driver=PipelineDriver(dut,DEPTHS,20261112);memory=LoopMemory(dut,MEDIA,20261112);driver.media=memory
        try:
            case=physical(image);case['media']=None
            free_slots=max(BLOCK_CHUNKS,32//MEDIA[2])
            memory.initial_override=dict(block_chunks=BLOCK_CHUNKS,chunk_frames=MEDIA[2],samples=[0]*(free_slots*MEDIA[2]),
                free_chunks=list(range(free_slots)),views=[dict(chunks=[],frames=0)])
            await driver.initialize(case,image['transition'])
            memory.pause_responses=True;memory.fail_next=True
            first=await driver.step(row(scenario['steps'][0]['inputs'],[record_command()]))
            assert first['error'] is None and first['transition']['next_frame']==32
            memory.pause_responses=False
            for _ in range(10000):
                await driver.edge()
                if int(dut.faulted.value):break
            else:raise AssertionError('posted writeback fault was not latched')
            failed=await driver.step(row(scenario['steps'][0]['inputs']))
            assert failed['error']==dict(stage='jobs',error='memory_fault')
            assert failed['audio']['outputs']==[[0]*32] and failed['transition']['next_frame']==32
        finally:memory.close()
        result=json.loads((OUTPUT/'fault-containment.json').read_text())
        result.update(state_generation=True,descriptor_exhaustion=True,automatic_view_creation=True,
            cold_metadata_error=True,reset_outstanding=True,posted_writeback=True)
        (OUTPUT/'fault-containment.json').write_text(json.dumps(result,indent=2)+'\n')
