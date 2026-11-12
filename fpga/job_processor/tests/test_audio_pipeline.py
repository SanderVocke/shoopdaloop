"""Arithmetic specifications and hardware stream/reset checks, without a Rust executor."""
from copy import deepcopy
import json
import os
from pathlib import Path
import random
import cocotb
from audio_cases import graph_case, make_case, ramp, samples, ONE, MIN, MAX
from audio_driver import AudioDriver
from cases import ins

CAPACITIES=tuple(map(int,os.environ['AUDIO_CAPACITIES'].split(',')))
SEED=int(os.environ['AUDIO_SEED'])
OUTPUT=Path(os.environ['AUDIO_OUTPUT'])

def clip(value):return min(MAX,max(MIN,value))
def trunc0(n,d):return (abs(n)//d)*(-1 if n<0 else 1)
def product(a,b):return clip(trunc0(a*b,ONE))
def curve(r,n):
    return r['target'] if not r['duration'] else r['start']+trunc0((r['target']-r['start'])*min(r['elapsed']+n,r['duration']),r['duration'])

def compile_graphs():
    instructions=[ins('input',port=0,destination=0),ins('gain',ramp=0,source=0,destination=1),
        ins('input',port=1,destination=2),ins('gain',ramp=1,source=2,destination=3),
        ins('mix',left=1,right=3,destination=4),ins('output',source=4,port=0)]
    return {n:dict(program=dict(instructions=instructions,quantum=n,chunk_frames=1,jobs_per_quantum=6),
        initial_state=[ramp(ONE,ONE),ramp(ONE,ONE)],bindings=[dict(kind='gain',value=dict(ramp=i)) for i in range(2)])
        for n in sorted({1,min(3,CAPACITIES[3]),CAPACITIES[3]})}

def numeric_expect(case,shape='two'):
    # Two explicit arithmetic fixtures. This does not interpret an instruction stream.
    data=case['steps'][0]['inputs'];ramps=[v['value'] for v in case['initial_state']['memory']]
    first=[product(x,curve(ramps[0],i)) for i,x in enumerate(data[0])]
    if shape=='two':
        second=[product(x,curve(ramps[1],i)) for i,x in enumerate(data[1])]
        mixed=[clip(a+b) for a,b in zip(first,second)]
        buffers={0:data[0],1:first,2:data[1],3:second,4:mixed}
    else:
        mixed=[clip(a+b) for a,b in zip(data[0],first)]
        buffers={0:data[0],1:first,2:mixed}
    return dict(outputs=[mixed]+[[0]*len(mixed) for _ in range(case['output_ports']-1)],buffers=buffers)

def assert_audio(expected,actual):
    assert actual['status']==dict(kind='completed'),actual['status']
    assert actual['outputs']==expected['outputs']
    for buffer,samples in expected['buffers'].items():assert actual['buffers'][buffer]==samples

@cocotb.test()
async def directed_audio(dut):
    comparisons=0
    settings=[ramp(0,target) for target in (0,1,-1,ONE,-ONE,ONE//2,MIN,MAX)]
    settings += [ramp(MIN,MAX,2**32-1,2**32-17),ramp(MAX,MIN,2**32-1,1234567890),
                 ramp(-ONE,ONE,1),ramp(ONE,-ONE,17,15),ramp(MIN,MAX,0,2**32-1)]
    for frames,compiled in compile_graphs().items():
        for i,setting in enumerate(settings):
            case=graph_case(compiled,CAPACITIES,frames,ramps=[setting,settings[-i-1]],steps=1)
            driver=AudioDriver(dut,CAPACITIES,SEED+i)
            await driver.load(case)
            assert_audio(numeric_expect(case),await driver.quantum_audio(case['steps'][0]))
            comparisons+=frames
        for bad,expected in [(ins('input',port=65535,destination=0),1),
                             (ins('mix',left=0,right=65535,destination=2),1),
                             (ins('output',source=65535,port=0),1),
                             (ins('gain',ramp=0,source=1,destination=2),1)]:
            case=make_case('invalid-job',[bad],[ramp(ONE,ONE)],CAPACITIES,frames=frames,steps=1)
            driver=AudioDriver(dut,CAPACITIES,SEED)
            await driver.load(case)
            actual=await driver.execute_audio(case['steps'][0]['inputs'])
            assert actual==(2,expected),(bad,actual,expected)
    (OUTPUT/'directed-summary.json').write_text(json.dumps(dict(exact_arithmetic_samples=comparisons))+'\n')

@cocotb.test()
async def randomized_audio(dut):
    rng=random.Random(SEED)
    for index in range(int(os.environ['AUDIO_RANDOM_CASES'])):
        frames=rng.choice(list(compile_graphs()))
        settings=[ramp(rng.randint(MIN,MAX),rng.randint(MIN,MAX),rng.getrandbits(32),rng.getrandbits(32)) for _ in range(2)]
        case=graph_case(compile_graphs()[frames],CAPACITIES,frames,ramps=settings,steps=1)
        case['steps'][0]['inputs']=[[rng.randint(MIN,MAX) for _ in range(frames)] for _ in range(2)]
        driver=AudioDriver(dut,CAPACITIES,SEED+index)
        await driver.load(case)
        assert_audio(numeric_expect(case),await driver.quantum_audio(case['steps'][0]))

@cocotb.test()
async def sample_protocol(dut):
    compiled = compile_graphs()[CAPACITIES[3]]
    case = graph_case(compiled, CAPACITIES, CAPACITIES[3],
                      ramps=[ramp(0, ONE, 71), ramp(ONE, -ONE, 100)], steps=1)
    driver = AudioDriver(dut, CAPACITIES, SEED)
    await driver.load(case)
    for space, size in ((0, CAPACITIES[5] * CAPACITIES[3]), (1, CAPACITIES[6] * CAPACITIES[3]),
                        (2, CAPACITIES[4] * CAPACITIES[3]), (3, 0)):
        await driver.audio_configure(space, size, MAX, expect_error=True)
        await driver.audio_configure(space, size, expect_error=True)
    for frame in range(CAPACITIES[3]):
        await driver.audio_configure(1, CAPACITIES[3] + frame, MIN if frame % 2 else MAX)
    expected = numeric_expect(case)
    assert_audio(expected, await driver.quantum_audio(case["steps"][0]))

    await driver.load(case)
    for port, data in enumerate(case["steps"][0]["inputs"]):
        for frame, value in enumerate(data):
            await driver.audio_configure(0, port * CAPACITIES[3] + frame, value)
    driver.set(run_buffers=case["buffers"], run_inputs=2, run_outputs=2, job_enable=0)
    await driver.start()
    for _ in range(200):
        _, after = await driver.tick()
        if after["jobs_busy"]:
            break
    else:
        raise AssertionError("job processor did not start")
    driver.set(cfg_valid=1, cfg_state=1, cfg_write=1, cfg_address=0, cfg_write_data=7,
               audio_cfg_valid=1, audio_cfg_space=0, audio_cfg_write=1, audio_cfg_address=0, audio_cfg_write_data=123,
               run_valid=1)
    queued = int(dut.queued_jobs.value)
    for _ in range(2 * CAPACITIES[3] + 20):
        before, after = await driver.tick()
        assert not before["monitor_ready"] and after["queued_jobs"] == queued
    driver.set(cfg_valid=0, audio_cfg_valid=0, run_valid=0, job_enable=1)
    for _ in range(300):
        await driver.tick()
        if any(job["kind"] == "gain" for job in driver.consumed):
            break
    else:
        raise AssertionError("no gain job accepted after FIFO unpause")
    for _ in range(10):
        await driver.tick()
    driver.set(reset=1)
    _, after = await driver.tick()
    assert not after["busy"] and not after["queued_jobs"] and not after["buffer_ready"]
    driver.set(reset=0)
    await driver.tick()
    assert await driver.audio_configure(0, 0) == case["steps"][0]["inputs"][0][0], "blocked sample write changed RAM"
    resumed = deepcopy(case)
    resumed["initial_state"] = await driver.snapshot()
    expected = numeric_expect(resumed)
    assert_audio(expected, await driver.quantum_audio(resumed["steps"][0]))

    unsupported = make_case("unsupported-capture", [ins("input", port=0, destination=0),
                           ins("loop_capture", view=0, enabled=1, source=0)],
                           [{"kind": "word", "value": 1}] * 2, CAPACITIES,
                           frames=min(3, CAPACITIES[3]), buffers=2, outputs=1, steps=1)
    await driver.load(unsupported)
    assert await driver.execute_audio(unsupported["steps"][0]["inputs"]) == (2, 3)


@cocotb.test()
async def arithmetic_stream_protocol(dut):
    frames = CAPACITIES[3]
    value = make_case('streaming-gain-mix', [ins('input', port=0, destination=0),
        ins('gain', ramp=0, source=0, destination=1), ins('mix', left=0, right=1, destination=2),
        ins('output', source=2, port=0)], [ramp(0, -ONE//2)], CAPACITIES,
        frames=frames, buffers=3, outputs=1, steps=1, inputs=[samples(frames)])
    target = numeric_expect(value,'gain_mix')
    driver = AudioDriver(dut, CAPACITIES, SEED)
    core = dut.sample_engine
    writes = {'gain': [], 'mix': []}

    def observe(cycle):
        if not int(dut.reset.value) and int(core.stream_write.value):
            kind = driver.consumed[-1]['kind']
            assert kind in writes
            destination = 1 if kind == 'gain' else 2
            assert not (int(dut.buffer_ready.value) & (1 << destination)), 'buffer published before its final write'
            writes[kind].append((cycle, int(core.scratch_ram_write_address.value)))

    driver.sample_observer = observe
    await driver.load(value)
    assert_audio(target, await driver.quantum_audio(value['steps'][0]))
    for kind, interval, destination in (('gain', 1, 1), ('mix', 2, 2)):
        assert len(writes[kind]) == frames
        assert [address for _,address in writes[kind]] == list(range(destination*frames, (destination+1)*frames))
        assert all(b[0]-a[0] == interval for a,b in zip(writes[kind], writes[kind][1:])), kind
    driver.sample_observer = None

    for count in sorted({1, frames}):
        for kind in ('gain', 'mix'):
            for delay in range(5):
                reset_case = deepcopy(value)
                reset_case['program']['quantum'] = count
                reset_case['initial_state']['memory'] = [ramp(ONE, -ONE, count+1)]
                reset_case['steps'][0]['inputs'] = [samples(count)]
                await driver.load(reset_case)
                for index, sample in enumerate(reset_case['steps'][0]['inputs'][0]):
                    await driver.audio_configure(0, index, sample)
                driver.set(run_buffers=3, run_inputs=1, run_outputs=1, job_enable=1, drain_ready=0)
                driver.consumed = []
                await driver.start()
                for _ in range(1000):
                    await driver.tick()
                    if driver.consumed and driver.consumed[-1]['kind'] == kind and int(core.streaming.value):
                        break
                else:
                    raise AssertionError(f'{kind} did not enter its streaming phase')
                for _ in range(delay):
                    await driver.tick()
                driver.set(reset=1)
                _, after = await driver.tick()
                assert not after['busy'] and not after['buffer_ready'] and not after['queued_jobs']
                driver.set(reset=0)
                for _ in range(6):
                    await driver.tick()
                    assert not int(core.scratch_write.value) and not int(core.stream_store_valid.value), 'stale pipeline write after reset'
                reset_case['initial_state'] = await driver.snapshot()
                expected = numeric_expect(reset_case,'gain_mix')
                assert_audio(expected, await driver.quantum_audio(reset_case['steps'][0]))
