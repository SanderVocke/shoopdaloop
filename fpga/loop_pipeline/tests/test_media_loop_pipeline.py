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


async def compare(dut, value, prepare=None, initial_override=None):
    source = document('pipeline_case', value)
    trace = wire.run_reference('trace', source, executable=REFERENCE)
    wire.run_reference('check', trace, executable=REFERENCE)
    image = wire.run_reference('compile', document('transition_plan', value['plan']), executable=REFERENCE)['payload']['value']
    driver = PipelineDriver(dut, DEPTHS, 20261111)
    memory = LoopMemory(dut, MEDIA, 20261111)
    memory.initial_override = initial_override
    driver.media = memory
    driver.preflight_hook = prepare
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
    assert any(row['reads'] for row in summary.values())
    if not replay:assert any(row['writes'] for row in summary.values())
    (OUTPUT/'media-directed-summary.json').write_text(json.dumps(summary, indent=2)+'\n')


@cocotb.test()
async def seeded_against_rust(dut):
    summary = {}
    for value in seeded(DEPTHS, MEDIA, int(os.environ['LP_RANDOM_CASES'])):
        summary[value['name']] = await compare(dut, value)
    (OUTPUT/'media-seeded-summary.json').write_text(json.dumps(summary, indent=2)+'\n')


