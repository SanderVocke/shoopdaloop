"""Observe coordinator, processor and memory phases without changing execution."""
from collections import Counter
from pathlib import Path
import re
from pipeline_driver import sp

ROOT = Path(__file__).resolve().parents[3]


def enum_names(path, name):
    text = (ROOT/path).read_text()
    match = re.search(r'typedef\s+enum[^\{]*\{([^}]+)\}\s*'+name, text, re.S)
    return [item.strip() for item in match[1].split(',')]


class ExecutionTiming:
    def __init__(self, dut, memory):
        self.dut, self.memory = dut, memory
        self.core = dut.core.core if hasattr(dut, 'm_axi_awvalid') else dut.core
        self.jp = self.core.audio.pipeline.sample_engine
        self.cm = self.core.audio.media.sample_memory
        self.labels = enum_names('fpga/loop_pipeline/rtl/loop_pipeline.sv', 'phase_t')
        self.jp_labels = enum_names('fpga/job_processor/rtl/job_processor.sv', 'phase_t')
        self.tp_labels = enum_names('fpga/transition_processor/rtl/transition_processor.sv', 'state_t')
        self.enabled = False

    def begin(self):
        self.enabled = True
        self.start = self.total = None
        self.phases, self.jp_phases, self.tp_states = Counter(), Counter(), Counter()
        self.state_cycles = self.job_cycles = self.fill_wait = self.dma_cycles = 0
        self.jobs = []
        self.segment = None
        self.bus_segments = []

    def snapshot(self):
        timing = self.memory.axi.memory.timing if self.memory.axi is not None else None
        return timing.bus_snapshot() if hasattr(timing, 'bus_snapshot') else None

    def observe(self):
        if not self.enabled:
            return
        cycle = self.memory.cycles
        if self.start is None:
            if int(self.dut.run_valid.value) and int(self.dut.run_ready.value):
                self.start = cycle+1
            return
        time = cycle-self.start
        phase = self.labels[int(self.core.phase.value)]
        if self.segment is None:
            self.segment = (phase, self.snapshot())
        if phase != self.segment[0] or int(self.dut.done.value):
            self.bus_segments.append((self.segment[0], self.segment[1], self.snapshot()))
            self.segment = (phase, self.snapshot())
        if int(self.dut.done.value):
            self.total = time
            self.enabled = False
            return
        self.phases[phase] += 1
        if phase in ('PREVIEW_SEND', 'PREVIEW_WAIT', 'COMMIT_SEND', 'COMMIT_WAIT', 'FINISH_SEND', 'FINISH_WAIT'):
            self.tp_states[self.tp_labels[int(self.core.timing.state.value)]] += 1
        if int(self.dut.state_busy.value):
            self.state_cycles += 1
        if int(self.dut.jobs_busy.value):
            self.job_cycles += 1
            self.jp_phases[self.jp_labels[int(self.jp.phase.value)]] += 1
        if int(self.dut.monitor_valid.value) and int(self.dut.monitor_ready.value):
            if self.jobs:
                self.jobs[-1]['end'] = time
            self.jobs.append(dict(job=sp.decode_job(int(self.dut.monitor_data.value)), start=time))
        if int(self.dut.jobs_done.value) and self.jobs:
            self.jobs[-1].setdefault('end', time)
        self.fill_wait += int(int(self.cm.phase.value) == 3 and int(self.cm.opcode.value) == 0)
        self.dma_cycles += int(int(self.cm.dma_phase.value) != 0)

    def bus(self, include_preflight):
        timing = self.memory.axi.memory.timing if self.memory.axi is not None else None
        segments = [(a, b) for phase, a, b in self.bus_segments
                    if a is not None and (include_preflight or phase != 'PREFLIGHT')]
        if not segments:
            return None
        zero = {key: 0 for key in segments[0][0]}
        end = {key: sum(b[key]-a[key] for a,b in segments) for key in zero}
        zero['generation'] = end['generation'] = segments[0][0]['generation']
        return timing.bus_utilization(zero, end)

    def finish(self):
        assert self.total is not None and sum(self.phases.values()) == self.total
        grouped = {}
        for row in self.jobs:
            name = row['job']['kind']
            group = grouped.setdefault(name, dict(count=0, cycles=0))
            group['count'] += 1
            group['cycles'] += row['end']-row['start']
        tp_cycles = sum(value for name,value in self.phases.items() if name.startswith(('PREVIEW_', 'COMMIT_', 'FINISH_')))
        active = self.total-self.phases['PREFLIGHT']
        return dict(raw_cycles=self.total, preflight_wait_cycles=self.phases['PREFLIGHT'],
                    active_cycles=active, transition_cycles=tp_cycles, state_cycles=self.state_cycles,
                    job_cycles=self.job_cycles, other_cycles=active-tp_cycles-self.state_cycles-self.job_cycles,
                    coordinator_phases=dict(self.phases), job_phases=dict(self.jp_phases),
                    transition_states=dict(self.tp_states), jobs=grouped,
                    demand_fill_wait_cycles=self.fill_wait, transfer_active_cycles=self.dma_cycles,
                    raw_ddr_bus=self.bus(True), active_ddr_bus=self.bus(False))
