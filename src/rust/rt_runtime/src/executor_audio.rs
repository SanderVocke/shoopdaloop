use super::*;
use crate::audio_refs::{AudioSequence, ReferenceBlock, RenderArena};

impl<P: Processor<L, Q>, L: Planner, Q: ProcessorOperation> GraphRuntime<P, L, Q> {
    pub fn audio_pool_work(&self) -> crate::resource::shared_audio::AudioPoolWork {
        self.resources.audio_pool_work()
    }
    pub fn audio_graph_work(&self) -> crate::audio_refs::AudioGraphWork {
        self.resources.audio_graph_work()
    }

    pub(super) fn run_graph_references(
        &mut self,
        audio: &mut impl crate::host_buffer::AudioIo,
        midi: &mut MidiBlock<'_>,
        start: usize,
        callback_start: Frame,
        quantum: crate::Quantum,
    ) -> Result<(), Error> {
        let frame = audio.frame().checked_add(start)?;
        let mut arena = self.audio_arena.take().ok_or(Error::InvalidState)?;
        if let Err(error) = arena.reset() {
            self.audio_arena = Some(arena);
            return Err(error);
        }
        self.host_sequences.fill(AudioSequence::default());
        let q = quantum.frames();
        let result = (|| {
            let Some(plan) = &self.active else {
                return Ok(());
            };
            if self.profiling.needs_layout() {
                for step in &plan.descriptor().steps {
                    self.profiling.layout_node(u64::from(step.node.slot()));
                }
                self.profiling.layout_done();
            }
            let mut first = 0;
            for step in &plan.descriptor().steps {
                {
                    let buffers = &mut self.slots[step.node.slot() as usize]
                        .as_mut()
                        .ok_or(Error::InvalidId)?
                        .buffers;
                    buffers.input_sequences.fill(AudioSequence::default());
                    buffers.output_sequences.fill(AudioSequence::default());
                    buffers.input_mixed.fill(false);
                    buffers.midi_forwarded.fill(false);
                    for input in &mut buffers.midi_input {
                        input.clear();
                    }
                    for output in &mut buffers.midi_output {
                        output.clear();
                    }
                }
                for edge in &plan.descriptor().connections[first..step.input_end] {
                    if edge.source.kind == PortType::Audio {
                        route_referenced(
                            &mut self.slots,
                            *edge,
                            &mut arena,
                            &mut self.host_sequences,
                            &self.resources,
                            audio,
                            start,
                            q,
                        )?;
                    } else {
                        route_offset(&mut self.slots, *edge, audio, midi, q, start, q, 0)?;
                    }
                    increment(
                        &mut self.diagnostics.connections_processed,
                        &mut self.diagnostics.counter_overflow,
                    );
                }
                let slot = self.slots[step.node.slot() as usize]
                    .as_mut()
                    .ok_or(Error::InvalidId)?;
                let processor = slot.processor.as_mut().ok_or(Error::InvalidId)?;
                let buffers = &mut slot.buffers;
                let handled = processor.process_references(&mut ReferenceBlock {
                    node: step.node,
                    frame,
                    graph_epoch: self.epoch,
                    quantum,
                    inputs: &buffers.input_sequences,
                    outputs: &mut buffers.output_sequences,
                    arena: &mut arena,
                    resources: &mut self.resources,
                    domain: &mut self.planner,
                })?;
                if !handled {
                    for (port, sequence) in buffers.input_sequences.iter().enumerate() {
                        arena.read_into(
                            *sequence,
                            &mut buffers.input[port * q..(port + 1) * q],
                            &self.resources,
                            false,
                        )?;
                    }
                    let output_count = slot.ports.audio_outputs;
                    let offset = arena.allocate(output_count * q)?;
                    let result = processor.process(&mut ProcessBlock {
                        node: step.node,
                        graph_epoch: self.epoch,
                        callback_start,
                        frame,
                        frames: q,
                        quantum,
                        audio: AudioPorts::new(
                            &buffers.input,
                            arena.output(offset, output_count * q),
                            q,
                            0,
                            q,
                        )?,
                        midi_inputs: &buffers.midi_input,
                        midi_outputs: &mut buffers.midi_output,
                        midi_forwarded: &mut buffers.midi_forwarded,
                        resources: self.resources.io(),
                        domain: &mut self.planner,
                    });
                    record_midi_overflow(result, &mut self.diagnostics)?;
                    for (port, sequence) in buffers.output_sequences.iter_mut().enumerate() {
                        *sequence = arena.transient(offset + port * q, q);
                    }
                }
                increment(
                    &mut self.diagnostics.nodes_processed,
                    &mut self.diagnostics.counter_overflow,
                );
                self.profiling.node_ready(u64::from(step.node.slot()));
                first = step.input_end;
            }
            for edge in &plan.descriptor().connections[first..] {
                if edge.source.kind == PortType::Audio {
                    route_referenced(
                        &mut self.slots,
                        *edge,
                        &mut arena,
                        &mut self.host_sequences,
                        &self.resources,
                        audio,
                        start,
                        q,
                    )?;
                } else {
                    route_offset(&mut self.slots, *edge, audio, midi, q, start, q, 0)?;
                }
                increment(
                    &mut self.diagnostics.connections_processed,
                    &mut self.diagnostics.counter_overflow,
                );
            }
            self.profiling.phase(rt_profiling::Phase::OutputRouting);
            Ok(())
        })();
        let unpin = arena.unpin(&mut self.resources);
        self.resources.add_audio_graph_work(arena.work);
        self.audio_arena = Some(arena);
        result.and(unpin)
    }
}

