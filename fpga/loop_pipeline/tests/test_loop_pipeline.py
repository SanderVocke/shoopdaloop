import json
import os
from pathlib import Path
import cocotb
from cocotb.triggers import FallingEdge, Timer, RisingEdge, with_timeout
import fpga_testbench as wire
from pipeline_driver import PipelineDriver
from pipeline_cases import directed, seeded, connect
from cases import case, spec, step, command

DEPTHS=tuple(map(int,os.environ['LP_DEPTHS'].split(',')))
OUTPUT=Path(os.environ['LP_OUTPUT'])
REFERENCE=Path(os.environ['LP_REFERENCE'])
def document(kind,value):return {'format_version':1,'payload':{'kind':kind,'value':value}}

async def compare(dut,value):
    source=document('pipeline_case',value)
    trace=wire.run_reference('trace',source,executable=REFERENCE)
    wire.run_reference('check',trace,executable=REFERENCE)
    image=wire.run_reference('compile',document('transition_plan',value['plan']),executable=REFERENCE)['payload']['value']
    driver=PipelineDriver(dut,DEPTHS,20261110)
    observed=[]
    try:
        await driver.initialize(value,image)
        for i,(row,expected) in enumerate(zip(value['steps'],trace['payload']['value']['results'])):
            actual=await driver.step(row);observed.append(actual)
            assert actual==expected,f'{value["name"]} quantum {i}:\n{json.dumps(actual)}\nexpected:\n{json.dumps(expected)}'
    except BaseException:
        wire.dump(source,OUTPUT/'failure.case.json');wire.dump(trace,OUTPUT/'failure.expected.trace.json')
        (OUTPUT/'failure.observed.json').write_text(json.dumps(observed,indent=2)+'\n')
        raise
    return len(observed)

@cocotb.test()
async def directed_against_rust(dut):
    summary={}
    replay=os.environ.get('LP_CASE')
    if replay:
        loaded=wire.load(replay)['payload'];value=loaded['value']['case'] if loaded['kind']=='pipeline_trace' else loaded['value']
        summary[value['name']]=await compare(dut,value)
    else:
        for value in directed(DEPTHS):summary[value['name']]=await compare(dut,value)
    (OUTPUT/'directed-summary.json').write_text(json.dumps(summary,indent=2)+'\n')

@cocotb.test()
async def seeded_against_rust(dut):
    summary={}
    for value in seeded(DEPTHS,int(os.environ['LP_RANDOM_CASES'])):summary[value['name']]=await compare(dut,value)
    (OUTPUT/'seeded-summary.json').write_text(json.dumps(summary,indent=2)+'\n')

@cocotb.test()
async def coordinator_protocol(dut):
    value=connect(case('protocol',[spec(0,32)],[step([command(0,1)])],requests=1),DEPTHS)
    image=wire.run_reference('compile',document('transition_plan',value['plan']),executable=REFERENCE)['payload']['value']
    driver=PipelineDriver(dut,DEPTHS)
    await driver.initialize(value,image)
    # Reset must abort a paused preflight and preserve signal RAM.
    await driver.observations([])
    await FallingEdge(dut.clk)
    driver.set(run_valid=1,run_quantum=16,run_instruction_count=len(value['program']['instructions']),run_state_count=1,
        run_job_budget=4,run_external_jobs=1,run_buffers=2,run_inputs=0,run_outputs=1)
    await driver.edge();driver.set(run_valid=0)
    await with_timeout(RisingEdge(dut.preflight_valid),1_000_000,'ns');await Timer(1,units='ns')
    assert int(dut.busy.value) and not int(dut.run_ready.value)
    await FallingEdge(dut.clk);driver.set(command_valid=1,command_opcode=10)
    await Timer(11,units='ns');assert not int(dut.command_ready.value)
    driver.set(command_valid=0)
    await driver.reset()
    assert not int(dut.busy.value) and not int(dut.preflight_valid.value)
    assert await driver.configure(0)==0
    assert (await driver.status())['configured'] is False
