"""Profile the complete 64-loop pipeline while comparing its full trace with Rust."""
import json
from copy import deepcopy
import os
from pathlib import Path
import time
import cocotb
import fpga_testbench as wire
from pipeline_driver import PipelineDriver
from loop_memory import LoopMemory
from benchmark_cases import playback64, streaming64, mixed64, document
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
    verify=os.environ.get('LP_TIMING_VERIFY')=='1'
    workload=os.environ.get('LP_TIMING_WORKLOAD','original')
    value={'original':playback64,'streaming':streaming64,'mixed':mixed64}[workload](reference)
    block_chunks=int(os.environ.get('AXI_BLOCK_CHUNKS','1'))
    unblocked=value
    if block_chunks!=1:
        from block_images import reblock
        value=deepcopy(value)
        value['media']=reblock(value['media'],block_chunks,media[1])
    wire.dump(document('pipeline_case', value), output/'benchmark.case.json')
    expected = wire.run_reference('trace', document('pipeline_case', value), executable=reference)
    if block_chunks!=1:
        before=wire.run_reference('trace',document('pipeline_case',unblocked),executable=reference)
        for old,new in zip(before['payload']['value']['results'],expected['payload']['value']['results']):
            old=deepcopy(old);new=deepcopy(new)
            old['audio']['media']=new['audio']['media']=None
            assert old==new,'blocking changed benchmark audio, jobs, state or transition behavior'
        (output/'allocation-equivalence.json').write_text(json.dumps(dict(quanta=len(value['steps']),result='passed',block_chunks=block_chunks,physical_slots=media[1]))+'\n')
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
        initialization_cycles=memory.cycles
        preparation_started=memory.cycles
        await memory.fence()
        if hasattr(dut.core,'memory_endpoint') and hasattr(dut.core.memory_endpoint,'scheduler'):
            await memory.configure(5,0,1)
        cold_preparation_cycles=memory.cycles-preparation_started
        for index, (row, target) in enumerate(zip(value['steps'], expected['payload']['value']['results'])):
            probe.begin()
            actual = await driver.step(row,snapshot_media=verify)
            target=deepcopy(target)
            if not verify:target['audio']['media']=None
            if actual != target:
                (output/'failure.observed.json').write_text(json.dumps(actual, indent=2)+'\n')
                wire.dump(expected, output/'failure.expected.trace.json')
                raise AssertionError(f'coordinated benchmark quantum {index} differs from Rust')
            result = probe.finish()
            result['quantum'] = index
            assert sum(group['count'] for group in result['jobs'].values()) == len(target['audio']['jobs'])
            assert min(result['transition_cycles'], result['state_cycles'], result['job_cycles']) > 0
            results.append(result)
            dut._log.info('quantum %s: %s active cycles, TP=%s SP=%s JP=%s',
                          index, result['active_cycles'], result['transition_cycles'], result['state_cycles'],
                          result['job_cycles'])
        fence_started=memory.cycles
        await memory.fence()
        fence_cycles=memory.cycles-fence_started
        snapshot_started=memory.cycles
        final_media=await memory.snapshot()
        snapshot_cycles=memory.cycles-snapshot_started
        assert final_media==expected['payload']['value']['results'][-1]['audio']['media']
        cleanup=[]
        if workload=='mixed':
            operations=[dict(kind='reclaim',view=i) for i in range(64,72)]
            cleanup_expected=wire.run_reference('trace',document('media_case',dict(name='bounded-range-reclaim',media=final_media,steps=operations)),executable=reference)['payload']['value']['results']
            for operation,target in zip(operations,cleanup_expected):
                observed=await memory.command(operation,stalls=False)
                wire.assert_media_result_matches(target,observed)
                cleanup.append(dict(view=operation['view'],command_cycles=memory.timing))
        statistics = memory.axi.report(final=True) if memory.axi is not None else None
        (output/'execution-timing.json').write_text(json.dumps(dict(configuration=metadata(),
            verification_replay=verify,workload=workload,cleanup=cleanup,seed=20261110, random_stalls=False,
            loop_slots=64, request_slots=128, quantum_frames=32, chunk_frames=32,
            read_buffers=int(os.environ['LP_READ_BUFFERS']), write_buffers=int(os.environ['LP_WRITE_BUFFERS']),
            final_fence_cycles=fence_cycles,observation="full snapshots" if verify else "no intermediate media snapshots; cold metadata and sample caches",
            initialization_cycles=initialization_cycles,cold_preparation_cycles=cold_preparation_cycles,final_snapshot_cycles=snapshot_cycles,
            simulation_seconds=time.perf_counter()-started, results=results, memory_statistics=statistics), indent=2)+'\n')
    finally:
        memory.close()
