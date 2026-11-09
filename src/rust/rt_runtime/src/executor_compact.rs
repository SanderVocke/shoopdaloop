use super::*;
use crate::audio_refs::{AudioSequence, ChunkRef, ReferenceBlock, RenderArena};

#[derive(Clone, Copy, Default)]
struct Route {
    wire: usize,
    port: usize,
}

#[derive(Clone, Copy)]
struct Operation {
    node: NodeId,
    first: usize,
    end: usize,
    output: usize,
    outputs: usize,
}

pub(super) struct Program {
    epoch: Option<Epoch>,
    bases: Box<[usize]>,
    wires: Box<[AudioSequence]>,
    operations: Vec<Operation>,
    inputs: Vec<Route>,
    outputs: Vec<Route>,
    host: usize,
}

impl Program {
    pub fn prepare(config: PreparedConfig) -> Self {
        let limits = config.limits();
        let mut operations = vec![
            Operation {
                node: NodeId::from_parts(0, 1).expect("initial node"),
                first: 0,
                end: 0,
                output: 0,
                outputs: 0
            };
            limits.max_nodes
        ];
        let mut inputs = vec![Route::default(); limits.max_edges];
        let mut outputs = vec![Route::default(); limits.max_edges];
        for op in &mut operations {
            *core::hint::black_box(op) = *op;
        }
        for route in inputs.iter_mut().chain(outputs.iter_mut()) {
            *core::hint::black_box(route) = Route::default();
        }
        operations.clear();
        inputs.clear();
        outputs.clear();
        Self {
            epoch: None,
            bases: vec![0; limits.max_nodes].into_boxed_slice(),
            wires: vec![AudioSequence::default(); limits.max_ports].into_boxed_slice(),
            operations,
            inputs,
            outputs,
            host: 0,
        }
    }

    fn compile<P>(
        &mut self,
        plan: &PreparedPlan,
        slots: &[Option<Slot<P>>],
        epoch: Epoch,
        hosts: usize,
    ) -> Result<(), Error> {
        self.operations.clear();
        self.inputs.clear();
        self.outputs.clear();
        let mut base = 0;
        for (index, slot) in slots.iter().enumerate() {
            self.bases[index] = base;
            if let Some(slot) = slot {
                base += slot.ports.audio_outputs;
            }
        }
        if base + hosts > self.wires.len() {
            return Err(Error::Capacity);
        }
        self.host = base;
        let wire = |edge: Connection, bases: &[usize]| {
            edge.source
                .node
                .map_or(base, |node| bases[node.slot() as usize])
                + edge.source.port as usize
        };
        let mut first = 0;
        for step in &plan.descriptor().steps {
            let slot = slots[step.node.slot() as usize]
                .as_ref()
                .ok_or(Error::InvalidId)?;
            if slot.ports.midi_inputs != 0 || slot.ports.midi_outputs != 0 {
                return Err(Error::InvalidGraph);
            }
            let begin = self.inputs.len();
            for &edge in &plan.descriptor().connections[first..step.input_end] {
                if edge.source.kind != PortType::Audio {
                    return Err(Error::InvalidGraph);
                }
                self.inputs.push(Route {
                    wire: wire(edge, &self.bases),
                    port: edge.destination.port as usize,
                });
            }
            self.operations.push(Operation {
                node: step.node,
                first: begin,
                end: self.inputs.len(),
                output: self.bases[step.node.slot() as usize],
                outputs: slot.ports.audio_outputs,
            });
            first = step.input_end;
        }
        for &edge in &plan.descriptor().connections[first..] {
            if edge.source.kind != PortType::Audio {
                return Err(Error::InvalidGraph);
            }
            self.outputs.push(Route {
                wire: wire(edge, &self.bases),
                port: edge.destination.port as usize,
            });
        }
        self.epoch = Some(epoch);
        Ok(())
    }

    fn source(
        &mut self,
        wire: usize,
        arena: &mut RenderArena,
        audio: &impl crate::host_buffer::AudioIo,
        start: usize,
        frames: usize,
    ) -> Result<AudioSequence, Error> {
        if wire >= self.host && self.wires[wire].silent() {
            let offset = arena.allocate(frames)?;
            arena
                .output(offset, frames)
                .copy_from_slice(&audio.input(wire - self.host)[start..start + frames]);
            arena.work.sample_reads += frames as u64;
            arena.work.sample_writes += frames as u64;
            self.wires[wire] = arena.transient(offset, frames);
        }
        Ok(self.wires[wire])
    }
}

