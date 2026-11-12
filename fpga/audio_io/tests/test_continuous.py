"""Continuous clocks and frame deadlines, with production samples as the oracle."""
import json
import os
from pathlib import Path
import cocotb
from cocotb.clock import Clock
from cocotb.triggers import FallingEdge, RisingEdge, Edge, Timer, with_timeout
from driver import Driver
from pipeline_driver import Transport, sp
from loop_memory import LoopMemory
from media_results import logical_media
from cocotb.utils import get_sim_time

OUTPUT = Path(os.environ['AIO_OUTPUT'])
PERIOD = int(os.environ['AIO_AUDIO_PERIOD'])
TICKS = int(os.environ['AIO_FRAME_CYCLES'])
P = int(os.environ['AIO_P'])
FRAME_TIMES = {}


class DebugPorts:
    def __init__(self, dut):
        self.dut = dut

    def __getattr__(self, name):
        if name.startswith('memory_') and not hasattr(self.dut, name):
            return getattr(self.dut.core, name)
        return getattr(self.dut, name)


async def edge(d):
    await RisingEdge(d.clk)
    await Timer(1, units='ns')


async def configure(d, address, data):
    await FallingEdge(d.clk)
    assert int(d.cfg_ready.value)
    d.cfg_valid.value = 1
    d.cfg_state.value = int(address >= (1 << 31))
    d.cfg_address.value = address & 65535
    d.cfg_write.value = 1
    d.cfg_write_data.value = data
    await edge(d)
    d.cfg_valid.value = 0
    assert int(d.cfg_response_valid.value) and not int(d.cfg_response_error.value)
    await edge(d)


async def feed_controls(d, records):
    for value in records:
        await FallingEdge(d.clk)
        d.control_valid.value = 1
        d.control_data.value = value
        while True:
            await Timer(1, units='ns')
            ready = int(d.control_ready.value)
            await edge(d)
            if ready:
                break
        d.control_valid.value = 0


async def hold_first_quantum(d, q):
    d.job_enable.value = 0
    while int(d.session_frame.value) < q+P+1:
        await edge(d)
    d.job_enable.value = 1


