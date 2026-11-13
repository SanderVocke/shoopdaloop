"""Matched admission/processing measurements using either frozen or current driver."""
import json
import os
from pathlib import Path
import cocotb
import codec as abi
from driver import Driver
from cases import case, spec, command, step
import rt_backend_testbench as wire

@cocotb.test()
async def command_costs(dut):
    depths = tuple(map(int,os.environ['TP_DEPTHS'].split(',')))
    slots = depths[1]
    source = case('boundary-command-costs',[spec(0,64)],[step()],q=32,requests=slots)
    image = wire.run(dict(source,format_version=1),kind='transition_case',command='transition-compile',executable=os.environ['TP_REFERENCE'])
    driver = Driver(dut,depths)
    await driver.load(source,image)
    rows = []
    for sequence in range(1,9):
        error,value = await driver.command(abi.SUBMIT,data=abi.encode_command(command(0,sequence)))
        assert error == 0
        row = dict(install=driver.last_cycles)
        identity = abi.decode_identity(value)
        await driver.ok(abi.PROCESS);row['process']=driver.last_cycles
        await driver.ok(abi.READ_STATE,0);row['read_state']=driver.last_cycles
        await driver.ok(abi.STATUS);row['status']=driver.last_cycles
        collected = await driver.collect(identity);row['collect']=driver.last_cycles
        assert collected['result']['value']['outcome']['kind']=='applied'
        rows.append(row)
    identities = []
    dense_installs = []
    for sequence in range(100,100+slots):
        error,value = await driver.command(abi.SUBMIT,data=abi.encode_command(command(0,sequence,{'kind':'stop'})))
        assert error == 0
        identities.append(abi.decode_identity(value))
        dense_installs.append(driver.last_cycles)
    assert (await driver.command(abi.SUBMIT,data=abi.encode_command(command(0,99+slots))))[0]==4
    duplicate_cycles = driver.last_cycles
    assert (await driver.command(abi.SUBMIT,data=abi.encode_command(command(0,100+slots))))[0]==2
    full_cycles = driver.last_cycles
    await driver.ok(abi.PROCESS);dense_process = driver.last_cycles
    assert (await driver.command(abi.SUBMIT,data=abi.encode_command(command(0,101+slots))))[0]==2
    retained_full_cycles = driver.last_cycles
    await driver.collect(identities[0])
    error,value = await driver.command(abi.SUBMIT,data=abi.encode_command(command(0,101+slots)))
    assert error == 0 and abi.decode_identity(value)['slot']==0
    service = getattr(driver,'service',None)
    result = dict(depths=depths,repeated=rows,duplicate_cycles=duplicate_cycles,full_cycles=full_cycles,
        retained_full_cycles=retained_full_cycles,dense_process=dense_process,dense_installs=dense_installs,
        reused_install_cycles=driver.last_cycles,host_service_ns=service.timings if service else [],
        host_metadata_bytes=service.metadata_bytes if service else None,
        payload_bytes=dict(install=80,cancel=80,collect_response=104))
    Path(os.environ['TP_COMMAND_OUTPUT']).write_text(json.dumps(result,indent=2)+'\n')
