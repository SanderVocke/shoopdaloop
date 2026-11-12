"""Exercise ordered sample jobs with block ownership and retained ranges."""
from copy import deepcopy
import os
import cocotb
from block_images import reblock
from test_media_pipeline import record_case, dependent_case, compare, CAPACITIES, MEDIA, SEED


@cocotb.test()
async def blocked_audio(dut):
    bc=int(os.environ['AXI_BLOCK_CHUNKS']);cf=MEDIA[2]
    values=[record_case(cf,SEED),record_case(CAPACITIES[3],SEED+1)]
    if CAPACITIES[1]>=11:values.append(dependent_case())
    shared=record_case(cf,SEED+2)
    shared['name']='block-retained-range-and-reused-writer'
    shared['media'].update(samples=list(range(1,3*cf+1)),views=[dict(chunks=[0,1,2],frames=3*cf)])
    capture=shared['program']['instructions'][2];playback=shared['program']['instructions'][3]
    shared['initial_state']['memory'][capture['enabled']]['value']=0
    shared['initial_state']['memory'][playback['enabled']]['value']=1
    shared['initial_state']['memory'][playback['view']]['value']=1
    shared['steps']=[dict(writes=[],inputs=[[0]*cf],media_actions=[dict(kind='slice',source=0,start=cf,frames=cf),dict(kind='clear',view=0)]),
        dict(writes=[dict(address=capture['enabled'],value=dict(kind='word',value=1))],inputs=[[1234]*cf])]
    values.append(shared)
    for i,value in enumerate(values):
        value=deepcopy(value);value['media']=reblock(value['media'],bc,MEDIA[1])
        await compare(dut,value,SEED+i)
