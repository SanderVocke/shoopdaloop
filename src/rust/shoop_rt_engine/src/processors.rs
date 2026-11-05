//! Prepared opaque processor owners and allocation-free applied-state observation.
use alloc::{boxed::Box, sync::Arc, vec::Vec};
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use rt_buffered::{Backend, BufferLimits, BufferedProcessor, Timing};
use rt_runtime::{graph::PortCounts, processor::ProcessBlock, Error, Frame, Quantum};
use shoop_loops::runtime::LoopPlanner;
use shoop_rt_protocol::{
    processors::{Effective, Unavailable, STATE_SCALARS},
    ProcessorKind,
};

pub struct PreparedProcessor {
    pub kind: ProcessorKind,
    pub backend: Box<dyn Backend>,
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
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProcessorState {
    pub global_midi: shoop_rt_protocol::processors::GlobalMidiDiagnostics,
    pub kind: ProcessorKind,
    pub frame: Frame,
    pub generation: u32,
    pub phase: ProcessorPhase,
    pub values: [f32; STATE_SCALARS],
}

pub use shoop_rt_protocol::processors::Phase as ProcessorPhase;

pub(crate) struct ProcessorCells {
    global_counts: [AtomicU64; 4],
    global_pending: AtomicU32,
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
        self.global_pending
            .store(u32::from(global.pending), Ordering::SeqCst);
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
        let global_midi = shoop_rt_protocol::processors::GlobalMidiDiagnostics {
            rejected: self.global_counts[0].load(Ordering::SeqCst),
            overwritten: self.global_counts[1].load(Ordering::SeqCst),
            deferred: self.global_counts[2].load(Ordering::SeqCst),
            drained: self.global_counts[3].load(Ordering::SeqCst),
            pending: self.global_pending.load(Ordering::SeqCst) as u16,
        };
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
            values,
        })
    }
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
    binding: Option<(usize, u32)>,
    pending_binding: Option<(rt_runtime::Epoch, usize, u32)>,
    parameters: [(Frame, u32, f32); 64],
    parameter_count: usize,
    manual_active: Option<(u32, bool)>,
    global: crate::global_midi::GlobalMidi,
}
impl HostedProcessor {
    pub fn additional_prepared_bytes(limits: BufferLimits) -> Result<usize, Error> {
        crate::global_midi::GlobalMidi::storage_bytes(
            limits.midi_input.events_per_quantum,
            limits.midi_input.bytes_per_quantum,
        )
    }
    pub(crate) fn new(
        backend: Box<dyn Backend>,
        cells: Arc<ProcessorCells>,
        quantum: Quantum,
        limits: BufferLimits,
        slot: usize,
        slots: usize,
    ) -> Result<Self, Error> {
        if backend.ports().midi_inputs > 1 {
            return Err(Error::InvalidShape);
        }
        let global = crate::global_midi::GlobalMidi::new(
            limits.midi_input.events_per_quantum,
            limits.midi_input.bytes_per_quantum,
        )?;
        let mut buffered = BufferedProcessor::with_backend(
            backend,
            quantum,
            Timing {
                batch_quanta: 1,
                slack_quanta: 0,
            },
            limits,
        )?;
        cells.publish(
            &**buffered.backend(),
            Frame(0),
            0,
            ProcessorPhase::Free,
            Default::default(),
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
        self.pending_binding = Some((epoch, source as usize, strip_generation));
        Ok(())
    }
    fn publish(&self, frame: Frame) {
        self.cells.publish(
            &**self.buffered.backend(),
            frame,
            self.generation,
            self.phase,
            self.global.diagnostics(),
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
        if let Some((_, source, generation)) = self
            .pending_binding
            .take_if(|(epoch, _, _)| *epoch == block.graph_epoch)
        {
            self.binding = Some((source, generation));
        }
        let mut demand = match self.binding {
            Some((source, generation)) => self.signal.as_ref().is_some_and(|cells| {
                let signal = &cells.strips[source];
                signal.owner_generation.load(Ordering::Relaxed) == generation
                    && signal.processor_demand.load(Ordering::Relaxed)
            }),
            None => self.phase == ProcessorPhase::Running,
        };
        if let Some((routing, active)) = self.manual_active {
            if routing == self.routing_state() {
                demand = active;
            } else {
                self.manual_active = None;
            }
        }
        let result = self.render(block, demand);
        if result.is_err() {
            self.phase = ProcessorPhase::Faulted;
            self.publish(block.frame);
            self.silence(block)?;
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
        self.binding
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
            return self.silence(block);
        }
        if has_midi {
            self.global.render(
                &block.midi_inputs[2],
                &block.midi_inputs[0],
                block.frames,
                demand,
            )?;
        }
        if !demand {
            self.buffered.begin_drain()?;
        } else {
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
            self.buffered.process_ports_tick(
                block.frame,
                block.quantum,
                &mut audio,
                if has_midi {
                    core::slice::from_ref(self.global.output())
                } else {
                    block.midi_inputs
                },
                block.midi_outputs,
            )?;
        }
        for port in 0..ports.audio_outputs {
            let output = block
                .audio
                .output(ports.audio_outputs + port)
                .ok_or(Error::InvalidShape)?;
            if let Some((_, samples)) = self.buffered.completed_audio(block.frame, port) {
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
        if !demand {
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
    use rt_buffered::{Batch, MidiLimits};
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
        fn process(&mut self, _: &mut Batch<'_>) -> Result<(), Error> {
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
    fn generation_claim_and_incremental_return_never_expose_previous_owner() {
        let cells = Arc::new(ProcessorCells::new(ProcessorKind::BuiltInFx));
        let mut node = HostedProcessor::new(
            Box::new(TestBackend { value: 0. }),
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
            1,
            2,
        )
        .unwrap();
        assert_no_heap(|| {
            assert_eq!(node.claim(Frame(0), 0), Err(Error::InvalidId));
            node.claim(Frame(0), 1).unwrap();
            assert_eq!(node.claim(Frame(0), 2), Err(Error::Busy));
            node.parameter(Frame(0), 1, 0, 0.75).unwrap();
            assert_eq!(cells.read().unwrap().values[0], 0.75);
            node.retire(Frame(0), 1).unwrap();
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
