use alloc::{boxed::Box, sync::Arc};
use rt_buffered::BufferLimits;
use rt_runtime::{
    buffer::MidiCapacity,
    graph::PortCounts,
    processor::{MidiBufferSettings, OperationContext, ProcessBlock},
    Error, Quantum,
};
use rtrb::{Consumer, Producer, PushError, RingBuffer};
use shoop_loops::runtime::LoopPlanner;
use shoop_rt_protocol::{ProcessorKind, ProcessorSlot, Refusal};

use crate::{
    cells::Cells, node::EngineOp, processors::ProcessorCells, HostedProcessor, PreparedProcessor,
};

pub(crate) const CHANNELS: usize = 16;

pub(crate) struct Slot {
    incoming: Producer<Box<HostedProcessor>>,
    retired: Consumer<Box<HostedProcessor>>,
    occupied: bool,
    cleanup: Option<Box<HostedProcessor>>,
}

pub(crate) struct Layout {
    pub(crate) first: usize,
    pub(crate) slots: Box<[Slot]>,
    pub(crate) limits: BufferLimits,
    pub(crate) global: MidiCapacity,
}

impl Layout {
    pub(crate) fn slot(&self, index: usize) -> Option<&Slot> {
        self.slots.get(index.checked_sub(self.first)?)
    }

    pub(crate) fn install(
        &mut self,
        slot: ProcessorSlot,
        processor: PreparedProcessor,
        cells: Arc<ProcessorCells>,
        signal: Arc<Cells>,
        quantum: Quantum,
    ) -> Result<(), Refusal> {
        let index = usize::from(slot.index);
        let handoff = self
            .slots
            .get_mut(index - self.first)
            .ok_or(Refusal::InvalidId)?;
        if handoff.occupied {
            return Err(Refusal::Busy);
        }
        let mut processor =
            HostedProcessor::new(processor, cells, quantum, self.limits, self.global, 0, 1)
                .map_err(|_| Refusal::Internal)?;
        processor.attach(signal);
        processor.set_previous_generation(slot.generation - 1);
        handoff
            .incoming
            .push(Box::new(processor))
            .map_err(|_| Refusal::Busy)?;
        handoff.occupied = true;
        Ok(())
    }
}

impl Slot {
    pub(crate) fn available(&self) -> bool {
        !self.occupied
    }

    pub(crate) fn reap(&mut self) -> bool {
        if self.cleanup.is_none() {
            self.cleanup = self.retired.pop().ok();
        }
        if self
            .cleanup
            .as_deref_mut()
            .is_some_and(HostedProcessor::reset_retired)
        {
            self.cleanup = None;
            self.occupied = false;
            true
        } else {
            false
        }
    }
}

pub struct DynamicProcessor {
    incoming: Consumer<Box<HostedProcessor>>,
    retired: Producer<Box<HostedProcessor>>,
    live: Option<Box<HostedProcessor>>,
    midi: MidiBufferSettings,
}

impl DynamicProcessor {
    pub(crate) fn new(limits: BufferLimits, global: MidiCapacity) -> (Slot, Self) {
        let (send, incoming) = RingBuffer::new(1);
        let (retired, receive) = RingBuffer::new(1);
        (
            Slot {
                incoming: send,
                retired: receive,
                occupied: false,
                cleanup: None,
            },
            Self {
                incoming,
                retired,
                live: None,
                midi: crate::processors::carla_midi_buffers(limits, global),
            },
        )
    }

    pub(crate) fn midi_buffer_settings(&self) -> MidiBufferSettings {
        self.midi
    }

    pub(crate) fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: CHANNELS,
            audio_outputs: CHANNELS * 2,
            midi_inputs: 3,
            midi_outputs: 1,
        }
    }

    pub(crate) fn operation(
        &mut self,
        operation: EngineOp,
        context: OperationContext<'_>,
    ) -> Result<(), Error> {
        if matches!(operation, EngineOp::ProcessorClaim { .. }) && self.live.is_none() {
            self.live = self.incoming.pop().ok();
        }
        let Some(processor) = self.live.as_deref_mut() else {
            return Err(Error::InvalidState);
        };
        let frame = context.frame;
        crate::node::processor_operation(processor, operation, context)?;
        if matches!(operation, EngineOp::ProcessorRetire { .. }) {
            processor.finish_dynamic_retirement(frame);
            if let Err(PushError::Full(processor)) = self.retired.push(self.live.take().unwrap()) {
                self.live = Some(processor);
                return Err(Error::Busy);
            }
        }
        Ok(())
    }

    pub(crate) fn process(
        &mut self,
        block: &mut ProcessBlock<'_, LoopPlanner>,
    ) -> Result<(), Error> {
        if let Some(processor) = self.live.as_deref_mut() {
            processor.process(block)?;
        }
        Ok(())
    }
}

pub(crate) fn empty_cells() -> ProcessorCells {
    ProcessorCells::new(ProcessorKind::External)
}
