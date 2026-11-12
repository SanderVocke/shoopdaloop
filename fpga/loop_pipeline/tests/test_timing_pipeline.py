"""Profile the complete 64-loop pipeline while comparing its full trace with Rust."""
import json
import os
from pathlib import Path
import time
import cocotb
import fpga_testbench as wire
from pipeline_driver import PipelineDriver
from loop_memory import LoopMemory
from benchmark_cases import playback64, document
from execution_timing import ExecutionTiming
from options import metadata


class BenchmarkDriver(PipelineDriver):
    async def command(self, *args, **kwargs):
        kwargs.setdefault('stall', 0)
        return await super().command(*args, **kwargs)


@cocotb.test()
async def benchmark64(dut):
    output = Path(os.environ['LP_OUTPUT'])
    reference = Path(os.environ['LP_REFERENCE'])
    depths = tuple(map(int, os.environ['LP_DEPTHS'].split(',')))
    media = tuple(map(int, os.environ['LP_MEDIA_CAPACITIES'].split(',')))
    value = playback64(reference)
    wire.dump(document('pipeline_case', value), output/'benchmark.case.json')
    expected = wire.run_reference('trace', document('pipeline_case', value), executable=reference)
    wire.run_reference('check', expected, executable=reference)
    image = wire.run_reference('compile', document('transition_plan', value['plan']), executable=reference)['payload']['value']
    driver = BenchmarkDriver(dut, depths, 20261110)
    driver.stalls = False
    memory = LoopMemory(dut, media, 20261110)
    memory.stalls = False
    driver.media = memory
    probe = ExecutionTiming(dut, memory)
    memory.before_clock = probe.observe
    results = []
    started = time.perf_counter()
    try:
        await driver.initialize(value, image)
        for index, (row, target) in enumerate(zip(value['steps'], expected['payload']['value']['results'])):
            reads, writes = memory.read_requests, memory.write_requests
            probe.begin()
            actual = await driver.step(row)
            if actual != target:
                (output/'failure.observed.json').write_text(json.dumps(actual, indent=2)+'\n')
                wire.dump(expected, output/'failure.expected.trace.json')
                raise AssertionError(f'coordinated benchmark quantum {index} differs from Rust')
            result = probe.finish()
            result['quantum'] = index
            result['axi_reads'] = memory.read_requests-reads
            result['axi_writes'] = memory.write_requests-writes
            assert sum(group['count'] for group in result['jobs'].values()) == 192
            assert min(result['transition_cycles'], result['state_cycles'], result['job_cycles']) > 0
            results.append(result)
            dut._log.info('quantum %s: %s active cycles, TP=%s SP=%s JP=%s',
                          index, result['active_cycles'], result['transition_cycles'], result['state_cycles'],
                          result['job_cycles'])
        statistics = memory.axi.report(final=True) if memory.axi is not None else None
        (output/'execution-timing.json').write_text(json.dumps(dict(configuration=metadata(),
            seed=20261110, random_stalls=False,
            loop_slots=64, request_slots=128, quantum_frames=32, chunk_frames=32,
            read_buffers=int(os.environ['LP_READ_BUFFERS']), write_buffers=int(os.environ['LP_WRITE_BUFFERS']),
            simulation_seconds=time.perf_counter()-started, results=results, memory_statistics=statistics), indent=2)+'\n')
    finally:
        memory.close()