async def drive_frames(d, expected, q, channels, count, missed=(), drive_input=True, start=0):
    actual = []
    for f in range(start, start+count):
        await FallingEdge(d.audio_clk)
        d.frame_prepare.value = 1
        await RisingEdge(d.audio_clk)
        await FallingEdge(d.audio_clk)
        d.frame_prepare.value = 0
        await RisingEdge(d.audio_clk)
        await FallingEdge(d.audio_clk)
        if drive_input:
            d.rx_samples.value = sum(((c+1)*100000+f*37+1) << (32*c) for c in range(channels))
        d.frame_tick.value = 1
        await RisingEdge(d.audio_clk)
        await Timer(1, units='ns')
        FRAME_TIMES[f] = float(get_sim_time(units="ns"))
        raw = int(d.tx_samples.value)
        source = f-q-P
        values = [(raw >> (32*c)) & 0xffffffff for c in range(channels)]
        values = [x-(1 << 32) if x & (1 << 31) else x for x in values]
        if source < 0 or source//q in missed:
            wanted = [0]*channels
        else:
            wanted = [expected['quanta'][source//q]['outputs']['channels'][c][source % q]
                      for c in range(channels)]
        assert values == wanted, (f, source//q, source % q, values, wanted,
                                  int(d.recovery_reason.value), int(d.error_code.value))
        actual.append(values)
        assert not int(d.recovery_requested.value), (f, int(d.recovery_reason.value), int(d.error_code.value))
        await FallingEdge(d.audio_clk)
        d.frame_tick.value = 0
        await Timer((TICKS-3)*PERIOD, units='ns')
    return actual


async def measure_publication(d, q):
    rows = []
    while True:
        await Edge(d.io.tx_request_sync)
        await Timer(1, units='ns')
        sequence = int(d.process_sequence.value)-1
        last_input = sequence*q+q-1
        if last_input in FRAME_TIMES:
            rows.append(dict(sequence=sequence, publication_ns=float(get_sim_time(units='ns')),
                             input_to_eligible_ns=float(get_sim_time(units='ns'))-FRAME_TIMES[last_input]))
            (OUTPUT/'publication.json').write_text(json.dumps(rows, indent=2)+'\n')


@cocotb.test()
async def continuous_record_playback(d):
    scenario = json.loads((OUTPUT/'scenario.json').read_text())
    image = json.loads((OUTPUT/'image.json').read_text())
    expected = json.loads((OUTPUT/'expected.json').read_text())
    q, channels = scenario['quantum'], scenario['input_ports']
    for name in ('enable', 'session_reset', 'quiesce', 'frame_tick', 'frame_prepare', 'rx_samples',
                 'control_valid', 'control_data', 'cfg_valid', 'cfg_state', 'cfg_write', 'cfg_address',
                 'cfg_write_data', 'audio_cfg_valid', 'audio_cfg_space', 'audio_cfg_write', 'audio_cfg_address',
                 'audio_cfg_write_data', 'drain_ready', 'store_cmd_valid', 'store_write_valid',
                 'store_read_ready', 'store_cfg_valid'):
        getattr(d, name).value = 0
    d.epoch.value = 1
    d.reset.value = 1
    d.job_enable.value = 1
    cocotb.start_soon(Clock(d.clk, 10, units='ns').start())
    cocotb.start_soon(Clock(d.audio_clk, PERIOD, units='ns').start())
    memory = LoopMemory(DebugPorts(d), (32, 256, scenario['chunk_frames']), 20261112)
    memory.stalls = os.environ['AIO_STALLS'] == '1'
    driver = Driver(Transport(d), (16, 128, 256), 20261112)
    case = dict(plan=dict(limits=image['transition']['limits']))
    try:
        await driver.load(case, image['transition'])
        for i, instruction in enumerate(image['program']['instructions']):
            await configure(d, i, sp.encode_instruction(instruction))
        for i, value in enumerate(image['initial_state']['memory']):
            await configure(d, i | (1 << 31), sp.encode_value(value))
        await memory.load({**image['media'], 'chunk_limit': 256})
        for name, value in dict(run_instruction_count=len(image['program']['instructions']),
                                run_state_count=len(image['initial_state']['memory']), run_quantum=q,
                                run_chunk_frames=scenario['chunk_frames'], run_job_budget=image['program']['jobs_per_quantum'],
                                run_buffers=image['buffers'], run_inputs=channels, run_outputs=channels).items():
            getattr(d, name).value = value
        records = [int(x, 16) for x in (OUTPUT/'events.hex').read_text().splitlines()]
        producer = cocotb.start_soon(feed_controls(d, records))
        measurement = cocotb.start_soon(measure_publication(d, q))
        d.enable.value = 1
        await Timer(5*PERIOD, units='ns')
        overrun = os.environ['AIO_OVERRUN'] == '1'
        if overrun:
            cocotb.start_soon(hold_first_quantum(d, q))
        actual = await with_timeout(drive_frames(d, expected, q, channels, 12*q+q+P,
                                                 missed=(0,) if overrun else ()), 100, 'ms')
        d.enable.value = 0
        while int(d.busy.value):
            await edge(d)
        await Timer(10*PERIOD, units='ns')
        producer.kill()
        measurement.kill()
        processed = int(d.processed_quanta.value)
        assert processed >= 12
        states = [{k: v for k, v in state.items() if k != 'revision_watermark'} for state in await driver.states()]
        assert states == expected['quanta'][processed-1]['states'], (processed, states, expected['quanta'][processed-1]['states'])
        snapshot = await memory.snapshot()
        media = logical_media(snapshot, image['loop_views'], states)
        assert media == expected['media']
        assert int(d.missed_deadlines.value) == int(overrun)
        assert int(d.discarded_quanta.value) == int(overrun)
        (OUTPUT/'actual-frames.json').write_text(json.dumps(actual)+'\n')
        (OUTPUT/'summary.json').write_text(json.dumps(dict(q=q, p=P, channels=channels, processed=processed,
             missed=int(d.missed_deadlines.value), discarded=int(d.discarded_quanta.value),
             rx_high_water=int(d.rx_high_water.value), tx_high_water=int(d.tx_high_water.value),
             compared_frames=len(actual), media_exact=True, states_exact=True), indent=2)+'\n')
    finally:
        memory.close()
