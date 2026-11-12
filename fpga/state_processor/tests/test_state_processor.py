"""Explicit instruction vectors and FIFO/protocol checks; no software SP executor."""
from copy import deepcopy
import json
import os
from pathlib import Path
import random
import cocotb
from cases import case, ins, ramp, word
from codec import encode_instruction, encode_value
from driver import Driver

DEPTHS=tuple(map(int,os.environ['SP_DEPTHS'].split(',')))
SEED=int(os.environ['SP_SEED'])
ARTIFACTS=Path(os.environ['SP_ARTIFACT_DIR'])

async def vector(dut,name,instruction,memory,jobs,*,status=0,after=None,budget=None,quantum=32):
    value=case(name,[instruction],memory,DEPTHS[2],budget=budget,quantum=quantum)
    driver=Driver(dut,DEPTHS,SEED)
    await driver.load(value)
    code=await driver.execute()
    observed=await driver.drain()
    assert (code,observed)==(status,jobs),(name,code,observed)
    assert await driver.snapshot()==dict(memory=memory if after is None else after),name

@cocotb.test()
async def instruction_vectors(dut):
    vectors=[
        (ins('input',port=65535,destination=417),dict(kind='input',port=65535,destination=417)),
        (ins('mix',left=65535,right=32768,destination=91),dict(kind='mix',left=65535,right=32768,destination=91)),
        (ins('output',source=65534,port=513),dict(kind='output',source=65534,port=513)),
        (ins('silence',destination=633),dict(kind='silence',destination=633)),
    ]
    for instruction,job in vectors:await vector(dut,instruction['kind'],instruction,[],[job])
    gain=ins('gain',ramp=0,source=91,destination=633)
    for q in (0,1,32,2**32-1):
        for duration,elapsed in ((0,0),(0,2**32-1),(1,0),(33,1),(2**32-1,2**32-16),(2**32-1,2**32-1)):
            memory=[ramp(duration,elapsed)];after=deepcopy(memory)
            after[0]['value']['elapsed']=min(duration,elapsed+q)
            await vector(dut,f'ramp-{q}-{duration}-{elapsed}',gain,memory,
                [dict(kind='gain',source=91,destination=633,ramp=memory[0]['value'])] if q else [],after=after if q else memory,status=0 if q else 2,quantum=q)
    await vector(dut,'wrong-ramp-type',gain,[word(0)],[],status=2)
    await vector(dut,'bad-ramp-address',dict(gain,ramp=65535),[],[],status=2)
    play=ins('loop_playback',view=65535,position=65535,length=65535,enabled=0,destination=633)
    await vector(dut,'disabled-playback',play,[word(0)],[dict(kind='silence',destination=633)])
    capture=ins('loop_capture',view=65535,enabled=0,source=911)
    await vector(dut,'disabled-capture',capture,[word(0)],[],budget=0)
    await vector(dut,'bad-enable',play,[ramp()],[],status=2)
    if DEPTHS[1]>=4:
        memory=[word(2**32-1),word(2**63),word(2**64-2),word(2**64-1)]
        play=dict(play,view=0,enabled=1,position=2,length=3)
        await vector(dut,'wide-playback',play,memory,[dict(kind='read_view',destination=633,view=2**32-1,position_chunks=2**64-2,length_chunks=2**64-1)])
        await vector(dut,'wide-capture',dict(capture,view=0,enabled=1),memory,[dict(kind='capture',source=911,view=2**32-1)])
        for field in ('view','enabled','position','length'):
            await vector(dut,'bad-'+field,dict(play,**{field:65535}),memory,[],status=2)
        for position,length in ((0,0),(9,9),(10,9)):
            bad=deepcopy(memory);bad[2:]=[word(position),word(length)]
            await vector(dut,'invalid-range',play,bad,[],status=2)
    await vector(dut,'last-state',dict(gain,ramp=DEPTHS[1]-1),[word(9)]*(DEPTHS[1]-1)+[ramp()],
        [dict(kind='gain',source=91,destination=633,ramp=ramp()['value'])],
        after=[word(9)]*(DEPTHS[1]-1)+[ramp(elapsed=32)])
    await vector(dut,'admission-refusal',gain,[ramp()],[],status=1,budget=DEPTHS[2]+1)

    # A delayed read-only loop-record slave checks the logical-node interface.
    class LoopDriver(Driver):
        pending=None
        response=0
        async def tick(self):
            resetting=int(self.dut.reset.value)
            ready=not resetting and self.rng.randrange(3)!=0
            valid=not resetting and self.pending==0
            self.set(loop_read_ready=int(ready),loop_response_valid=int(valid),loop_response_data=self.response)
            request=bool(int(self.dut.loop_read_valid.value) and ready)
            if request:assert int(self.dut.loop_read_slot.value)==3
            result=await super().tick()
            if resetting or valid:self.pending=None
            elif self.pending is not None:self.pending-=1
            if request:self.pending=3
            return result
    logical=dict(loop_id=dict(slot=3,generation=0x81234567),view=0,destination=91)
    for mode in (0,1,2,3):
        for generation in (0x81234567,0x81234566):
            driver=LoopDriver(dut,DEPTHS,SEED)
            driver.response=1|(generation<<1)|(mode<<33)|(64<<35)|(32<<99)|(1<<163)
            value=case('logical-playback',[ins('logical_playback',**logical)],[word(2**32-1)],DEPTHS[2])
            value['program']['chunk_frames']=32
            await driver.load(value)
            code=await driver.execute();jobs=await driver.drain()
            if generation!=0x81234567 or mode==3:assert (code,jobs)==(2,[])
            else:
                expected=dict(kind='read_view',destination=91,view=2**32-1,position_chunks=1,length_chunks=2) if mode==1 else dict(kind='silence',destination=91)
                assert (code,jobs)==(0,[expected])
    if DEPTHS[2]>=2:
        for first in (False,True):
            driver=LoopDriver(dut,DEPTHS,SEED)
            driver.response=1|(0x81234567<<1)|(2<<33)|(int(first)<<163)
            value=case('logical-capture',[ins('logical_capture',loop_id=logical['loop_id'],view=0,source=91)],[word(37)],DEPTHS[2],budget=2)
            await driver.load(value)
            assert await driver.execute()==0
            assert await driver.drain()==([dict(kind='begin_capture',view=37)] if first else [])+[dict(kind='capture',source=91,view=37)]

