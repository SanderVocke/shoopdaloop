"""Block ownership, partial-range sharing, reuse and snapshot restore."""
import json
import os
from pathlib import Path
import random
import cocotb
import fpga_testbench as wire
from media_driver import Driver


@cocotb.test()
async def block_ownership(dut):
    capacities=tuple(map(int,os.environ['MEDIA_CAPACITIES'].split(',')))
    bc=int(os.environ['AXI_BLOCK_CHUNKS']);cf=capacities[2];bf=bc*cf
    output=Path(os.environ['MEDIA_OUTPUT']);reference=Path(os.environ['MEDIA_REFERENCE'])
    initial=dict(block_chunks=bc,chunk_frames=cf,chunk_limit=capacities[1],samples=[],free_chunks=[],views=[])
    operations=[dict(kind='create'),dict(kind='append',view=0,samples=[i*701-91000 for i in range(2*bf+3)]),
        dict(kind='slice',source=0,start=bf-3,frames=bf+6),
        dict(kind='slice',source=1,start=2,frames=bf+3),dict(kind='clear',view=0),
        dict(kind='append',view=0,samples=[1901]*cf),
        dict(kind='read',view=1,position=bf+5,length=bf+6,frames=3*cf+7),
        dict(kind='remove',view=1),dict(kind='reclaim',view=2),
        dict(kind='read',view=2,position=0,length=bf+1,frames=2*cf),
        dict(kind='reclaim',view=2),dict(kind='read',view=2,position=0,length=1,frames=cf),
        dict(kind='reclaim',view=2),dict(kind='remove',view=2),dict(kind='clear',view=0)]
    rng=random.Random(20261112)
    for _ in range(8):
        operations.extend([dict(kind='append',view=0,samples=[rng.randrange(-1000000,1000000) for _ in range(rng.randrange(1,bf+3))]),dict(kind='clear',view=0)])
    operations.extend([dict(kind='append',view=0,samples=[i-12000 for i in range(capacities[1]*cf)]),
        dict(kind='append',view=0,samples=[1]),dict(kind='clear',view=0)])
    case=dict(name='blocked-shared-ranges-reuse-capacity',media=initial,steps=operations)
    source=dict(format_version=3,payload=dict(kind='media_case',value=case))
    wire.dump(source,output/'block.case.json')
    trace=wire.run_reference('trace',source,executable=reference)
    driver=Driver(dut,capacities,20261112)
    try:
        await driver.load(initial)
        for i,(operation,target) in enumerate(zip(operations,trace['payload']['value']['results'])):
            actual=await driver.command(operation)
            wire.assert_media_result_matches(target,actual)
            if i==6:
                await driver.load(actual['media'])
                assert await driver.snapshot()==actual['media']
        assert trace['payload']['value']['results'][-2]['status']==dict(kind='failed',value=dict(error='memory_full'))
        (output/'block-summary.json').write_text(json.dumps(dict(commands=len(operations),result='passed'))+'\n')
    finally:driver.close()
