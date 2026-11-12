"""No Rust state/jobs are supplied to RTL: only the compiled image and common inputs."""
import json
import os
from pathlib import Path
import cocotb
import rt_backend_testbench as wire
from pipeline_driver import PipelineDriver
from loop_memory import LoopMemory
import codec as tp
from pipeline_driver import sp
from execution_timing import ExecutionTiming
from options import metadata

OUTPUT=Path(os.environ['UNIFY_OUTPUT'])
DEPTHS=tuple(map(int,os.environ['LP_DEPTHS'].split(',')))
MEDIA=tuple(map(int,os.environ['LP_MEDIA_CAPACITIES'].split(',')))

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

def logical_media(snapshot,views,states):
    result=[]
    for row in views:
        state=next(s for s in states if s['id']==row['id'])
        if state['mode']=='recording':continue
        channels=[]
        for view in row['views']:
            value=snapshot['views'][view]
            data=[]
            for chunk in value['chunks']:
                first=chunk*snapshot['chunk_frames'];data.extend(snapshot['samples'][first:first+snapshot['chunk_frames']])
            channels.append(data[:value['frames']])
        if channels and channels[0]:
            result.append(dict(name=f"loop:{row['id']['slot']}:{row['id']['generation']}",frames=len(channels[0]),samples=dict(encoding='q824',channels=channels)))
    return result

@cocotb.test()
async def production_scenarios(dut):
    timings=[]
    for index in range(int(os.environ['UNIFY_CASES'])):
        (OUTPUT/f'{index}.partial.actual.json').unlink(missing_ok=True)
        scenario=wire.load(OUTPUT/f'{index}.scenario.json')
        image=wire.load(OUTPUT/f'{index}.image.json','image')
        expected=wire.load(OUTPUT/f'{index}.expected.json','result')
        physical={**image['media'],'chunk_limit':MEDIA[1]}
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
                            response=await memory.command(dict(kind='slice',source=view,start=0,frames=frames),stalls=driver.stalls)
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
                    response=await memory.command(dict(kind='read',view=view,position=0,length=frames,frames=frames),stalls=driver.stalls)
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
                media={**image['media'],'chunk_limit':MEDIA[1]})
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
            case=physical(image);case['media']['chunk_limit']=scenario['quantum']//MEDIA[2]
            await driver.initialize(case,image['transition'])
            assert (await driver.step(row(scenario['steps'][0]['inputs'],[record_command()])))['error'] is None
            failed=await driver.step(row(scenario['steps'][1]['inputs']))
            assert failed['error']==dict(stage='jobs',error='memory_full')
            assert failed['audio']['outputs']==[[0]*32]
            assert failed['transition']['next_frame']==32
            assert int(dut.faulted.value) and not int(dut.run_ready.value)
            held=await driver.step(row(scenario['steps'][2]['inputs']))
            assert held['error']==failed['error'] and held['transition']['next_frame']==32
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
            memory.initial_override=dict(chunk_frames=MEDIA[2],samples=[0]*32,
                free_chunks=list(range(32//MEDIA[2])),views=[dict(chunks=[],frames=0)])
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