fn route_referenced<P>(
    slots: &mut [Option<Slot<P>>],
    edge: Connection,
    arena: &mut RenderArena,
    host: &mut [AudioSequence],
    resources: &ResourceStore,
    audio: &mut impl crate::host_buffer::AudioIo,
    start: usize,
    frames: usize,
) -> Result<(), Error> {
    let sequence = if let Some(source) = edge.source.node {
        slots[source.slot() as usize]
            .as_ref()
            .ok_or(Error::InvalidId)?
            .buffers
            .output_sequences[edge.source.port as usize]
    } else {
        let port = edge.source.port as usize;
        if host[port].silent() {
            let offset = arena.allocate(frames)?;
            arena
                .output(offset, frames)
                .copy_from_slice(&audio.input(port)[start..start + frames]);
            arena.work.sample_reads += frames as u64;
            arena.work.sample_writes += frames as u64;
            host[port] = arena.transient(offset, frames);
        }
        host[port]
    };
    if let Some(target) = edge.destination.node {
        let buffers = &mut slots[target.slot() as usize]
            .as_mut()
            .ok_or(Error::InvalidId)?
            .buffers;
        let port = edge.destination.port as usize;
        if buffers.input_sequences[port].silent() {
            buffers.input_sequences[port] = sequence;
            arena.work.forwarded += 1;
        } else if !sequence.silent() {
            let offset = if buffers.input_mixed[port] {
                let crate::audio_refs::ChunkRef::Transient { offset, .. } =
                    buffers.input_sequences[port].spans[0].chunk
                else {
                    return Err(Error::InvalidState);
                };
                offset
            } else {
                let offset = arena.allocate(frames)?;
                arena.accumulate(buffers.input_sequences[port], offset, frames, resources)?;
                buffers.input_sequences[port] = arena.transient(offset, frames);
                buffers.input_mixed[port] = true;
                offset
            };
            arena.accumulate(sequence, offset, frames, resources)?;
        }
    } else {
        arena.read_into(
            sequence,
            &mut audio.output(edge.destination.port as usize)[start..start + frames],
            resources,
            true,
        )?;
    }
    Ok(())
}
