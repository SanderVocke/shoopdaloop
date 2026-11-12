"""Exercise ordered sample jobs with block ownership and retained ranges."""
import os
import cocotb
from block_images import reblock
from test_media_pipeline import record_case, record_expected, assert_audio, product, CAPACITIES, MEDIA, SEED, ONE
from pipeline_driver import PipelineDriver


@cocotb.test()
async def blocked_audio(dut):
    bc=int(os.environ['AXI_BLOCK_CHUNKS']);cf=MEDIA[2]
    for frames in sorted({cf,CAPACITIES[3]}):
        value=record_case(frames,SEED)
        value['media']=reblock(value['media'],bc,MEDIA[1])
        driver=PipelineDriver(dut,CAPACITIES,MEDIA,SEED)
        try:
            await driver.load(value)
            for step,expected in zip(value['steps'],record_expected(value)):
                assert_audio(expected,await driver.quantum_audio(step))
            last=[product(x,ONE//2) for x in value['steps'][2]['inputs'][0]]
            result=await driver.media.command(dict(kind='read',view=0,position=0,length=frames,frames=frames))
            assert result['value']==dict(kind='samples',value=last)
        finally:driver.media.close()
    shared=record_case(cf,SEED+2)
    shared['name']='block-retained-range-and-reused-writer'
    shared['media'].update(samples=list(range(1,3*cf+1)),views=[dict(chunks=[0,1,2],frames=3*cf)])
    capture=shared['program']['instructions'][2];playback=shared['program']['instructions'][3]
    shared['initial_state']['memory'][capture['enabled']]['value']=0
    shared['initial_state']['memory'][playback['enabled']]['value']=1
    shared['initial_state']['memory'][playback['view']]['value']=1
    shared['steps']=[dict(writes=[],inputs=[[0]*cf],media_actions=[dict(kind='slice',source=0,start=cf,frames=cf),dict(kind='clear',view=0)]),
        dict(writes=[dict(address=capture['enabled'],value=dict(kind='word',value=1))],inputs=[[1234]*cf])]
    shared['media']=reblock(shared['media'],bc,MEDIA[1])
    driver=PipelineDriver(dut,CAPACITIES,MEDIA,SEED+2)
    try:
        await driver.load(shared)
        for step in shared['steps']:
            assert_audio(dict(status=dict(kind='completed'),outputs=[list(range(cf+1,2*cf+1)),[0]*cf],buffers={}),
                await driver.quantum_audio(step))
        result=await driver.media.command(dict(kind='read',view=0,position=0,length=cf,frames=cf))
        assert result['value']==dict(kind='samples',value=[617]*cf)
    finally:driver.media.close()
