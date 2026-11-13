"""Check workload execution, measurement boundaries and hardware run controls."""
import json
import os
from pathlib import Path
import cocotb
from cocotb.triggers import Timer
from options import create_memory, metadata


@cocotb.test()
async def deployed_runner(d):
    config = json.loads(Path(os.environ['BENCH_CONFIG']).read_text())
    p = config['parameters']
    blocks = p['SLOTS']//p['BLOCK_CHUNKS']
    pages = (blocks+14)//15+8*p['VIEWS']+16
    words = ((p['SLOTS']*p['CHUNK_FRAMES']+15)//16)*16+2*((blocks+15)//16)*16+pages*16+int(p.get("RETIRED_TABLES",256))*16
    initial = [0xa5a5a5a5]*words
    memory = create_memory(d, initial, seed=20261112, output=os.environ['BENCH_OUTPUT'])
    cycle = 0
    starts, completions = [], []

    def set(**values):
        for name, value in values.items():
            getattr(d, name).value = value

    async def tick():
        nonlocal cycle
        d.clk.value = 0
        if not memory.is_native:
            memory.drive()
        await Timer(5, units='ns')
        if int(d.run_pulse.value):
            starts.append(cycle)
        if int(d.done_pulse.value):
            completions.append(cycle)
        if not memory.is_native:
            memory.step(memory.sample(), bool(int(d.reset.value)))
        d.clk.value = 1
        await Timer(5, units='ns')
        d.clk.value = 0
        cycle += 1

    async def pulse(name):
        set(**{name: 1})
        await tick()
        set(**{name: 0})
        await tick()

    async def stopped():
        for _ in range(max(2000000, int(os.environ['BENCH_QUANTA'])*1000000)):
            await tick()
            status = int(d.status.value)
            if status & 7 == 5:
                await tick()
                await tick()
                return
        raise AssertionError(('runner did not stop', hex(int(d.status.value))))

    def stats():
        value = int(d.statistics.value)
        return [(value >> (64*i)) & ((1 << 64)-1) for i in range(16)]

    set(clk=0, reset=1, start=0, stop=0, snapshot=0, quanta_limit=int(os.environ['BENCH_QUANTA']),
        warmup_quanta=config['warmup_quanta'], gap_cycles=17, watchdog_cycles=1000000)
    try:
        await tick()
        set(reset=0)
        await tick()
        if memory.is_native:
            await memory.load_words(initial, tick)
        else:
            memory.stalls = True
        await pulse('start')
        await stopped()
        assert not int(d.status.value) & 16, hex(int(d.status.value))
        observed = [end-start for start, end in zip(starts, completions)]
        selected = observed[config['warmup_quanta']:]
        result = stats()
        assert result[1] == len(observed) == int(os.environ['BENCH_QUANTA'])
        assert result[2] == len(selected)
        assert result[3:7] == [sum(selected), selected[-1], min(selected), max(selected)]
        assert result[8] and result[9] and result[10] and result[11] and result[14]
        assert all(b-a >= 17 for a, b in zip(completions, starts[1:]))
        output = dict(memory=metadata(), cycles=observed, statistics=result,
                      hardware_counter_boundaries_match=True, output_correctness_checked=False)
        if os.environ['BENCH_CONTROLS'] == '1':
            set(quanta_limit=0, warmup_quanta=0, gap_cycles=0, watchdog_cycles=10000)
            memory.pause_responses = True
            await pulse('start')
            for _ in range(200000):
                await tick()
                if memory.pending is not None:
                    break
            else:
                raise AssertionError('no AXI transaction accepted before drain test')
            for _ in range(200000):
                await tick()
                if int(d.status.value) & 7 == 4:
                    break
            else:
                raise AssertionError('watchdog did not request drain')
            for _ in range(20):
                await tick()
                assert int(d.status.value) & 7 == 4, 'reset crossed an accepted AXI transaction'
            memory.pause_responses = False
            await stopped()
            assert int(d.status.value) & 16
            set(quanta_limit=0, watchdog_cycles=1000000)
            await pulse('start')
            for _ in range(200000):
                await tick()
                if int(d.debug_quantum.value) >= 3:
                    break
            else:
                raise AssertionError('restart did not execute work')
            await pulse('snapshot')
            assert stats()[1] >= 3
            await pulse('stop')
            await stopped()
            assert not int(d.status.value) & 16
            assert int(d.status.value) & 8
            await pulse('start')
            for _ in range(100):
                await tick()
                if int(d.status.value) & 7 == 2:
                    break
            else:
                raise AssertionError('runner did not enter loader')
            await pulse('stop')
            await stopped()
            assert not int(d.status.value) & 16
            set(quanta_limit=1)
            await pulse('start')
            await stopped()
            assert int(d.debug_quantum.value) == 1
            assert not int(d.status.value) & 16
            output.update(watchdog_drain_restart=True, snapshot_and_stop=True, stop_during_loading=True)
        (Path(os.environ['BENCH_OUTPUT'])/'result.json').write_text(json.dumps(output, indent=2)+'\n')
    finally:
        memory.close()
