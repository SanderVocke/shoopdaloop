//! Prepared opaque processor owners and allocation-free applied-state observation.
use alloc::{boxed::Box, sync::Arc, vec::Vec};
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use rt_buffered::{AsyncStats, Backend, BufferLimits, BufferedProcessor, Lifecycle, Timing};
use rt_runtime::{graph::PortCounts, processor::ProcessBlock, Error, Frame, Quantum};
use shoop_loops::runtime::{LoopPlanner, WorkerCaptureRelease, WorkerCaptureSource};
use shoop_rt_protocol::{
    processors::{Effective, Unavailable, WorkerDiagnostics, STATE_SCALARS},
    ProcessorKind,
};

#[cfg(test)]
mod capture_tests;

pub struct PreparedProcessor {
    pub kind: ProcessorKind,
    pub backend: Box<dyn Backend>,
    pub timing: Timing,
}
impl PreparedProcessor {
    pub const INLINE_TIMING: Timing = Timing {
        batch_quanta: 1,
        slack_quanta: 0,
    };
}
pub use shoop_rt_protocol::ProcessorPreset;
pub struct PreparedProcessors {
    pub effective: Effective,
    pub reserved_bytes: u64,
    pub instances: Vec<PreparedProcessor>,
    pub presets: Box<[ProcessorPreset]>,
}
impl Default for PreparedProcessors {
    fn default() -> Self {
        Self {
            effective: Effective {
                fx_instances: 0,
                synth_instances: 0,
                fx_unavailable: Some(Unavailable::Disabled),
                synth_unavailable: Some(Unavailable::Disabled),
                memory_bytes: 0,
                carla: Default::default(),
            },
            reserved_bytes: 0,
            instances: Vec::new(),
            presets: Vec::new().into_boxed_slice(),
        }
    }
}
#[derive(Clone, Debug)]
pub struct ProcessorCatalog {
    pub effective: Effective,
    pub reserved_bytes: u64,
    pub kinds: Box<[ProcessorKind]>,
    pub presets: Box<[ProcessorPreset]>,
    pub(crate) worker_delays: Box<[usize]>,
    pub(crate) carla_delay: u32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProcessorState {
    pub global_midi: shoop_rt_protocol::processors::GlobalMidiDiagnostics,
    pub kind: ProcessorKind,
    pub frame: Frame,
    pub generation: u32,
    pub phase: ProcessorPhase,
    pub worker: Option<WorkerDiagnostics>,
    pub values: [f32; STATE_SCALARS],
}

pub use shoop_rt_protocol::processors::Phase as ProcessorPhase;

pub(crate) struct ProcessorCells {
    global_counts: [AtomicU64; 4],
    global_pending: AtomicU32,
    global_latest: AtomicU32,
    has_worker: AtomicBool,
    worker_depths: [AtomicU32; 2],
    worker_counts: [AtomicU64; 7],
    kind: ProcessorKind,
    revision: AtomicU64,
    frame: AtomicU64,
    generation: AtomicU32,
    phase: AtomicU32,
    values: [AtomicU32; STATE_SCALARS],
}
impl ProcessorCells {
    pub(crate) fn new(kind: ProcessorKind) -> Self {
        Self {
            global_counts: core::array::from_fn(|_| AtomicU64::new(0)),
            global_pending: AtomicU32::new(0),
            global_latest: AtomicU32::new(0),
            has_worker: AtomicBool::new(false),
            worker_depths: core::array::from_fn(|_| AtomicU32::new(0)),
            worker_counts: core::array::from_fn(|_| AtomicU64::new(0)),
            kind,
            revision: AtomicU64::new(0),
            frame: AtomicU64::new(0),
            generation: AtomicU32::new(0),
            phase: AtomicU32::new(ProcessorPhase::Free as u32),
            values: core::array::from_fn(|_| AtomicU32::new(0)),
        }
    }
    fn publish(
        &self,
        backend: &dyn Backend,
        frame: Frame,
        generation: u32,
        phase: ProcessorPhase,
        global: shoop_rt_protocol::processors::GlobalMidiDiagnostics,
        worker: Option<AsyncStats>,
    ) {
        self.revision.fetch_add(1, Ordering::SeqCst);
        for (cell, value) in self.global_counts.iter().zip([
            global.rejected,
            global.overwritten,
            global.deferred,
            global.drained,
        ]) {
            cell.store(value, Ordering::SeqCst);
        }
        self.global_latest.store(
            global
                .latest_cc
                .map_or(0, |data| u32::from_le_bytes([data[0], data[1], data[2], 1])),
            Ordering::SeqCst,
        );
        self.global_pending
            .store(u32::from(global.pending), Ordering::SeqCst);
        if let Some(worker) = worker {
            for (cell, value) in self.worker_counts.iter().zip([
                worker.missed,
                worker.stale,
                worker.rejected,
                worker.failed,
                worker.invalid,
                worker.last_completion_frames,
                worker.peak_completion_frames,
            ]) {
                cell.store(value, Ordering::SeqCst);
            }
            self.worker_depths[0].store(worker.pending as u32, Ordering::SeqCst);
            self.worker_depths[1].store(worker.peak_pending as u32, Ordering::SeqCst);
            self.has_worker.store(true, Ordering::SeqCst);
        }
        for index in 0..self.kind.scalar_count() {
            if let Some(value) = self
                .kind
                .scalar_id(index)
                .and_then(|id| backend.parameter(id))
            {
                self.values[index].store(value.to_bits(), Ordering::SeqCst);
            }
        }
        self.frame.store(frame.0, Ordering::SeqCst);
        self.generation.store(generation, Ordering::SeqCst);
        self.phase.store(phase as u32, Ordering::SeqCst);
        self.revision.fetch_add(1, Ordering::SeqCst);
    }
    pub(crate) fn read(&self) -> Option<ProcessorState> {
        let before = self.revision.load(Ordering::SeqCst);
        if before % 2 != 0 {
            return None;
        }
        let values =
            core::array::from_fn(|index| f32::from_bits(self.values[index].load(Ordering::SeqCst)));
        let frame = Frame(self.frame.load(Ordering::SeqCst));
        let generation = self.generation.load(Ordering::SeqCst);
        let latest = self.global_latest.load(Ordering::SeqCst).to_le_bytes();
        let global_midi = shoop_rt_protocol::processors::GlobalMidiDiagnostics {
            latest_cc: (latest[3] == 1).then_some([latest[0], latest[1], latest[2]]),
            rejected: self.global_counts[0].load(Ordering::SeqCst),
            overwritten: self.global_counts[1].load(Ordering::SeqCst),
            deferred: self.global_counts[2].load(Ordering::SeqCst),
            drained: self.global_counts[3].load(Ordering::SeqCst),
            pending: self.global_pending.load(Ordering::SeqCst) as u16,
        };
        let worker = self
            .has_worker
            .load(Ordering::SeqCst)
            .then(|| WorkerDiagnostics {
                pending: self.worker_depths[0].load(Ordering::SeqCst),
                peak_pending: self.worker_depths[1].load(Ordering::SeqCst),
                missed: self.worker_counts[0].load(Ordering::SeqCst),
                stale: self.worker_counts[1].load(Ordering::SeqCst),
                rejected: self.worker_counts[2].load(Ordering::SeqCst),
                failed: self.worker_counts[3].load(Ordering::SeqCst),
                invalid: self.worker_counts[4].load(Ordering::SeqCst),
                last_completion_frames: self.worker_counts[5].load(Ordering::SeqCst),
                peak_completion_frames: self.worker_counts[6].load(Ordering::SeqCst),
            });
        let phase = match self.phase.load(Ordering::SeqCst) {
            0 => ProcessorPhase::Free,
            1 => ProcessorPhase::Owned,
            2 => ProcessorPhase::Running,
            3 => ProcessorPhase::Retiring,
            _ => ProcessorPhase::Faulted,
        };
        if before != self.revision.load(Ordering::SeqCst) {
            return None;
        }
        Some(ProcessorState {
            global_midi,
            kind: self.kind,
            frame,
            generation,
            phase,
            worker,
            values,
        })
    }
}

#[derive(Clone, Copy)]
pub(crate) enum ProcessorTarget {
    Track { source: usize, generation: u32 },
    Bus(shoop_rt_protocol::BusId),
}
pub struct HostedProcessor {
    buffered: BufferedProcessor<Box<dyn Backend>>,
    cells: Arc<ProcessorCells>,
    generation: u32,
    phase: ProcessorPhase,
    cleanup: usize,
    slot: usize,
    slots: usize,
    quantum: usize,
    signal: Option<Arc<crate::cells::Cells>>,
    binding: Option<ProcessorTarget>,
    pending_binding: Option<(rt_runtime::Epoch, ProcessorTarget)>,
    parameters: [(Frame, u32, f32); 64],
    parameter_count: usize,
    manual_active: Option<(u32, bool)>,
    global: crate::global_midi::GlobalMidi,
    midi_capacity: rt_runtime::buffer::MidiCapacity,
    global_capacity: rt_runtime::buffer::MidiCapacity,
    midi_output_capacity: rt_runtime::buffer::MidiCapacity,
}
impl HostedProcessor {
    pub fn additional_prepared_bytes(
        limits: BufferLimits,
        global: rt_runtime::buffer::MidiCapacity,
    ) -> Result<usize, Error> {
        crate::global_midi::GlobalMidi::storage_bytes(
            rt_runtime::buffer::MidiCapacity {
                events: limits.midi_input.events_per_quantum,
                bytes: limits.midi_input.bytes_per_quantum,
            },
            global,
        )
    }
    pub(crate) fn new(
        processor: PreparedProcessor,
        cells: Arc<ProcessorCells>,
        quantum: Quantum,
        limits: BufferLimits,
        global_capacity: rt_runtime::buffer::MidiCapacity,
        slot: usize,
        slots: usize,
    ) -> Result<Self, Error> {
        if processor.backend.ports().midi_inputs > 1 {
            return Err(Error::InvalidShape);
        }
        let global = crate::global_midi::GlobalMidi::new(
            rt_runtime::buffer::MidiCapacity {
                events: limits.midi_input.events_per_quantum,
                bytes: limits.midi_input.bytes_per_quantum,
            },
            global_capacity,
        )?;
        let mut buffered =
            BufferedProcessor::with_backend(processor.backend, quantum, processor.timing, limits)?;
        if !buffered.is_worker_backed() && processor.timing != PreparedProcessor::INLINE_TIMING {
            return Err(Error::InvalidConfig);
        }
        cells.publish(
            &**buffered.backend(),
            Frame(0),
            0,
            ProcessorPhase::Free,
            Default::default(),
            buffered.async_stats(),
        );
        buffered.abort(Frame(0))?;
        Ok(Self {
            buffered,
            cells,
            generation: 0,
            phase: ProcessorPhase::Free,
            cleanup: 0,
            slot,
            slots,
            quantum: quantum.frames(),
            signal: None,
            binding: None,
            pending_binding: None,
            parameters: [(Frame(0), 0, 0.); 64],
            parameter_count: 0,
            manual_active: None,
            global,
            global_capacity,
            midi_capacity: rt_runtime::buffer::MidiCapacity {
                events: limits.midi_input.events_per_quantum,
                bytes: limits.midi_input.bytes_per_quantum,
            },
            midi_output_capacity: rt_runtime::buffer::MidiCapacity {
                events: limits.midi_output.events_per_quantum,
                bytes: limits.midi_output.bytes_per_quantum,
            },
        })
    }
    pub(crate) fn attach(&mut self, cells: Arc<crate::cells::Cells>) {
        self.signal = Some(cells);
    }
    pub(crate) fn bind(
        &mut self,
        generation: u32,
        source: u32,
        strip_generation: u32,
        epoch: rt_runtime::Epoch,
    ) -> Result<(), Error> {
        self.check(generation)?;
        if self.phase != ProcessorPhase::Owned
            || strip_generation == 0
            || self
                .signal
                .as_ref()
                .is_none_or(|cells| source as usize >= cells.strips.len())
        {
            return Err(Error::InvalidParameter);
        }
        self.pending_binding = Some((
            epoch,
            ProcessorTarget::Track {
                source: source as usize,
                generation: strip_generation,
            },
        ));
        Ok(())
    }
    pub(crate) fn bind_bus(
        &mut self,
        generation: u32,
        bus: shoop_rt_protocol::BusId,
        epoch: rt_runtime::Epoch,
    ) -> Result<(), Error> {
        self.check(generation)?;
        if self.phase != ProcessorPhase::Owned
            || bus.0.generation == 0
            || self
                .signal
                .as_ref()
                .is_none_or(|cells| bus.0.index as usize >= cells.buses.len())
        {
            return Err(Error::InvalidParameter);
        }
        self.pending_binding = Some((epoch, ProcessorTarget::Bus(bus)));
        Ok(())
    }
    fn track_binding(&self) -> Option<(usize, u32)> {
        match self.binding {
            Some(ProcessorTarget::Track { source, generation }) => Some((source, generation)),
            _ => None,
        }
    }
    fn publish(&self, frame: Frame) {
        self.cells.publish(
            &**self.buffered.backend(),
            frame,
            self.generation,
            self.phase,
            self.global.diagnostics(),
            self.buffered.async_stats(),
        );
    }
    fn check(&self, generation: u32) -> Result<(), Error> {
        if generation != self.generation {
            return Err(Error::InvalidId);
        }
        if !matches!(self.phase, ProcessorPhase::Owned | ProcessorPhase::Running) {
            return Err(Error::InvalidState);
        }
        Ok(())
    }
    pub(crate) fn claim(&mut self, frame: Frame, generation: u32) -> Result<(), Error> {
        if self.phase != ProcessorPhase::Free {
            return Err(Error::Busy);
        }
        if self.generation.checked_add(1) != Some(generation) {
            return Err(Error::InvalidId);
        }
        self.generation = generation;
        self.phase = ProcessorPhase::Owned;
        self.publish(frame);
        Ok(())
    }
    pub(crate) fn start(&mut self, frame: Frame, generation: u32) -> Result<(), Error> {
        self.check(generation)?;
        if self.phase != ProcessorPhase::Owned {
            return Err(Error::InvalidState);
        }
        self.buffered.activate_prepared(frame)?;
        self.phase = ProcessorPhase::Running;
        self.publish(frame);
        Ok(())
    }
    pub(crate) fn parameter(
        &mut self,
        frame: Frame,
        generation: u32,
        id: u32,
        value: f32,
    ) -> Result<(), Error> {
        self.check(generation)?;
        if self.phase == ProcessorPhase::Running {
            if id == shoop_rt_protocol::processors::WIDTH {
                return Err(Error::InvalidParameter);
            }
            self.buffered.backend().validate_parameter(id, value)?;
            let slot = self
                .parameters
                .get_mut(self.parameter_count)
                .ok_or(Error::Capacity)?;
            *slot = (frame, id, value);
            self.parameter_count += 1;
            Ok(())
        } else {
            self.buffered.inactive_backend_mut()?.control(id, value)?;
            self.publish(frame);
            Ok(())
        }
    }
    pub(crate) fn suspend(&mut self, frame: Frame, generation: u32) -> Result<(), Error> {
        self.check(generation)?;
        if self.phase == ProcessorPhase::Owned {
            return Ok(());
        }
        self.buffered.abort(frame)?;
        if let Err(error) = self.buffered.inactive_backend_mut()?.suspend() {
            self.phase = ProcessorPhase::Faulted;
            self.publish(frame);
            return Err(error);
        }
        for &(_, id, value) in &self.parameters[..self.parameter_count] {
            self.buffered.inactive_backend_mut()?.control(id, value)?;
        }
        self.parameter_count = 0;
        self.phase = ProcessorPhase::Owned;
        self.publish(frame);
        Ok(())
    }
    pub(crate) fn control(
        &mut self,
        frame: Frame,
        generation: u32,
        control: shoop_rt_protocol::processors::Control,
    ) -> Result<(), Error> {
        use shoop_rt_protocol::processors::{Control, CC_BASE, CLEAR_CC, RESET, WIDTH};
        self.check(generation)?;
        if let Control::Active(on) = control {
            self.manual_active = Some((self.routing_state(), on));
            if !on {
                self.suspend(frame, generation)?;
            }
            return Ok(());
        }
        let Control::Restore {
            values,
            replace_mappings,
        } = control
        else {
            let Control::Parameter { parameter, value } = control else {
                unreachable!()
            };
            if parameter == WIDTH {
                return Err(Error::InvalidParameter);
            }
            return self.parameter(frame, generation, parameter, value);
        };
        let kind = self.cells.kind;
        let mut changes = [(frame, 0, 0.); STATE_SCALARS];
        self.buffered.backend().validate_parameter(RESET, 0.)?;
        let mut count = 0;
        if replace_mappings {
            self.buffered.backend().validate_parameter(CLEAR_CC, 0.)?;
            changes[count] = (frame, CLEAR_CC, 0.);
            count += 1;
        }
        for (index, &value) in values.iter().enumerate().take(kind.scalar_count()) {
            let id = kind.scalar_id(index).ok_or(Error::InvalidParameter)?;
            if id == WIDTH {
                if self.buffered.backend().parameter(id) != Some(value) {
                    return Err(Error::InvalidShape);
                }
                continue;
            }
            if id >= CC_BASE {
                if !replace_mappings {
                    continue;
                }
                if value >= 0.
                    && (0..index).any(|previous| {
                        kind.scalar_id(previous)
                            .is_some_and(|id| id >= CC_BASE && id < WIDTH)
                            && values[previous] == value
                    })
                {
                    return Err(Error::InvalidParameter);
                }
            }
            self.buffered.backend().validate_parameter(id, value)?;
            changes[count] = (frame, id, value);
            count += 1;
        }
        changes[count] = (frame, RESET, 0.);
        count += 1;
        if self.phase == ProcessorPhase::Running {
            if count > self.parameters.len() - self.parameter_count {
                return Err(Error::Capacity);
            }
            self.parameters[self.parameter_count..self.parameter_count + count]
                .copy_from_slice(&changes[..count]);
            self.parameter_count += count;
        } else {
            for &(_, id, value) in &changes[..count] {
                self.buffered.inactive_backend_mut()?.control(id, value)?;
            }
            self.publish(frame);
        }
        Ok(())
    }
    pub(crate) fn retire(&mut self, frame: Frame, generation: u32) -> Result<(), Error> {
        if generation != self.generation {
            return Err(Error::InvalidId);
        }
        if matches!(self.phase, ProcessorPhase::Free | ProcessorPhase::Retiring) {
            return Err(Error::InvalidState);
        }
        self.buffered.abort(frame)?;
        self.parameter_count = 0;
        self.binding = None;
        self.manual_active = None;
        self.global.reset();
        self.pending_binding = None;
        self.phase = ProcessorPhase::Retiring;
        self.cleanup = 0;
        self.publish(frame);
        Ok(())
    }
    pub(crate) fn midi_buffer_settings(
        &self,
    ) -> Result<rt_runtime::processor::MidiBufferSettings, Error> {
        use rt_runtime::processor::{MidiBufferSettings, MidiPortCapacity};
        let mut settings = MidiBufferSettings::default();
        if self.buffered.ports().midi_inputs == 1 {
            settings.ports[0] = Some(MidiPortCapacity {
                input: true,
                port: 0,
                capacity: self.midi_capacity,
            });
            settings.ports[1] = Some(MidiPortCapacity {
                input: true,
                port: 2,
                capacity: self.global_capacity,
            });
        }
        if self.buffered.ports().midi_outputs == 1 {
            settings.ports[2] = Some(MidiPortCapacity {
                input: false,
                port: 0,
                capacity: self.midi_output_capacity,
            });
        }
        Ok(settings)
    }
    pub(crate) fn ports(&self) -> PortCounts {
        let mut ports = self.buffered.ports();
        ports.audio_outputs *= 2;
        if ports.midi_inputs == 1 {
            ports.midi_inputs += 2;
        }
        ports
    }
    pub(crate) fn process(
        &mut self,
        block: &mut ProcessBlock<'_, LoopPlanner>,
    ) -> Result<(), Error> {
        if let Some((_, target)) = self
            .pending_binding
            .take_if(|(epoch, _)| *epoch == block.graph_epoch)
        {
            self.binding = Some(target);
        }
        let mut demand = match self.binding {
            Some(ProcessorTarget::Bus(bus)) => bus.0.generation != 0,
            Some(ProcessorTarget::Track { source, generation }) => {
                self.signal.as_ref().is_some_and(|cells| {
                    let signal = &cells.strips[source];
                    signal.owner_generation.load(Ordering::Relaxed) == generation
                        && signal.processor_demand.load(Ordering::Relaxed)
                })
            }
            None => self.phase == ProcessorPhase::Running,
        };
        if let Some((routing, active)) = self.manual_active {
            if routing == self.routing_state() {
                demand = active;
            } else {
                self.manual_active = None;
            }
        }
        if self.buffered.is_worker_backed() {
            if let Some((source, _)) = self.track_binding() {
                block.domain.abort_unpaired_worker_captures(source as u32);
            }
        }
        let result = self.render(block, demand);
        if result.is_err() {
            self.phase = ProcessorPhase::Faulted;
            self.publish(block.frame);
            self.silence(block)?;
        }
        if self.phase == ProcessorPhase::Faulted {
            if let Some((source, _)) = self.track_binding() {
                block
                    .domain
                    .abort_source_captures(source as u32, Error::InvalidState);
            }
        }
        self.maintain(block.frame)?;
        Ok(())
    }
    fn silence(&self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        for port in 0..self.ports().audio_outputs {
            block
                .audio
                .output(port)
                .ok_or(Error::InvalidShape)?
                .fill(0.);
        }
        for output in block.midi_outputs.iter_mut() {
            output.clear();
        }
        Ok(())
    }
    fn routing_state(&self) -> u32 {
        self.track_binding()
            .and_then(|(source, _)| {
                self.signal
                    .as_ref()
                    .map(|cells| cells.strips[source].routing_state.load(Ordering::Relaxed))
            })
            .unwrap_or(0)
    }
    fn render(
        &mut self,
        block: &mut ProcessBlock<'_, LoopPlanner>,
        demand: bool,
    ) -> Result<(), Error> {
        let has_midi = self.buffered.ports().midi_inputs == 1;
        if self.phase == ProcessorPhase::Running
            && block
                .midi_inputs
                .first()
                .is_some_and(|input| input.is_recovery())
        {
            self.suspend(block.frame, self.generation)?;
        }
        if demand && self.phase == ProcessorPhase::Owned {
            self.start(block.frame, self.generation)?;
        }
        if self.phase != ProcessorPhase::Running {
            if self.phase == ProcessorPhase::Owned && has_midi {
                self.global.render(
                    &block.midi_inputs[2],
                    &block.midi_inputs[0],
                    block.frames,
                    false,
                )?;
                self.idle_controls(&block.midi_inputs[1], &block.midi_inputs[0])?;
                self.publish(block.frame.checked_add(block.frames)?);
            }
            self.silence(block)?;
            if matches!(self.binding, Some(ProcessorTarget::Bus(_)))
                && self.phase == ProcessorPhase::Owned
            {
                for channel in 0..self.buffered.ports().audio_inputs {
                    for i in 0..block.frames {
                        let value = block.audio.input(channel).ok_or(Error::InvalidShape)?[i];
                        block.audio.output(channel).ok_or(Error::InvalidShape)?[i] = value;
                    }
                }
            }
            return Ok(());
        }
        let worker = self.buffered.is_worker_backed();
        if !demand && self.buffered.lifecycle() == Lifecycle::Running {
            self.buffered.begin_drain()?;
        }
        let collecting = self.buffered.lifecycle() == Lifecycle::Running;
        if has_midi {
            self.global.render(
                &block.midi_inputs[2],
                &block.midi_inputs[0],
                block.frames,
                collecting,
            )?;
        }
        if collecting {
            for &(frame, id, value) in &self.parameters[..self.parameter_count] {
                self.buffered.schedule_parameter(frame, id, value)?;
            }
            self.parameter_count = 0;
        }
        let ports = self.buffered.ports();
        {
            let mut audio = block
                .audio
                .subset(0..ports.audio_inputs, 0..ports.audio_outputs)?;
            let capture_source = if worker {
                None
            } else {
                self.track_binding()
                    .map(|(source, _)| source as u32)
                    .filter(|source| block.domain.recording_starts(*source).next().is_some())
            };
            let mut observer = crate::recorded::TakeObserver {
                domain: block.domain,
                cells: self.signal.as_deref(),
                source: capture_source,
                kind: self.cells.kind,
                frame: block.frame,
                done: false,
            };
            let observed = !worker && self.buffered.backend().supports_observation();
            self.buffered.process_ports_tick_controls(
                block.frame,
                block.quantum,
                &mut audio,
                if has_midi {
                    &block.midi_inputs[..1]
                } else {
                    block.midi_inputs
                },
                block.midi_outputs,
                has_midi.then(|| self.global.controls()),
                if observed { Some(&mut observer) } else { None },
            )?;
        }
        let paired_worker_release = worker
            && self.track_binding().is_some_and(|(source, generation)| {
                let Some(release) = self
                    .buffered
                    .last_release()
                    .filter(|release| release.available)
                else {
                    return false;
                };
                let Ok(source_frame) = release.tag.source.checked_add(release.offset) else {
                    return false;
                };
                block.domain.publish_worker_capture_release(
                    source,
                    WorkerCaptureRelease {
                        owner: WorkerCaptureSource {
                            strip_generation: generation,
                            processor: block.node,
                            processor_generation: self.generation,
                            delay_frames: self.buffered.timing().latency_frames(),
                        },
                        source_frame,
                        render_frame: block.frame,
                        stream_generation: release.tag.generation,
                    },
                )
            });
        for port in 0..ports.audio_outputs {
            if paired_worker_release {
                block.audio.copy_output(port, ports.audio_outputs + port)?;
                continue;
            }
            let output = block
                .audio
                .output(ports.audio_outputs + port)
                .ok_or(Error::InvalidShape)?;
            if let Some((_, samples)) = (!worker)
                .then(|| self.buffered.completed_audio(block.frame, port))
                .flatten()
            {
                output.copy_from_slice(samples);
            } else {
                output.fill(0.);
            }
        }
        if (0..self.ports().audio_outputs).any(|port| {
            block
                .audio
                .output(port)
                .is_none_or(|samples| samples.iter().any(|sample| !sample.is_finite()))
        }) {
            return Err(Error::InvalidParameter);
        }
        if (!worker && !collecting) || self.buffered.lifecycle() == Lifecycle::Suspended {
            self.suspend(block.frame.checked_add(self.quantum)?, self.generation)?;
            if has_midi {
                self.idle_controls(&block.midi_inputs[1], &block.midi_inputs[0])?;
            }
        }
        self.publish(block.frame.checked_add(block.frames)?);
        Ok(())
    }
    fn idle_controls(
        &mut self,
        raw: &rt_runtime::buffer::MidiBuffer,
        ordinary: &rt_runtime::buffer::MidiBuffer,
    ) -> Result<(), Error> {
        let routing = self.routing_state();
        let live = routing & 7 != 0 && routing & 48 == 0;
        let mut raw_index = if raw.is_recovery() || live {
            raw.len()
        } else {
            0
        };
        let mut ordinary_index = if ordinary.is_recovery() {
            ordinary.len()
        } else {
            0
        };
        while raw_index < raw.len() || ordinary_index < ordinary.len() {
            let raw_event = raw.get(raw_index);
            let ordinary_event = ordinary.get(ordinary_index);
            let event = if raw_event
                .is_some_and(|(at, _)| ordinary_event.is_none_or(|(other, _)| at <= other))
            {
                raw_index += 1;
                raw_event
            } else {
                ordinary_index += 1;
                ordinary_event
            };
            let (_, data) = event.ok_or(Error::InvalidEvent)?;
            self.buffered.inactive_backend_mut()?.control_midi(data)?;
        }
        Ok(())
    }
    fn maintain(&mut self, frame: Frame) -> Result<(), Error> {
        if self.phase == ProcessorPhase::Retiring
            && (frame.0 / self.quantum as u64) % self.slots as u64 == self.slot as u64
        {
            match self
                .buffered
                .inactive_backend_mut()?
                .reset_step(self.cleanup)
            {
                Ok(true) => self.phase = ProcessorPhase::Free,
                Ok(false) => self.cleanup += 1,
                Err(_) => self.phase = ProcessorPhase::Faulted,
            }
            self.publish(frame);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rt_buffered::{AsyncTransport, Batch, LocalTransport, MidiLimits};
    use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};
    struct TestBackend {
        value: f32,
    }
    impl Backend for TestBackend {
        fn ports(&self) -> PortCounts {
            PortCounts::default()
        }
        fn parameter(&self, id: u32) -> Option<f32> {
            (id == 0).then_some(self.value)
        }
        fn validate_parameter(&self, id: u32, value: f32) -> Result<(), Error> {
            if id == 0 && value.is_finite() {
                Ok(())
            } else {
                Err(Error::InvalidParameter)
            }
        }
        fn control(&mut self, id: u32, value: f32) -> Result<(), Error> {
            if id != 0 || !value.is_finite() {
                return Err(Error::InvalidParameter);
            }
            self.value = value;
            Ok(())
        }
        fn process(&mut self, _: &mut Batch<'_, '_>) -> Result<(), Error> {
            Ok(())
        }
        fn suspend(&mut self) -> Result<(), Error> {
            Ok(())
        }
        fn reset(&mut self) -> Result<(), Error> {
            self.value = 0.;
            Ok(())
        }
        fn reset_step(&mut self, step: usize) -> Result<bool, Error> {
            if step == 2 {
                self.reset()?;
                return Ok(true);
            }
            Ok(false)
        }
    }

    #[shoop_test]
    fn hosted_worker_pairs_source_inputs_and_releases_the_padded_tail_before_suspending() {
        use crate::node::{EngineNode, EngineOp, NodeConfiguration, NodeSettings, StripInput};
        use rt_runtime::{
            buffer::{AudioBlock, MidiBlock, MidiCapacity},
            executor::{GraphRuntime, PreparedNode},
            graph::{Connection, Endpoint, GraphDescription, PortType},
            resource::ResourceStore,
            Config, Epoch, NodeId, PlannerId,
        };
        use shoop_loops::{
            runtime::{Settings, WorkerCaptureInputSettings},
            Limits, TopologyDescription,
        };

        const PORTS: PortCounts = PortCounts {
            audio_inputs: 1,
            audio_outputs: 1,
            midi_inputs: 0,
            midi_outputs: 0,
        };
        struct EchoWorker(LocalTransport);
        impl Backend for EchoWorker {
            fn ports(&self) -> PortCounts {
                PORTS
            }

            fn async_transport(&mut self) -> Option<&mut dyn AsyncTransport> {
                Some(&mut self.0)
            }

            fn process(&mut self, _: &mut Batch<'_, '_>) -> Result<(), Error> {
                panic!("worker ran inline")
            }

            fn suspend(&mut self) -> Result<(), Error> {
                Ok(())
            }

            fn reset(&mut self) -> Result<(), Error> {
                Ok(())
            }
        }

        let q = Quantum::new(16).unwrap();
        let timing = Timing {
            batch_quanta: 2,
            slack_quanta: 1,
        };
        let limits = BufferLimits::default();
        let (transport, mut worker) =
            LocalTransport::prepare(PORTS, timing.prepare(q).unwrap(), limits, 2).unwrap();
        let state = Arc::new(ProcessorCells::new(ProcessorKind::BuiltInFx));
        let mut hosted = HostedProcessor::new(
            PreparedProcessor {
                kind: ProcessorKind::BuiltInFx,
                backend: Box::new(EchoWorker(transport)),
                timing,
            },
            state.clone(),
            q,
            limits,
            Default::default(),
            0,
            1,
        )
        .unwrap();
        let driver = shoop_rt_protocol::DriverStatus {
            kind: shoop_rt_protocol::DriverKind::Dummy,
            state: shoop_rt_protocol::DriverRunState::Running,
            sample_rate: 8000,
            buffer_frames: 16,
        };
        let signal = crate::cells::Cells::new(1, 1, 1, 512, driver, 16);
        signal.strips[0]
            .owner_generation
            .store(1, Ordering::Relaxed);
        hosted.attach(signal.clone());
        hosted.claim(Frame(0), 1).unwrap();
        hosted.binding = Some(ProcessorTarget::Track {
            source: 0,
            generation: 1,
        });
        let config = Config {
            audio_inputs: 1,
            audio_outputs: 2,
            midi_inputs: 0,
            midi_outputs: 0,
            ..Config::default()
        }
        .prepare()
        .unwrap();
        let mut resources = ResourceStore::new(config, q).unwrap();
        let topology =
            shoop_loops::compile(Limits::default(), &TopologyDescription::default()).unwrap();
        let mut loops = LoopPlanner::prepare(
            config,
            q,
            topology,
            &mut resources,
            Settings {
                planner: PlannerId::from_parts(0, 1).unwrap(),
                normal_requests: 1,
                media_slots: 2,
                export_slots: 1,
            },
            Vec::new(),
            Vec::new(),
            Default::default(),
        )
        .unwrap();
        loops
            .planner
            .prepare_worker_capture_inputs(WorkerCaptureInputSettings {
                sources: 1,
                edit_loops: 0,
                channels: 1,
                midi: MidiCapacity::default(),
                delay_frames: timing.prepare(q).unwrap().latency_frames(),
            })
            .unwrap();
        // Drive the worker's demand explicitly while exercising the production dry tap.
        let mut source = EngineNode::StripInput(StripInput::new(
            0,
            &NodeSettings {
                midi: crate::midi_layout::Layout::new(&crate::EngineDescription::default())
                    .unwrap(),
                external_midi: None,
                cells: crate::cells::Cells::new(1, 1, 1, 512, driver, 16),
                channels: 1,
                master_channels: 1,
                frames: q.frames(),
                ramp: 0,
            },
        ));
        source
            .apply_signal(
                EngineOp::Configure(NodeConfiguration {
                    immediate_playback: false,
                    recorded_state: None,
                    processor: Some(ProcessorKind::BuiltInFx),
                    send_bank: 0,
                    return_bank: 0,
                    token: 1,
                    epoch: Epoch(1),
                    inputs: 1,
                    outputs: 1,
                    bank: 0,
                    gain: 1.,
                    balance: 0.,
                    monitoring: false,
                    generation: 1,
                    muted: false,
                }),
                Epoch(0),
            )
            .unwrap();
        let node = NodeId::from_parts(0, 1).unwrap();
        let input = NodeId::from_parts(1, 1).unwrap();
        let (_, mut runtime) = GraphRuntime::prepare_with_planner_and_resources(
            config,
            q,
            alloc::vec![
                PreparedNode::new(node, EngineNode::Processor(hosted)),
                PreparedNode::new(input, source),
            ],
            loops.planner,
            resources,
        )
        .unwrap();
        runtime
            .prepare_initial_graph(&GraphDescription {
                nodes: alloc::vec![input, node],
                connections: alloc::vec![
                    Connection {
                        source: Endpoint::host(PortType::Audio, 0),
                        destination: Endpoint::node(input, PortType::Audio, 0)
                    },
                    Connection {
                        source: Endpoint::node(input, PortType::Audio, 0),
                        destination: Endpoint::node(node, PortType::Audio, 0)
                    },
                    Connection {
                        source: Endpoint::node(node, PortType::Audio, 0),
                        destination: Endpoint::host(PortType::Audio, 0)
                    },
                    Connection {
                        source: Endpoint::node(node, PortType::Audio, 1),
                        destination: Endpoint::host(PortType::Audio, 1)
                    },
                ],
            })
            .unwrap();

        for tick in 0..10 {
            worker
                .process_next(|_, batch| {
                    let (input, output) = batch.audio.pair(0, 0).unwrap();
                    output.copy_from_slice(input);
                    Ok(())
                })
                .unwrap();
            signal.strips[0]
                .processor_demand
                .store(tick == 0 || (3..=6).contains(&tick), Ordering::Relaxed);
            let mut monitor = [0.; 16];
            let mut capture = [1.; 16];
            assert_no_heap(|| {
                runtime.process(
                    AudioBlock {
                        frame: Frame(tick * 16),
                        frames: 16,
                        inputs: &[&[tick as f32 + 1.; 16]],
                        outputs: &mut [&mut monitor, &mut capture],
                    },
                    MidiBlock {
                        inputs: &[],
                        outputs: &mut [],
                    },
                )
            })
            .unwrap();
            assert_eq!(
                monitor,
                [match tick {
                    3 => 1.,
                    8 => 6.,
                    9 => 7.,
                    _ => 0.,
                }; 16]
            );
            assert_eq!(capture, monitor);
            assert_eq!(
                state.read().unwrap().phase,
                if [4, 9].contains(&tick) {
                    ProcessorPhase::Owned
                } else {
                    ProcessorPhase::Running
                }
            );
        }
    }

    #[shoop_test]
    fn generation_claim_and_incremental_return_never_expose_previous_owner() {
        let cells = Arc::new(ProcessorCells::new(ProcessorKind::BuiltInFx));
        let mut node = HostedProcessor::new(
            PreparedProcessor {
                kind: ProcessorKind::BuiltInFx,
                backend: Box::new(TestBackend { value: 0. }),
                timing: PreparedProcessor::INLINE_TIMING,
            },
            cells.clone(),
            Quantum::new(16).unwrap(),
            BufferLimits {
                midi_input: MidiLimits {
                    events_per_quantum: 0,
                    bytes_per_quantum: 0,
                },
                midi_output: MidiLimits {
                    events_per_quantum: 0,
                    bytes_per_quantum: 0,
                },
                parameters_per_quantum: 4,
            },
            rt_runtime::buffer::MidiCapacity::default(),
            1,
            2,
        )
        .unwrap();
        node.attach(crate::cells::Cells::new(
            1,
            1,
            1,
            512,
            shoop_rt_protocol::DriverStatus {
                kind: shoop_rt_protocol::DriverKind::Dummy,
                state: shoop_rt_protocol::DriverRunState::Running,
                sample_rate: 8000,
                buffer_frames: 16,
            },
            16,
        ));
        assert_no_heap(|| {
            assert_eq!(node.claim(Frame(0), 0), Err(Error::InvalidId));
            node.claim(Frame(0), 1).unwrap();
            node.bind_bus(1, shoop_rt_protocol::BusId::MASTER, rt_runtime::Epoch(1))
                .unwrap();
            assert_eq!(
                node.bind_bus(2, shoop_rt_protocol::BusId::MASTER, rt_runtime::Epoch(1)),
                Err(Error::InvalidId)
            );
            node.binding = node.pending_binding.take().map(|(_, target)| target);
            assert_eq!(node.track_binding(), None);
            assert_eq!(node.routing_state(), 0);
            assert_eq!(node.claim(Frame(0), 2), Err(Error::Busy));
            node.parameter(Frame(0), 1, 0, 0.75).unwrap();
            assert_eq!(cells.read().unwrap().values[0], 0.75);
            node.retire(Frame(0), 1).unwrap();
            assert!(node.binding.is_none() && node.pending_binding.is_none());
            assert!(node.parameter(Frame(0), 1, 0, 1.).is_err());
            for frame in [0, 16, 32, 48, 64] {
                node.maintain(Frame(frame)).unwrap();
            }
            assert_eq!(cells.read().unwrap().phase, ProcessorPhase::Retiring);
            assert_eq!(node.claim(Frame(64), 2), Err(Error::Busy));
            node.maintain(Frame(80)).unwrap();
            assert_eq!(cells.read().unwrap().phase, ProcessorPhase::Free);
            assert_eq!(cells.read().unwrap().values[0], 0.);
            node.claim(Frame(80), 2).unwrap();
            assert_eq!(node.parameter(Frame(80), 1, 0, 0.5), Err(Error::InvalidId));
            node.parameter(Frame(80), 2, 0, 0.25).unwrap();
            node.start(Frame(80), 2).unwrap();
            assert_eq!(node.start(Frame(80), 2), Err(Error::InvalidState));
            assert_eq!(cells.read().unwrap().generation, 2);
            node.parameter(Frame(96), 2, 0, 0.375).unwrap();
            assert_eq!(cells.read().unwrap().values[0], 0.25);
            node.suspend(Frame(96), 2).unwrap();
            assert_eq!(cells.read().unwrap().phase, ProcessorPhase::Owned);
            assert_eq!(cells.read().unwrap().values[0], 0.375);
            node.parameter(Frame(96), 2, 0, 0.5).unwrap();
            node.start(Frame(96), 2).unwrap();
            assert_eq!(cells.read().unwrap().values[0], 0.5);
        });
    }
}
