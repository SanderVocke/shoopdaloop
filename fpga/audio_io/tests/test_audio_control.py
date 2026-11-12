"""Check quantum fences, backpressure and malformed fabric commands."""
import cocotb
from cocotb.clock import Clock
from cocotb.triggers import FallingEdge, RisingEdge, Timer, with_timeout


def packet(sequence, kind, data=0, opcode=0, index=0):
    return sequence << 682 | kind << 680 | opcode << 672 | index << 640 | data


async def edge(d):
    await RisingEdge(d.clk)
    await Timer(1, units='ns')


async def send(d, records):
    for record in records:
        await FallingEdge(d.clk)
        d.control_valid.value = 1
        d.control_data.value = record
        while True:
            await Timer(1, units='ns')
            ready = int(d.control_ready.value)
            await edge(d)
            if ready:
                break
        d.control_valid.value = 0


@cocotb.test()
async def fences_and_failures(d):
    for name in ('enable', 'sequence_number', 'busy', 'run_start', 'control_valid', 'control_data',
                 'command_ready', 'response_valid', 'response_error', 'cfg_ready', 'cfg_response_valid', 'cfg_response_error'):
        getattr(d, name).value = 0
    d.reset.value = 1
    cocotb.start_soon(Clock(d.clk, 10, units='ns').start())
    await edge(d)
    d.reset.value = 0
    records = [packet(0, 0, 123, opcode=4), packet(0, 2),
               packet(1, 1, 456, index=(1 << 31) | 7), packet(1, 2), packet(3, 0, 789)]
    producer = cocotb.start_soon(send(d, records))
    await Timer(150, units='ns')
    assert not int(d.control_ready.value), 'bounded FIFO did not backpressure'
    assert not int(d.command_valid.value)
    d.enable.value = 1
    while not int(d.command_valid.value):
        await edge(d)
    for _ in range(3):
        assert int(d.command_data.value) == 123
        assert int(d.command_opcode.value) == 4
        await edge(d)
    d.command_ready.value = 1
    await edge(d)
    d.command_ready.value = 0
    assert not int(d.fenced.value)
    d.response_valid.value = 1
    await edge(d)
    d.response_valid.value = 0
    while not int(d.fenced.value):
        await edge(d)
    for _ in range(5):
        await edge(d)
        assert not int(d.cfg_valid.value), 'future control applied before its quantum'
    d.run_start.value = 1
    d.busy.value = 1
    await edge(d)
    d.run_start.value = 0
    d.sequence_number.value = 1
    for _ in range(5):
        await edge(d)
        assert not int(d.cfg_valid.value), 'control changed state during processing'
    d.busy.value = 0
    while not int(d.cfg_valid.value):
        await edge(d)
    assert int(d.cfg_state.value) == 1 and int(d.cfg_address.value) == 7
    assert int(d.cfg_write_data.value) == 456
    d.cfg_ready.value = 1
    await edge(d)
    d.cfg_ready.value = 0
    d.cfg_response_valid.value = 1
    await edge(d)
    d.cfg_response_valid.value = 0
    while not int(d.fenced.value):
        await edge(d)
    d.run_start.value = 1
    await edge(d)
    d.run_start.value = 0
    d.sequence_number.value = 2
    for _ in range(10):
        await edge(d)
        assert not int(d.command_valid.value), 'wrong-sequence command was dispatched'
    assert int(d.fault.value)
    await producer
