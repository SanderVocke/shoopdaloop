"""Independent frame scoreboard; the local consumer only copies RAM samples."""
import os
import cocotb
from cocotb.clock import Clock
from cocotb.triggers import FallingEdge, RisingEdge, Timer, with_timeout

Q = int(os.environ['AIO_Q'])
P = int(os.environ['AIO_P'])
CHANNELS = int(os.environ['AIO_CHANNELS'])
BANKS = int(os.environ['AIO_BANKS'])
PERIOD = int(os.environ['AIO_AUDIO_PERIOD'])
TICKS = max(100, CHANNELS*8)


def sample(channel, frame):
    return (channel+1)*100000+frame*37+1


def packed(frame):
    return sum(sample(c, frame) << (32*c) for c in range(CHANNELS))


async def edge(d, clock='clk'):
    await RisingEdge(getattr(d, clock))
    await Timer(1, units='ns')


async def initialize(d):
    for name in ('enable', 'frame_tick', 'frame_prepare', 'rx_samples', 'process_ready',
                 'process_done', 'process_fault', 'input_read', 'input_address',
                 'output_write', 'output_address', 'output_data', 'session_reset'):
        getattr(d, name).value = 0
    d.epoch.value = 7
    d.reset.value = 1
    await Timer(100, units='ns')
    d.reset.value = 0
    await Timer(100, units='ns')
    d.enable.value = 1
    await Timer(100, units='ns')


async def frames(d, count, missed=(), check=True):
    outputs = []
    for f in range(count):
        await FallingEdge(d.audio_clk)
        d.frame_prepare.value = 1
        await edge(d, 'audio_clk')
        await FallingEdge(d.audio_clk)
        d.frame_prepare.value = 0
        await edge(d, 'audio_clk')
        await FallingEdge(d.audio_clk)
        d.rx_samples.value = packed(f)
        d.frame_tick.value = 1
        await edge(d, 'audio_clk')
        got = int(d.tx_samples.value)
        outputs.append(got)
        source = f-Q-P
        expected = 0 if source < 0 or source//Q in missed else packed(source)
        if check:
            assert got == expected, (f, source, hex(got), hex(expected))
        await FallingEdge(d.audio_clk)
        d.frame_tick.value = 0
        for _ in range(TICKS-3):
            await RisingEdge(d.audio_clk)
    return outputs


async def consumer(d, blocks, late=False):
    for n in range(blocks):
        while not int(d.process_valid.value):
            await edge(d)
        assert int(d.process_sequence.value) == n
        d.process_ready.value = 1
        await edge(d)
        d.process_ready.value = 0
        for c in range(CHANNELS):
            for f in range(Q):
                d.input_address.value = c*Q+f
                d.input_read.value = 1
                await edge(d)
                value = int(d.input_data.value)
                assert value == sample(c, n*Q+f), (n, c, f, value)
                d.input_read.value = 0
                d.output_write.value = 1
                d.output_address.value = c*Q+f
                d.output_data.value = value
                await edge(d)
                d.output_write.value = 0
        if late and n == 0:
            while int(d.session_frame.value) < Q+P+1:
                await edge(d)
        d.process_done.value = 1
        await edge(d)
        d.process_done.value = 0


@cocotb.test()
async def nominal_and_recoverable_miss(d):
    cocotb.start_soon(Clock(d.clk, 10, units='ns').start())
    cocotb.start_soon(Clock(d.audio_clk, PERIOD, units='ns').start())
    await initialize(d)
    count = 8*Q+Q+P
    task = cocotb.start_soon(consumer(d, count//Q))
    await with_timeout(frames(d, count), count*TICKS*PERIOD*2, 'ns')
    assert not int(d.recovery_requested.value)
    assert int(d.missed_deadlines.value) == 0
    task.kill()
    if Q > 1 and BANKS >= 3:
        d.session_reset.value = 1
        d.enable.value = 0
        await Timer(100, units='ns')
        d.session_reset.value = 0
        d.epoch.value = 8
        await Timer(100, units='ns')
        d.enable.value = 1
        await Timer(100, units='ns')
        task = cocotb.start_soon(consumer(d, count//Q, late=True))
        await with_timeout(frames(d, count, missed=(0,)), count*TICKS*PERIOD*2, 'ns')
        assert not int(d.recovery_requested.value)
        assert int(d.missed_deadlines.value) == 1
        assert int(d.discarded_quanta.value) == 1
        task.kill()


@cocotb.test()
async def input_exhaustion_and_rearm(d):
    cocotb.start_soon(Clock(d.clk, 10, units='ns').start())
    cocotb.start_soon(Clock(d.audio_clk, PERIOD, units='ns').start())
    await initialize(d)
    await frames(d, (BANKS+1)*Q, check=False)
    assert int(d.recovery_requested.value)
    assert int(d.recovery_reason.value) == 1
    assert int(d.input_losses.value) == 1
    before = int(d.audio_frame.value)
    d.session_reset.value = 1
    d.enable.value = 0
    await frames(d, Q, check=False)
    d.session_reset.value = 0
    d.epoch.value = 11
    await Timer(100, units='ns')
    d.enable.value = 1
    await Timer(100, units='ns')
    count = 6*Q+P
    task = cocotb.start_soon(consumer(d, count//Q))
    await frames(d, count)
    assert not int(d.recovery_requested.value)
    assert int(d.audio_frame.value) > before
    task.kill()


@cocotb.test()
async def reset_in_flight_ownership(d):
    cocotb.start_soon(Clock(d.clk, 10, units='ns').start())
    cocotb.start_soon(Clock(d.audio_clk, PERIOD, units='ns').start())
    await initialize(d)
    for placement in sorted({1, Q, Q+1, Q+P, 2*Q+P}):
        task = cocotb.start_soon(consumer(d, 20))
        await frames(d, placement, check=False)
        task.kill()
        d.enable.value = 0
        d.session_reset.value = 1
        d.input_read.value = 0
        d.output_write.value = 0
        d.process_done.value = 0
        d.process_ready.value = 0
        await Timer(200, units='ns')
        d.epoch.value = int(d.epoch.value)+1
        d.session_reset.value = 0
        await Timer(100, units='ns')
        d.enable.value = 1
        await Timer(100, units='ns')
        task = cocotb.start_soon(consumer(d, 20))
        await frames(d, 4*Q+P)
        assert not int(d.recovery_requested.value)
        task.kill()
        d.enable.value = 0
        d.session_reset.value = 1
        await Timer(200, units='ns')
        d.session_reset.value = 0
        d.epoch.value = int(d.epoch.value)+1
        await Timer(100, units='ns')
        d.enable.value = 1
        await Timer(100, units='ns')
