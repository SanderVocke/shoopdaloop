"""The prepared fabric loader runs without host participation between quanta."""
import json
import os
from pathlib import Path
import cocotb
from cocotb.clock import Clock
from cocotb.triggers import FallingEdge, RisingEdge, Timer, with_timeout
from options import create_memory
from test_continuous import drive_frames, edge, PERIOD, TICKS, P

OUTPUT = Path(os.environ['AIO_OUTPUT'])


async def service(d, memory):
    while True:
        await FallingEdge(d.clk)
        memory.drive()
        await Timer(1, units='ns')
        packet = memory.sample()
        resetting = bool(int(d.reset.value))
        await RisingEdge(d.clk)
        memory.step(packet, resetting)


async def pulse(d):
    await FallingEdge(d.audio_clk)
    d.frame_prepare.value = 1
    await RisingEdge(d.audio_clk)
    await FallingEdge(d.audio_clk)
    d.frame_prepare.value = 0
    await RisingEdge(d.audio_clk)
    await FallingEdge(d.audio_clk)
    d.frame_tick.value = 1
    await RisingEdge(d.audio_clk)
    await FallingEdge(d.audio_clk)
    d.frame_tick.value = 0
    await Timer((TICKS-3)*PERIOD, units='ns')


async def recover(d, minimum_epoch):
    for _ in range(100):
        await pulse(d)
        if int(d.epoch.value) >= minimum_epoch and int(d.boot_done.value):
            await Timer(5*PERIOD, units='ns')
            return int(d.session_frame.value)
    raise AssertionError(('recovery did not finish', int(d.epoch.value), int(d.boot_fault.value),
                          int(d.memory_quiescent.value), int(d.recovery_reason.value)))


@cocotb.test()
async def prepared_demo_and_recovery(d):
    config = json.loads((OUTPUT/'config.json').read_text())
    expected = json.loads((OUTPUT/'expected.json').read_text())
    q, channels = config['q'], config['channels']
    d.reset.value = 1
    d.restart.value = 0
    d.frame_tick.value = 0
    d.frame_prepare.value = 0
    d.job_enable.value = 1
    cocotb.start_soon(Clock(d.clk, 10, units='ns').start())
    cocotb.start_soon(Clock(d.audio_clk, PERIOD, units='ns').start())
    chunk = json.loads((OUTPUT/'scenario.json').read_text())['chunk_frames']
    pages = (256+14)//15+8*32+16
    words = ((256*chunk+15)//16)*16+2*((256+15)//16)*16+pages*16
    memory = create_memory(d, [0]*words, seed=20261112, output=OUTPUT)
    task = None
    if not getattr(memory, 'is_native', False):
        memory.stalls = os.environ['AIO_STALLS'] == '1'
        task = cocotb.start_soon(service(d, memory))
    try:
        await Timer(200, units='ns')
        d.reset.value = 0
        for _ in range(100000):
            if int(d.boot_done.value):
                break
            assert not int(d.boot_fault.value)
            await edge(d)
        else:
            raise AssertionError('prepared image loading timed out')
        await Timer(5*PERIOD, units='ns')
        count = 12*q+q+P
        await drive_frames(d, expected, q, channels, count, drive_input=False)
        assert int(d.recoveries.value) == 0
        before = int(d.audio_frame.value)
        # A hung executor fills the bounded input ring; the demo quiesces and reloads.
        d.job_enable.value = 0
        for _ in range(10*q):
            await pulse(d)
            if int(d.recoveries.value):
                break
        assert int(d.recoveries.value) == 1
        assert int(d.last_recovery_reason.value) == 1, int(d.last_recovery_reason.value)
        d.job_enable.value = 1
        start = await recover(d, 2)
        await drive_frames(d, expected, q, channels, count-start, drive_input=False, start=start)
        assert int(d.audio_frame.value) > before
        assert int(d.recoveries.value) == 1
        # An injected memory failure in a fresh epoch must also drain safely.
        d.restart.value = 1
        await edge(d)
        d.restart.value = 0
        memory.fail_next = True
        for _ in range(10*q):
            await pulse(d)
            if int(d.recoveries.value) >= 3:
                break
        assert int(d.recoveries.value) == 3
        assert int(d.last_recovery_reason.value) == 3
        assert int(d.last_error_stage.value) == 3
        start = await recover(d, 4)
        await drive_frames(d, expected, q, channels, count-start, drive_input=False, start=start)
        assert int(d.recoveries.value) == 3
        # Withhold an accepted memory response: reset must wait for its retirement.
        memory.pause_responses = True
        for _ in range(4*q):
            await pulse(d)
            if memory.pending is not None:
                break
        assert memory.pending is not None
        assert not int(d.memory_quiescent.value)
        d.restart.value = 1
        await edge(d)
        d.restart.value = 0
        for _ in range(2):
            await pulse(d)
            assert int(d.epoch.value) == 4, 'reset crossed an unretired response'
            assert int(d.tx_samples.value) == 0
        memory.pause_responses = False
        start = await recover(d, 5)
        await drive_frames(d, expected, q, channels, count-start, drive_input=False, start=start)
        assert int(d.recoveries.value) == 4
        (OUTPUT/'summary.json').write_text(json.dumps(dict(q=q, p=P, channels=channels,
            generated_input=True, prepared_loader=True, recoveries=int(d.recoveries.value),
            epoch=int(d.epoch.value), input_exhaustion_recovered=True, memory_fault_recovered=True, pending_response_drained=True), indent=2)+'\n')
    finally:
        if task:
            task.kill()
        memory.close()
