"""Compare complete musical quanta including samples and persistent media."""
import json
import os
from pathlib import Path
import cocotb
from cocotb.triggers import RisingEdge, with_timeout
import fpga_testbench as wire
from pipeline_driver import PipelineDriver
from loop_memory import LoopMemory
from loop_media_cases import audio, empty, recorded, directed, seeded, document
from cases import case, spec, step, command

DEPTHS = tuple(map(int, os.environ['LP_DEPTHS'].split(',')))
MEDIA = tuple(map(int, os.environ['LP_MEDIA_CAPACITIES'].split(',')))
OUTPUT = Path(os.environ['LP_OUTPUT'])
REFERENCE = Path(os.environ['LP_REFERENCE'])


async def compare(dut, value, initial_override=None):
    source = document('pipeline_case', value)
    wire.dump(source,OUTPUT/'current.case.json')
    trace = wire.run_reference('trace', source, executable=REFERENCE)
    wire.run_reference('check', trace, executable=REFERENCE)
    image = wire.run_reference('compile', document('transition_plan', value['plan']), executable=REFERENCE)['payload']['value']
    driver = PipelineDriver(dut, DEPTHS, 20261111)
    memory = LoopMemory(dut, MEDIA, 20261111)
    memory.initial_override = initial_override
    driver.media = memory
    observed = []
    try:
        await driver.initialize(value, image)
        for i, (row, expected) in enumerate(zip(value['steps'], trace['payload']['value']['results'])):
            actual = await driver.step(row)
            observed.append(actual)
            assert actual == expected, f'{value["name"]} quantum {i} differs from Rust'
    except BaseException:
        wire.dump(source, OUTPUT/'failure.case.json')
        wire.dump(trace, OUTPUT/'failure.expected.trace.json')
        (OUTPUT/'failure.observed.json').write_text(json.dumps(observed, indent=2)+'\n')
        raise
    finally:
        memory.close()
    return dict(quanta=len(observed), reads=memory.read_requests, writes=memory.write_requests)


@cocotb.test()
async def directed_against_rust(dut):
    summary = {}
    replay = os.environ.get('LP_CASE')
    if replay:
        loaded = wire.load(replay)['payload']
        value = loaded['value']['case'] if loaded['kind'] == 'pipeline_trace' else loaded['value']
        values = [value]
    else:
        values = directed(DEPTHS, MEDIA, REFERENCE)
    for value in values:
        summary[value['name']] = await compare(dut, value)
    if not replay:
        assert any(row['reads'] for row in summary.values())
        assert any(row['writes'] for row in summary.values())
    (OUTPUT/'media-directed-summary.json').write_text(json.dumps(summary, indent=2)+'\n')


@cocotb.test()
async def seeded_against_rust(dut):
    summary = {}
    for value in seeded(DEPTHS, MEDIA, int(os.environ['LP_RANDOM_CASES'])):
        summary[value['name']] = await compare(dut, value)
    (OUTPUT/'media-seeded-summary.json').write_text(json.dumps(summary, indent=2)+'\n')