@cocotb.test()
async def fifo_order_wrap_and_partial_failure(dut):
    rng=random.Random(SEED)
    driver=Driver(dut,DEPTHS,SEED)
    count=min(DEPTHS[0],DEPTHS[2])
    instructions=[ins('input',port=rng.randrange(65536),destination=rng.randrange(65536)) for _ in range(count)]
    value=case('fifo-wrap',instructions,[],DEPTHS[2])
    expected=[dict(kind='input',port=i['port'],destination=i['destination']) for i in instructions]
    await driver.load(value)
    for _ in range(2*DEPTHS[2]//count+3):
        assert await driver.execute()==0
        assert await driver.drain()==expected
    if DEPTHS[0]>DEPTHS[2]:
        value=case('partial-fifo',instructions+[ins('gain',ramp=0,source=0,destination=0)],[ramp()],DEPTHS[2],budget=0)
        await driver.load(value)
        assert await driver.execute()==1
        assert await driver.drain()==expected
        assert await driver.snapshot()==value['initial_state']
    if DEPTHS[0]>=2:
        value=case('partial-invalid-state',[ins('input',port=0,destination=1),ins('gain',ramp=65535,source=1,destination=2)],[],DEPTHS[2],budget=0)
        await driver.load(value)
        assert await driver.execute()==2
        assert await driver.drain()==[dict(kind='input',port=0,destination=1)]

@cocotb.test()
async def configuration_and_error_protocol(dut):
    driver = Driver(dut, DEPTHS, SEED)
    value = case("protocol", [ins("gain", ramp=0, source=1, destination=2)], [ramp()], DEPTHS[2])
    await driver.load(value)
    for space, depth in ((False, DEPTHS[0]), (True, DEPTHS[1])):
        await driver.configure(space, depth, 123, expect_error=True)
        await driver.configure(space, depth, expect_error=True)
        await driver.configure(space, 65535, expect_error=True)
    assert await driver.snapshot() == value["initial_state"], "rejected configuration write aliased state"
    assert await driver.configure(False, 0) == encode_instruction(value["program"]["instructions"][0])

    driver.run_inputs()
    driver.set(cfg_valid=1, cfg_state=1, cfg_write=0, cfg_address=0)
    before, after = await driver.tick()
    assert before["cfg_ready"] and not before["run_ready"], "configuration did not win arbitration"
    assert after["cfg_response_valid"] and not after["busy"]
    driver.set(cfg_valid=0)
    before, _ = await driver.tick()
    assert not before["run_ready"], "run accepted before configuration response retired"
    driver.set(run_valid=0)

    # Hold configuration and a changed run request across every busy cycle.
    after = await driver.start()
    assert after["busy"]
    driver.set(cfg_valid=1, cfg_state=1, cfg_write=1, cfg_address=0, cfg_write_data=encode_value(word(7)),
               run_valid=1, run_instruction_count=0, job_ready=1)
    for _ in range(20):
        before, after = await driver.tick()
        if after["done"]:
            break
    else:
        raise AssertionError("run with blocked requests failed to finish")
    assert before["busy"] and not before["cfg_ready"] and not before["run_ready"]
    driver.set(cfg_valid=0, run_valid=0, job_ready=0)
    await driver.tick()
    expected_gain=dict(kind='gain',source=1,destination=2,ramp=deepcopy(value['initial_state']['memory'][0]['value']))
    assert await driver.drain()==[expected_gain]
    assert (await driver.snapshot())['memory'][0]['value']['elapsed']==32

    await driver.load(value)
    assert await driver.execute(run_instruction_count=DEPTHS[0] + 1) == 3
    assert await driver.snapshot() == value["initial_state"]
    assert await driver.execute(run_state_count=DEPTHS[1] + 1) == 2
    assert await driver.snapshot() == value["initial_state"]
    await driver.configure(False, 0, 255)
    assert await driver.execute() == 3
    assert await driver.drain() == []
    assert await driver.snapshot() == value["initial_state"]

    for bad in (65536, (1 << 32) - 1, -1):
        try:
            encode_instruction(ins("input", port=bad, destination=0))
        except ValueError:
            pass
        else:
            raise AssertionError("codec silently truncated an unrepresentable operand")


@cocotb.test()
async def nonempty_admission_and_reset(dut):
    driver = Driver(dut, DEPTHS, SEED)
    value = case("reset", [ins("gain", ramp=0, source=1, destination=2)], [ramp()], DEPTHS[2])
    await driver.load(value)
    after = await driver.start()
    assert after["busy"]
    driver.set(reset=1)
    _, after = await driver.tick()
    assert not after["busy"] and not after["job_valid"] and not after["queued_jobs"]
    driver.set(reset=0)
    await driver.tick()
    assert await driver.snapshot() == value["initial_state"]
    assert await driver.execute() == 0
    updated = await driver.snapshot()
    assert int(dut.queued_jobs.value) == 1
    # A second run must fail without mutating the completed first run's ramp or queue.
    assert await driver.execute() == 1
    assert await driver.snapshot() == updated
    assert int(dut.queued_jobs.value) == 1
    driver.set(reset=1)
    await driver.tick()
    driver.set(reset=0)
    await driver.tick()
    assert int(dut.queued_jobs.value) == 0
    assert await driver.snapshot() == updated, "reset cleared retained architectural state"
    assert await driver.configure(False, 0) == encode_instruction(value["program"]["instructions"][0])
    before=updated['memory'][0]['value']
    result=await driver.quantum([])
    assert result['status']==dict(kind='completed')
    assert result['jobs']==[dict(kind='gain',source=1,destination=2,ramp=before)]
    assert result['state']['memory'][0]['value']['elapsed']==64