@cocotb.test()
async def resource_preparation_and_faults(dut):
    value = audio(case('create-recording-during-preflight', [spec(0, 0)], [
        step([command(0, 1, dict(kind='record'))]), step([command(0, 2)]), step(),
    ], q=1, requests=2), DEPTHS, MEDIA, views=1)
    prepared = False

    async def create(driver, attempt, decision):
        nonlocal prepared
        if prepared:return
        before = await driver.states()
        result = await driver.media.command(dict(kind='create'))
        assert result['status']['kind'] == 'completed' and result['value']['value'] == 0
        await driver.configure(1, 0)
        assert await driver.states() == before
        assert int(dut.debug_frame.value) == 0
        prepared = True

    result = await compare(dut, value, create, empty(MEDIA, 0))
    assert result['writes'], 'recorded data did not reach DDR at the snapshot fence'

    # Exhausted view metadata must be reported before committing the recording.
    value = audio(case('preflight-view-refusal', [spec(0, 0)], [step(
        [command(0, 1, dict(kind='record'))], previews=[[], [dict(kind='capture_unavailable', target=spec(0, 0)['id'], reason='capacity')]])
    ], q=1, requests=1), DEPTHS, MEDIA, views=MEDIA[0])
    async def refuse(driver, attempt, decision):
        if attempt==0:
            result=await driver.media.command(dict(kind='create'))
            assert result['status']==dict(kind='failed',value=dict(error='memory_full'))
    result=await compare(dut,value,refuse)
    assert result['writes']==0

    # A host read may remain backpressured during preflight. Even an eager
    # acknowledgement must wait until preparation releases the media store.
    value = audio(case('preflight-waits-for-media', [spec(0, 1)], [
        step([command(0, 1)]), step(),
    ], q=1, requests=1), DEPTHS, MEDIA)
    value['media'] = recorded(MEDIA, [dict(kind='create'),
        dict(kind='append', view=0, samples=[123456])], REFERENCE)
    prepared = False

    async def stalled_read(driver, attempt, decision):
        nonlocal prepared
        if prepared:return
        token = int(dut.boundary_token.value)
        bank = int(dut.debug_bank.value)
        driver.set(store_cmd_valid=1, store_cmd_opcode=6, store_cmd_view=0,
                   store_cmd_start=0, store_cmd_length=1, store_cmd_count=1)
        before, _ = await driver.media.tick()
        assert before['cmd_ready']
        driver.set(store_cmd_valid=0, preflight_ready=1)
        await with_timeout(RisingEdge(dut.store_read_valid), 1_000_000, 'ns')
        for _ in range(8):
            await driver.edge()
            assert int(dut.store_read_valid.value) and int(dut.store_read_data.value) == 123456
            assert not int(dut.preflight_valid.value) and not int(dut.state_busy.value)
            assert int(dut.boundary_token.value) == token and int(dut.debug_bank.value) == bank
            assert int(dut.debug_frame.value) == 0
        driver.set(preflight_ready=0, store_read_ready=1)
        for _ in range(100):
            _, after = await driver.media.tick()
            if after['done']:break
        else:raise AssertionError('host read did not complete')
        driver.set(store_read_ready=0)
        await driver.edge()
        prepared = True

    await compare(dut, value, stalled_read)

    # Physical bus errors have no Rust equivalent: assert fault containment,
    # inspectability and reset recovery separately from sample equivalence.
    value = audio(case('physical-memory-fault-and-reset', [spec(0, 1)], [
        step([command(0, 1)]), step([command(0, 2, dict(kind='record'))]),
    ], q=1, requests=1), DEPTHS, MEDIA)
    value['media'] = recorded(MEDIA, [dict(kind='create'), dict(kind='append', view=0, samples=[123456])], REFERENCE)
    trace = wire.run_reference('trace', document('pipeline_case', value), executable=REFERENCE)['payload']['value']
    image = wire.run_reference('compile', document('transition_plan', value['plan']), executable=REFERENCE)['payload']['value']
    driver, memory = PipelineDriver(dut, DEPTHS), LoopMemory(dut, MEDIA, 17)
    driver.media = memory
    probing = False
    probes = 0

    def probe_host_ports():
        nonlocal probing, probes
        if int(dut.jobs_busy.value):
            # These would corrupt the prepared take if accepted.
            driver.set(store_cmd_valid=1, store_cmd_opcode=2, store_cmd_view=0,
                       store_cfg_valid=1, store_cfg_write=1, store_cfg_space=0,
                       store_cfg_address=0, store_cfg_write_data=0,
                       cfg_valid=1, cfg_write=1, cfg_state=1, cfg_address=1, cfg_write_data=0xffffffff,
                       audio_cfg_valid=1, audio_cfg_write=1, audio_cfg_space=0,
                       audio_cfg_address=0, audio_cfg_write_data=0,
                       command_valid=1, command_opcode=8)
            assert not any(int(getattr(dut, name).value) for name in
                ('store_cmd_ready', 'store_cfg_ready', 'cfg_ready', 'audio_cfg_ready', 'command_ready'))
            probing = True
            probes += 1
        elif probing:
            driver.set(store_cmd_valid=0, store_cfg_valid=0, cfg_valid=0,
                       audio_cfg_valid=0, command_valid=0)
            probing = False

    try:
        await driver.initialize(value, image)
        memory.before_clock = probe_host_ports
        memory.fail_next = True
        result = await driver.step(value['steps'][0])
        assert result['error'] == dict(stage='jobs', error='memory_fault')
        assert result['transition']['next_frame'] == 0
        assert not int(dut.run_ready.value) and not int(dut.store_cmd_ready.value)
        assert await memory.configure(0, 0) == 1
        assert await memory.configure(1, 0) & 1
        assert probes > 0

        await driver.initialize(value, image)
        for row, expected in zip(value['steps'], trace['results']):
            assert await driver.step(row) == expected

        # Reset also flushes an accepted request whose response is still pending.
        await driver.initialize(value, image)
        memory.pause_responses = True
        running = cocotb.start_soon(driver.step(value['steps'][0]))
        for _ in range(10000):
            await driver.edge()
            if memory.pending is not None:break
        else:raise AssertionError('no sample request reached memory')
        for _ in range(8):
            await driver.edge()
            assert int(dut.debug_frame.value) == 0 and not int(dut.done.value)
        running.kill()
        await driver.reset()
        assert memory.pending is None and not int(dut.memory_valid.value)
        probing = False
        memory.pause_responses = False
        await driver.initialize(value, image)
        for row, expected in zip(value['steps'], trace['results']):
            assert await driver.step(row) == expected

        # A buffered recording can finish its logical quantum before DDR fails.
        posted = audio(case('late-writeback-fault', [spec(0, 0)], [
            step([command(0, 1, dict(kind='record'))]),
        ], q=1, requests=1), DEPTHS, MEDIA)
        posted_image = wire.run_reference('compile', document('transition_plan', posted['plan']), executable=REFERENCE)['payload']['value']
        await driver.initialize(posted, posted_image)
        memory.pause_responses = True
        memory.fail_next = True
        running = cocotb.start_soon(driver.step(posted['steps'][0]))
        can_post = int(os.environ['LP_WRITE_BUFFERS']) >= posted['program']['quantum']//MEDIA[2]
        expected_frame = posted['program']['quantum'] if can_post else 0
        try:
            for _ in range(20000):
                await driver.edge()
                if (int(dut.debug_frame.value) == expected_frame and memory.pending is not None
                        and int(dut.store_busy.value) and (can_post or int(dut.jobs_busy.value))):
                    break
            else:
                raise AssertionError('recording did not reach posted completion or write-pool pressure')
            if not can_post:
                for _ in range(30):
                    await driver.edge()
                    assert int(dut.debug_frame.value) == 0 and not int(dut.done.value)
            assert not int(dut.memory_fault.value) and memory.pending[2]
        finally:
            running.kill()
        memory.pause_responses = False
        for _ in range(10000):
            await driver.edge()
            if int(dut.memory_fault.value) and int(dut.error_code.value) == 8:
                break
        else:
            raise AssertionError('late writeback failure was not propagated')
        assert int(dut.error_stage.value) == 3 and int(dut.debug_frame.value) == expected_frame
        assert not int(dut.run_ready.value) and not int(dut.store_cmd_ready.value)
        await driver.initialize(value, image)
        for row, expected in zip(value['steps'], trace['results']):
            assert await driver.step(row) == expected
    finally:
        memory.close()
