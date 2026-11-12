"""Connected loops retain their audio contract with allocation blocks."""
from copy import deepcopy
import os
import cocotb
import fpga_testbench as wire
from block_images import reblock
from test_media_loop_pipeline import compare, DEPTHS, MEDIA, REFERENCE
from loop_media_cases import directed, seeded, audio, document
from cases import case, spec, step, command
from pipeline_driver import PipelineDriver
from loop_memory import LoopMemory


@cocotb.test()
async def blocked_loops(dut):
    bc=int(os.environ['AXI_BLOCK_CHUNKS'])
    original=(MEDIA[0],MEDIA[1]//bc,MEDIA[2])
    values=[*directed(DEPTHS,original,REFERENCE),*seeded(DEPTHS,original,int(os.environ['LP_RANDOM_CASES']))]
    for value in values:
        value=deepcopy(value);value['media']=reblock(value['media'],bc,MEDIA[1])
        await compare(dut,value)
    rows=[step([command(0,1,dict(kind='record'))])]+[step() for _ in range(35)]
    rows += [step([command(0,2)]),step(),step([command(0,3,dict(kind='record'))]),step([command(0,4)])]
    long=audio(case('capture-across-allocation-block-boundaries',[spec(0,0)],rows,q=MEDIA[2],requests=2),DEPTHS,original,views=0)
    long['media']=reblock(long['media'],bc,MEDIA[1])
    await compare(dut,long)
    value=audio(case('block-cold-memory-fault',[spec(0,0)],[step([command(0,1,dict(kind='record'))]),step()],q=MEDIA[2],requests=2),DEPTHS,original,views=0)
    value['media']=reblock(value['media'],bc,MEDIA[1])
    limited=deepcopy(value);limited['name']='blocked-fatal-capacity-refusal';limited['media']['chunk_limit']=0
    await compare(dut,limited)
    image=wire.run_reference('compile',document('transition_plan',value['plan']),executable=REFERENCE)['payload']['value']
    driver=PipelineDriver(dut,DEPTHS);memory=LoopMemory(dut,MEDIA,20261112);driver.media=memory
    try:
        await driver.initialize(value,image)
        memory.fail_next=True
        failed=await driver.step(value['steps'][0])
        assert failed['error']==dict(stage='jobs',error='memory_fault')
        assert int(dut.faulted.value) and not int(dut.run_ready.value)
        assert failed['audio']['outputs']==[[0]*value['program']['quantum']]
        assert failed['transition']['next_frame']==0
        await driver.initialize(value,image)
        assert (await driver.step(value['steps'][0]))['error'] is None
    finally:memory.close()
