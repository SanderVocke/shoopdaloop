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
    for index in range(int(os.environ['UNIFY_CASES'])):
        scenario=wire.load(OUTPUT/f'{index}.scenario.json')
        image=wire.load(OUTPUT/f'{index}.image.json','image')
        expected=wire.load(OUTPUT/f'{index}.expected.json','result')
        physical={**image['media'],'chunk_limit':MEDIA[1]}
        case=dict(name=scenario['name'],plan=dict(description=image['transition']['description'],limits=image['transition']['limits']),
            program=image['program'],initial_state=image['initial_state'],buffers=image['buffers'],output_ports=image['output_ports'],queue_capacity=DEPTHS[5],media=physical)
        driver=PipelineDriver(dut,DEPTHS,20261112);driver.stalls=os.environ['UNIFY_STALLS']=='1'
        memory=LoopMemory(dut,MEDIA,20261112);memory.stalls=driver.stalls;driver.media=memory
        actual=dict(format_version=1,name=scenario['name'],numeric='q824',quanta=[],media=[])
        names={};submitted={};sequence=0
        scheduled_gains=[];held_gains=[]
        try:
            await driver.initialize(case,image['transition'])
            for number,step in enumerate(scenario['steps']):
                frame=number*scenario['quantum']
                outcomes=await collect(driver,names,submitted,scenario['quantum']) if step['collect_completed'] else []
                if step['collect_completed']:
                    outcomes.extend(held_gains);held_gains=[]
                commands=[]
                for action in step['actions']:
                    sequence+=1
                    names[sequence]=action['name'];submitted[action['name']]=(action.get('at',frame),frame)
                    if action['kind']=='transition':
                        commands.append(dict(target=action['target'],sequence=sequence,at=action['at'],timing=action['timing'],operation=action['operation']))
                    elif action['kind']=='gain':
                        scheduled_gains.append(action)
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
                row=await driver.step(dict(transition=dict(collect_completed=False,collect=[],cancel=[],commands=commands),writes=[],work=dict(kind='audio',value=dict(inputs=step['inputs']))))
                assert row['error'] is None,row['error']
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
            actual['quanta'][-1]['outcomes'].sort(key=lambda r:r['name'])
            actual['media']=logical_media(row['audio']['media'],image['loop_views'],states)
            wire.dump(actual,OUTPUT/f'{index}.actual.json','result')
            wire.assert_exact(actual,expected)
        finally:memory.close()