impl<P: Processor<L, Q>, L: Planner, Q: ProcessorOperation> GraphRuntime<P, L, Q> {
    pub(super) fn run_graph_compact(
        &mut self,
        audio: &mut impl crate::host_buffer::AudioIo,
        start: usize,
        callback_start: Frame,
        quantum: crate::Quantum,
    ) -> Result<(), Error> {
        let mut program = self.compact.take().ok_or(Error::InvalidState)?;
        let Some(mut arena) = self.audio_arena.take() else {
            self.compact = Some(program);
            return Err(Error::InvalidState);
        };
        let result = (|| {
            arena.reset()?;
            let Some(plan) = &self.active else {
                return Ok(());
            };
            if program.epoch != Some(self.epoch) {
                program.compile(plan, &self.slots, self.epoch, self.host_sequences.len())?;
            }
            program.wires[program.host..program.host + self.host_sequences.len()]
                .fill(AudioSequence::default());
            let q = quantum.frames();
            let frame = audio.frame().checked_add(start)?;
            if self.profiling.needs_layout() {
                for op in &program.operations {
                    self.profiling.layout_node(u64::from(op.node.slot()));
                }
                self.profiling.layout_done();
            }
            let mut nodes = 0usize;
            let mut skipped = 0usize;
            let mut edges = program.outputs.len();
            for index in 0..program.operations.len() {
                let op = program.operations[index];
                program.wires[op.output..op.output + op.outputs].fill(AudioSequence::default());
                let slot = self.slots[op.node.slot() as usize]
                    .as_mut()
                    .ok_or(Error::InvalidId)?;
                let inputs_silent = program.inputs[op.first..op.end]
                    .iter()
                    .all(|route| route.wire < program.host && program.wires[route.wire].silent());
                let active = slot
                    .processor
                    .as_mut()
                    .ok_or(Error::InvalidId)?
                    .compact_active(crate::processor::CompactActivity {
                        node: op.node,
                        graph_epoch: self.epoch,
                        inputs_silent,
                        domain: &self.planner,
                    })?;
                if !active {
                    skipped += 1;
                    continue;
                }
                nodes += 1;
                edges += op.end - op.first;
                let buffers = &mut slot.buffers;
                buffers.input_sequences.fill(AudioSequence::default());
                buffers.input_mixed.fill(false);
                for index in op.first..op.end {
                    let route = program.inputs[index];
                    let source = program.source(route.wire, &mut arena, audio, start, q)?;
                    let input = &mut buffers.input_sequences[route.port];
                    if input.silent() {
                        *input = source;
                        arena.work.forwarded += 1;
                    } else if !source.silent() {
                        let offset = if buffers.input_mixed[route.port] {
                            let ChunkRef::Transient { offset, .. } = input.spans[0].chunk else {
                                return Err(Error::InvalidState);
                            };
                            offset
                        } else {
                            let offset = arena.allocate(q)?;
                            arena.accumulate(*input, offset, q, &self.resources)?;
                            *input = arena.transient(offset, q);
                            buffers.input_mixed[route.port] = true;
                            offset
                        };
                        arena.accumulate(source, offset, q, &self.resources)?;
                    }
                }
                let processor = slot.processor.as_mut().ok_or(Error::InvalidId)?;
                let handled = processor.process_references(&mut ReferenceBlock {
                    borrow_persistent: true,
                    node: op.node,
                    frame,
                    graph_epoch: self.epoch,
                    quantum,
                    inputs: &buffers.input_sequences,
                    outputs: &mut program.wires[op.output..op.output + op.outputs],
                    arena: &mut arena,
                    resources: &mut self.resources,
                    domain: &mut self.planner,
                })?;
                if !handled {
                    for (port, &sequence) in buffers.input_sequences.iter().enumerate() {
                        arena.read_into(
                            sequence,
                            &mut buffers.input[port * q..(port + 1) * q],
                            &self.resources,
                            false,
                        )?;
                    }
                    let offset = arena.allocate(op.outputs * q)?;
                    processor.process(&mut ProcessBlock {
                        node: op.node,
                        graph_epoch: self.epoch,
                        callback_start,
                        frame,
                        frames: q,
                        quantum,
                        audio: AudioPorts::new(
                            &buffers.input,
                            arena.output(offset, op.outputs * q),
                            q,
                            0,
                            q,
                        )?,
                        midi_inputs: &buffers.midi_input,
                        midi_outputs: &mut buffers.midi_output,
                        midi_forwarded: &mut buffers.midi_forwarded,
                        resources: self.resources.io(),
                        domain: &mut self.planner,
                    })?;
                    for (port, output) in program.wires[op.output..op.output + op.outputs]
                        .iter_mut()
                        .enumerate()
                    {
                        *output = arena.transient(offset + port * q, q);
                    }
                }
                processor.publish_references(&mut ReferenceBlock {
                    borrow_persistent: true,
                    node: op.node,
                    frame,
                    graph_epoch: self.epoch,
                    quantum,
                    inputs: &buffers.input_sequences,
                    outputs: &mut program.wires[op.output..op.output + op.outputs],
                    arena: &mut arena,
                    resources: &mut self.resources,
                    domain: &mut self.planner,
                })?;
                self.profiling.node_ready(u64::from(op.node.slot()));
            }
            for index in 0..program.outputs.len() {
                let route = program.outputs[index];
                let source = program.source(route.wire, &mut arena, audio, start, q)?;
                arena.read_into(
                    source,
                    &mut audio.output(route.port)[start..start + q],
                    &self.resources,
                    true,
                )?;
            }
            if let Some(count) = self.diagnostics.nodes_processed.checked_add(nodes) {
                self.diagnostics.nodes_processed = count;
            } else {
                self.diagnostics.counter_overflow = true;
                self.diagnostics.nodes_processed = usize::MAX;
            }
            if let Some(count) = self.diagnostics.nodes_skipped.checked_add(skipped) {
                self.diagnostics.nodes_skipped = count;
            } else {
                self.diagnostics.counter_overflow = true;
                self.diagnostics.nodes_skipped = usize::MAX;
            }
            if let Some(count) = self.diagnostics.connections_processed.checked_add(edges) {
                self.diagnostics.connections_processed = count;
            } else {
                self.diagnostics.counter_overflow = true;
                self.diagnostics.connections_processed = usize::MAX;
            }
            self.profiling.phase(rt_profiling::Phase::OutputRouting);
            Ok(())
        })();
        let unpin = arena.unpin(&mut self.resources);
        self.resources.add_audio_graph_work(arena.work);
        self.audio_arena = Some(arena);
        self.compact = Some(program);
        result.and(unpin)
    }
}