@cocotb.test()
async def fatal_resources_silence_and_reset(dut):
    q=next(q for q in (1,16,32,64,128) if q>=MEDIA[2])
    # View creation is engine work, even when the host supplies no descriptors.
    value=audio(case('automatic-view-creation',[spec(0,0)],[
        step([command(0,1,dict(kind='record'))]),step([command(0,2)]),step(),
    ],q=q,requests=2),DEPTHS,MEDIA,views=0)
    await compare(dut,value)

    value=audio(case('fatal-state-generation',[spec(0,0)],[step(),step()],q=q,requests=2),DEPTHS,MEDIA,views=0)
    value['program']['instructions'][1]['loop_id']['generation']+=1
    await compare(dut,value)

    # Output is written before allocation fails. The published block must be zero.
    value=audio(case('fatal-arena-exhaustion',[spec(0,0)],[
        step([command(0,1,dict(kind='record'))]),step(),step(),
    ],q=q,requests=2),DEPTHS,MEDIA,views=0)
    value['media']['chunk_limit']=q//MEDIA[2]
    value['program']['instructions'].insert(2,dict(kind='process_output',source=0,port=0))
    value['program']['jobs_per_quantum']+=1
    await compare(dut,value)

    # Physical descriptor capacity is finite; the functional store is elastic.
    value=audio(case('fatal-descriptor-exhaustion',[spec(0,0)],[
        step([command(0,1,dict(kind='record'))]),step(),
    ],q=q,requests=2),DEPTHS,MEDIA,views=0)
    value['media']=empty(MEDIA,MEDIA[0])
    value['initial_state']['memory'][1]['value']=MEDIA[0]
    image=wire.run_reference('compile',document('transition_plan',value['plan']),executable=REFERENCE)['payload']['value']
    driver=PipelineDriver(dut,DEPTHS);memory=LoopMemory(dut,MEDIA,20261112);driver.media=memory
    try:
        await driver.initialize(value,image)
        first=await driver.step(value['steps'][0])
        assert first['error']==dict(stage='jobs',error='memory_full')
        assert first['audio']['outputs']==[[0]*q]
        assert int(dut.faulted.value) and not int(dut.run_ready.value)
        second=await driver.step(value['steps'][1])
        assert second['error']==first['error'] and second['transition']['next_frame']==0
        value['initial_state']['memory'][1]['value']=0
        await driver.initialize(value,image)
        assert (await driver.step(value['steps'][0]))['error'] is None
    finally:memory.close()

    # A memory response failure is terminal, including across later callbacks.
    value=audio(case('fatal-memory-response',[spec(0,q)],[step([command(0,1)]),step()],q=q,requests=2),DEPTHS,MEDIA)
    value['media']=recorded(MEDIA,[dict(kind='create'),dict(kind='append',view=0,samples=[7654321]*q)],REFERENCE)
    image=wire.run_reference('compile',document('transition_plan',value['plan']),executable=REFERENCE)['payload']['value']
    driver=PipelineDriver(dut,DEPTHS);memory=LoopMemory(dut,MEDIA,20261112);driver.media=memory
    try:
        for cold_metadata in (False,True):
            await driver.initialize(value,image)
            if cold_metadata:
                await memory.fence();await memory.configure(5,0,1)
            memory.fail_next=True
            failed=await driver.step(value['steps'][0])
            assert failed['error']==dict(stage='jobs',error='memory_fault')
            assert failed['audio']['outputs']==[[0]*q] and failed['transition']['next_frame']==0
            assert (await driver.step(value['steps'][1]))['audio']['outputs']==[[0]*q]
            memory.fail_next=False
            await driver.initialize(value,image)
            assert (await driver.step(value['steps'][0]))['audio']['outputs']==[[7654321]*q]

        # Reset drains the simulated old-response epoch before another run.
        await driver.initialize(value,image);memory.pause_responses=True
        running=cocotb.start_soon(driver.step(value['steps'][0]))
        for _ in range(10000):
            await driver.edge()
            if memory.pending is not None:break
        else:raise AssertionError('no outstanding memory request')
        running.kill();await driver.reset()
        assert memory.pending is None
        memory.pause_responses=False
        await driver.initialize(value,image)
        assert (await driver.step(value['steps'][0]))['audio']['outputs']==[[7654321]*q]
    finally:memory.close()

    # A completed buffered capture can fault later during writeback.
    value=audio(case('late-writeback-fault',[spec(0,0)],[step([command(0,1,dict(kind='record'))]),step()],q=q,requests=2),DEPTHS,MEDIA,views=0)
    value['media']=None
    image=wire.run_reference('compile',document('transition_plan',value['plan']),executable=REFERENCE)['payload']['value']
    driver=PipelineDriver(dut,DEPTHS);memory=LoopMemory(dut,MEDIA,20261112);driver.media=memory
    try:
        initial=empty(MEDIA,1)
        initial['samples']=[0]*q
        initial['free_chunks']=list(range(q//MEDIA[2]))
        memory.initial_override=initial
        await driver.initialize(value,image)
        memory.pause_responses=True;memory.fail_next=True
        first=await driver.step(value['steps'][0])
        assert first['error'] is None and first['transition']['next_frame']==q
        memory.pause_responses=False
        for _ in range(10000):
            await driver.edge()
            if int(dut.faulted.value):break
        else:raise AssertionError('late writeback fault was not latched')
        second=await driver.step(value['steps'][1])
        assert second['error']==dict(stage='jobs',error='memory_fault')
        assert second['transition']['next_frame']==q and second['audio']['outputs']==[[0]*q]
    finally:memory.close()
