"""Directed sample/media fixtures and hardware stream/fault/reset checks."""
from copy import deepcopy
import json
import os
from pathlib import Path
import random
import cocotb
from pipeline_driver import PipelineDriver
from codec import encode_value
from cases import ins,word
from audio_cases import ramp,ONE

CAPACITIES=tuple(map(int,os.environ['AUDIO_CAPACITIES'].split(',')))
MEDIA=tuple(map(int,os.environ['MEDIA_CAPACITIES'].split(',')))
SEED=int(os.environ['MEDIA_SEED'])
OUTPUT=Path(os.environ['MEDIA_OUTPUT'])

def clip(x):return min(2**31-1,max(-2**31,x))
def product(a,b):
    n=a*b
    return clip((abs(n)//ONE)*(-1 if n<0 else 1))

def record_case(frames,seed=0):
    rng=random.Random(seed)
    instructions=[ins('input',port=0,destination=0),ins('gain',ramp=0,source=0,destination=1),
        ins('loop_capture',view=1,enabled=2,source=1),
        ins('loop_playback',view=3,position=4,length=5,enabled=6,destination=3),ins('output',source=3,port=0)]
    def writes(record,play):return [dict(address=a,value=word(v)) for a,v in ((2,record),(6,play))]
    return dict(name=f'record-play-reuse-{frames}-{seed}',
        program=dict(quantum=frames,chunk_frames=MEDIA[2],instructions=instructions,jobs_per_quantum=5),
        initial_state=dict(memory=[ramp(ONE//2,ONE//2),word(0),word(1),word(0),word(0),word(frames//MEDIA[2]),word(0)]),
        queue_capacity=CAPACITIES[2],buffers=5,output_ports=2,
        media=dict(chunk_frames=MEDIA[2],chunk_limit=MEDIA[1],samples=[],free_chunks=[],views=[dict(chunks=[],frames=0)]),
        steps=[dict(writes=[],inputs=[[rng.randint(-2**31,2**31-1) for _ in range(frames)]]),
               dict(writes=writes(0,1),inputs=[[0]*frames]),
               dict(writes=writes(1,0),inputs=[[-3]*frames],media_actions=[dict(kind='clear',view=0)]),
               dict(writes=writes(0,1),inputs=[[0]*frames])])

def record_expected(case):
    frames=case['program']['quantum'];silence=[0]*frames
    captured=[[product(x,ONE//2) for x in case['steps'][i]['inputs'][0]] for i in (0,2)]
    return [dict(status=dict(kind='completed'),outputs=[data,silence],buffers={}) for data in (silence,captured[0],silence,captured[1])]

def playback_expected(case):
    frames=case['program']['quantum'];data=case['media']['samples'][:MEDIA[2]]
    return dict(status=dict(kind='completed'),outputs=[[data[i%len(data)] for i in range(frames)]],buffers={})

def assert_audio(expected,actual):
    assert actual['status']==expected['status'],actual['status']
    assert actual['outputs']==expected['outputs']
    for buffer,samples in expected['buffers'].items():assert actual['buffers'][buffer]==samples

async def record_session(dut,frames,seed):
    value=record_case(frames,seed);driver=PipelineDriver(dut,CAPACITIES,MEDIA,seed)
    try:
        await driver.load(value)
        for step,expected in zip(value['steps'],record_expected(value)):
            actual=await driver.quantum_audio(step)
            assert_audio(expected,actual)
        last=[product(x,ONE//2) for x in value['steps'][2]['inputs'][0]]
        result=await driver.media.command(dict(kind='read',view=0,position=0,length=frames,frames=frames))
        assert result['value']==dict(kind='samples',value=last)
    finally:driver.media.close()

@cocotb.test()
async def directed_pipeline(dut):
    for frames in sorted({MEDIA[2],CAPACITIES[3]}):await record_session(dut,frames,SEED)
    cf=MEDIA[2]
    data=[100000+i*128 for i in range(cf)]
    instructions=[ins('loop_playback',view=0,position=2,length=3,enabled=1,destination=0),
        ins('gain',ramp=4,source=0,destination=1),ins('loop_capture',view=0,enabled=1,source=1),
        ins('loop_playback',view=0,position=5,length=6,enabled=1,destination=3),ins('output',source=3,port=0)]
    value=dict(name='read-append-read-dependency',program=dict(quantum=cf,chunk_frames=cf,instructions=instructions,jobs_per_quantum=5),
        initial_state=dict(memory=[word(0),word(1),word(0),word(1),ramp(ONE//2,ONE//2),word(1),word(2)]),
        queue_capacity=CAPACITIES[2],buffers=5,output_ports=1,
        media=dict(chunk_frames=cf,samples=data,free_chunks=[],views=[dict(chunks=[0],frames=cf)]))
    driver=PipelineDriver(dut,CAPACITIES,MEDIA,SEED)
    try:
        await driver.load(value)
        for index in range(3):
            writes=[dict(address=a,value=word(v)) for a,v in ((2,index),(3,index+1),(5,index+1),(6,index+2))]
            actual=await driver.quantum_audio(dict(writes=writes,inputs=[]))
            data=[product(x,ONE//2) for x in data]
            assert_audio(dict(status=dict(kind='completed'),outputs=[data],buffers={3:data}),actual)
    finally:driver.media.close()

@cocotb.test()
async def randomized_pipeline(dut):
    for i in range(int(os.environ['MEDIA_RANDOM_CASES'])):await record_session(dut,MEDIA[2],SEED+i+1)

@cocotb.test()
async def playback_stream_protocol(dut):
    driver = PipelineDriver(dut, CAPACITIES, MEDIA, SEED+91)
    core = dut.core.pipeline.sample_engine if driver.media.axi is not None else dut.pipeline.sample_engine
    writes = []
    before_clock = driver.media.before_clock

    def observe():
        before_clock()
        if not int(dut.reset.value) and int(core.playback_write.value):
            assert not (int(dut.buffer_ready.value) & 1), 'playback published before its final write'
            writes.append((driver.media.cycles, int(core.scratch_ram_write_address.value)))

    driver.media.before_clock = observe

    def make_case(frames, second_view=False):
        memory=[word(0),word(0),word(1),word(1)]
        instructions=[ins('loop_playback',view=0,position=1,length=2,enabled=3,destination=0)]
        if second_view:
            memory += [word(1),word(0),word(1),word(1)]
            instructions.append(ins('loop_playback',view=4,position=5,length=6,enabled=7,destination=1))
        instructions.append(ins('output',source=0,port=0))
        compiled=dict(program=dict(quantum=frames,chunk_frames=MEDIA[2],instructions=instructions,jobs_per_quantum=len(instructions)),initial_state=memory)
        views = 2 if second_view else 1
        return {'name':'playback-stream', 'program':compiled['program'],
            'initial_state':{'memory':compiled['initial_state']}, 'queue_capacity':CAPACITIES[2],
            'buffers':2, 'output_ports':1,
            'media':{'chunk_frames':MEDIA[2], 'samples':[13-7*i for i in range(views*MEDIA[2])],
                     'free_chunks':[], 'views':[{'chunks':[i], 'frames':MEDIA[2]} for i in range(views)]},
            'steps':[{'writes':[], 'inputs':[]}]}

    async def warm(case):
        await driver.load(case)
        result = await driver.media.command({'kind':'read', 'view':0, 'position':0,
                                            'length':MEDIA[2], 'frames':case['program']['quantum']}, stalls=False)
        assert result['status']['kind'] == 'completed'
        writes.clear()

    frames = min(CAPACITIES[3], MEDIA[2])
    case = make_case(frames)
    target = playback_expected(case)
    assert target['status']['kind'] == 'completed'
    await warm(case)
    assert_audio(target, await driver.quantum_audio(case['steps'][0], stalls=False))
    assert [address for _,address in writes] == list(range(frames))
    assert all(b[0]-a[0] == 1 for a,b in zip(writes, writes[1:])), 'playback scratch writes were not consecutive'
    for prefix in sorted({0, min(1, frames-1), frames//2, frames-1}):
        await warm(case)
        driver.consumed, driver.drained = [], []
        driver.set(run_buffers=2, run_inputs=0, run_outputs=1, job_enable=1, drain_ready=0)
        await driver.start()
        for _ in range(10000):
            if (len(writes) >= prefix and int(core.media_read_ready.value)
                    and (prefix != 0 or int(core.playback_write.value))):
                break
            await driver.tick()
        else:
            raise AssertionError('playback did not reach reset placement')
        driver.set(reset=1)
        await driver.tick()
        driver.set(reset=0)
        for _ in range(6):
            await driver.tick()
            assert not int(core.scratch_write.value) and not int(dut.buffer_ready.value), 'stale playback write after reset'
        await warm(case)
        assert_audio(target, await driver.quantum_audio(case['steps'][0], stalls=False))

    if frames >= 16 and driver.media.axi is not None:
        abort_case = make_case(frames, second_view=True)
        await warm(abort_case)
        driver.media.pause_responses = True
        driver.media.fail_next = True
        driver.consumed, driver.drained = [], []
        driver.set(run_buffers=2, run_inputs=0, run_outputs=1, job_enable=1, drain_ready=0)
        await driver.start()
        for _ in range(10000):
            await driver.tick()
            if writes:
                break
        else:
            raise AssertionError('cached playback did not overlap failing prefetch')
        assert driver.media.pending is not None and driver.media.pending[2]
        driver.media.pause_responses = False
        for _ in range(10000):
            _, after = await driver.tick()
            if after['done']:
                break
        else:
            raise AssertionError('playback did not abort on a memory fault')
        assert after['error_stage'] == 2 and after['error_code'] == 8
        assert not (after['buffer_ready'] & 1)
        for _ in range(6):
            await driver.tick()
            assert not int(core.playback_write.value) and not int(core.scratch_write.value)
        await warm(case)
        assert_audio(target, await driver.quantum_audio(case['steps'][0], stalls=False))
    driver.media.close()


@cocotb.test()
async def pipeline_protocol(dut):
    value = record_case(MEDIA[2])
    target=record_expected(value)[0]
    driver = PipelineDriver(dut, CAPACITIES, MEDIA, SEED)
    await driver.load(value)
    for port, samples in enumerate(value["steps"][0]["inputs"]):
        for frame, sample in enumerate(samples):
            await driver.audio_configure(0, port*driver.max_frames+frame, sample)
    driver.set(run_buffers=value["buffers"], run_inputs=1, run_outputs=2, job_enable=0, drain_ready=0)
    await driver.start()
    for _ in range(200):
        _, after = await driver.tick()
        if after["jobs_busy"]:
            break
    else:
        raise AssertionError("jobs did not start")
    driver.media.set(cmd_valid=1, cmd_opcode=0, cfg_valid=1, cfg_write=1,
                     cfg_space=0, cfg_address=0, cfg_write_data=0)
    driver.set(run_valid=1, cfg_valid=1, cfg_state=1, cfg_write=1, cfg_address=0, cfg_write_data=7,
               audio_cfg_valid=1, audio_cfg_space=0, audio_cfg_write=1, audio_cfg_address=0, audio_cfg_write_data=123)
    for _ in range(25):
        before, _ = await driver.tick()
        assert not before["cfg_ready"] and not before["audio_cfg_ready"] and not before["run_ready"]
        assert not int(dut.store_cmd_ready.value) and not int(dut.store_cfg_ready.value)
    driver.media.set(cmd_valid=0, cfg_valid=0)
    driver.set(run_valid=0, cfg_valid=0, audio_cfg_valid=0, job_enable=1)
    for _ in range(10000):
        _, after = await driver.tick()
        if after["done"]:
            break
    else:
        raise AssertionError("pipeline did not finish")
    assert after["error_code"] == 0
    await driver.tick()
    buffers, outputs = await driver.read_samples()
    observed = {"jobs": driver.consumed, "remaining_jobs": [], "state": await driver.snapshot(),
                "buffers": buffers, "outputs": outputs, "stage": "jobs", "status": {"kind": "completed"},
                "media": await driver.media.snapshot()}
    assert_audio(target, observed)
    fault_case = deepcopy(value)
    fault_case["media"]["samples"] = list(range(1, MEDIA[2]+1))
    fault_case["media"]["views"] = [{"chunks": [0], "frames": MEDIA[2]}]
    await driver.load(fault_case)
    for write in fault_case["steps"][1]["writes"]:
        await driver.configure(True, write["address"], encode_value(write["value"]))
    driver.media.fail_next = True
    assert await driver.execute_audio(value["steps"][1]["inputs"]) == (2, 8)
    assert not int(dut.run_ready.value), "memory fault allowed another quantum"
    await driver.load(value)
    assert_audio(target,
                                    await driver.quantum_audio(value["steps"][0]))
    if driver.media.axi is not None and not driver.media.native_memory and CAPACITIES[3] >= 16:
        frames, cf = CAPACITIES[3], MEDIA[2]
        instructions=[ins('input',port=0,destination=0),ins('gain',ramp=0,source=0,destination=1),
            ins('mix',left=0,right=1,destination=2),ins('loop_playback',view=1,position=2,length=3,enabled=4,destination=3),
            ins('output',source=2,port=0)]
        compiled=dict(program=dict(quantum=frames,chunk_frames=cf,instructions=instructions,jobs_per_quantum=5),
            initial_state=[ramp(ONE//2,ONE//2),word(0),word(0),word(frames//cf),word(1)])
        abort_case = {"name": "fault-during-arithmetic-stream", "program": compiled["program"],
            "initial_state": {"memory": compiled["initial_state"]}, "queue_capacity": CAPACITIES[2],
            "buffers": 4, "output_ports": 1,
            "media": {"chunk_frames": cf, "samples": list(range(frames)), "free_chunks": [],
                      "views": [{"chunks": list(range(frames//cf)), "frames": frames}]},
            "steps": [{"writes": [], "inputs": [[7*i-13 for i in range(frames)]]}]}
        data=abort_case['steps'][0]['inputs'][0]
        half=[product(x,ONE//2) for x in data]
        complete_buffers={1:half,2:[clip(a+b) for a,b in zip(data,half)]}
        core = dut.core.pipeline.sample_engine
        for kind, destination in (("gain", 1), ("mix", 2)):
            await driver.load(abort_case)
            driver.media.stalls = False
            driver.media.pause_responses = True
            driver.media.fail_next = True
            for frame, sample in enumerate(abort_case["steps"][0]["inputs"][0]):
                await driver.audio_configure(0, frame, sample)
            driver.consumed, driver.drained = [], []
            driver.set(run_buffers=4, run_inputs=1, run_outputs=1, job_enable=1, drain_ready=0)
            await driver.start()
            for _ in range(1000):
                await driver.tick()
                if (driver.consumed and driver.consumed[-1]["kind"] == kind
                        and int(core.streaming.value) and int(core.stream_store_valid.value)):
                    break
            else:
                raise AssertionError(f"{kind} did not start while the failing prefetch was held")
            assert driver.media.pending is not None and driver.media.pending[2]
            driver.media.pause_responses = False
            ready_at_fault = None
            for _ in range(1000):
                _, after = await driver.tick()
                if int(core.media_fault.value) and ready_at_fault is None:
                    ready_at_fault = after["buffer_ready"] & (1 << destination)
                if after["done"]:
                    break
            else:
                raise AssertionError("arithmetic stream did not abort on a memory fault")
            assert after["error_stage"] == 2 and after["error_code"] == 8
            assert ready_at_fault is not None
            assert after["buffer_ready"] & (1 << destination) == ready_at_fault
            if ready_at_fault:
                buffers, _ = await driver.read_samples()
                assert buffers[destination] == complete_buffers[destination]
            for _ in range(6):
                await driver.tick()
                assert not int(core.stream_store_valid.value) and not int(core.scratch_write.value)
        await driver.load(value)
        assert_audio(target,
                                        await driver.quantum_audio(value["steps"][0]))
    elif driver.media.axi is not None:
        dut._log.info('Active-arithmetic fault placement needs a Python backend and >=16 frames; short jobs can finish before burst drain reports a fault')
    driver.media.close()
