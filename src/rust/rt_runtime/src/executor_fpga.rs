//! Execute a control-compiled node program against persistent render-owned state.
use super::*;
use crate::fpga::{Fixed, Job, Machine, NodeInstruction, Source, StateBlock};

/// The host delivers scheduled actions before entering this state processor.
/// The existing resolver supplies transition propagation; node/capture lifecycle,
/// job construction and quantum finalization execute under this owner.
struct StateProcessor<'a, P, L: Planner, Q: ProcessorOperation> {
    runtime: &'a mut GraphRuntime<P, L, Q>,
}

impl<P: Processor<L, Q>, L: Planner, Q: ProcessorOperation> GraphRuntime<P, L, Q> {
    pub(super) fn run_fpga_tick(
        &mut self,
        audio: &mut impl crate::host_buffer::AudioIo,
        start: usize,
        callback_start: Frame,
        quantum: crate::Quantum,
    ) -> Result<(), Error> {
        StateProcessor { runtime: self }.run(audio, start, callback_start, quantum)
    }
    pub fn fpga_program(&self) -> Option<&crate::fpga::NodeProgram> {
        self.active.as_ref()?.fpga_program()
    }
}

impl<P: Processor<L, Q>, L: Planner, Q: ProcessorOperation> StateProcessor<'_, P, L, Q> {
    fn run(
        &mut self,
        audio: &mut impl crate::host_buffer::AudioIo,
        start: usize,
        callback_start: Frame,
        quantum: crate::Quantum,
    ) -> Result<(), Error> {
        let runtime = &mut self.runtime;
        let frame = audio.frame().checked_add(start)?;
        let end = frame.checked_add(quantum.frames())?;
        runtime.planner.begin_tick(PlanContext {
            graph_epoch: runtime.epoch,
            graphs: GraphAccess::new(
                &mut runtime.graph_stages,
                &mut runtime.active,
                &mut runtime.epoch,
                true,
                &|candidate| activation_compatible(&runtime.slots, candidate),
                &mut runtime.recovery,
            ),
            callback_start,
            frame,
            ceiling: end,
            resources: runtime.resources.planner_io(frame, end),
            receipts: runtime.scheduler.receipts(frame),
        })?;
        if runtime.scheduler.recovery_pending() {
            return Err(Error::InvalidState);
        }
        runtime.profiling.phase(rt_profiling::Phase::StateSettled);
        runtime.profiling.graph_start(runtime.epoch.0, frame.0);
        let mut machine = runtime.fpga.take().ok_or(Error::InvalidState)?;
        let result = (|| {
            machine.reset()?;
            let Some(plan) = &runtime.active else {
                return Ok(());
            };
            let program = plan.fpga_program().ok_or(Error::InvalidConfig)?;
            let q = quantum.frames();
            for (port, samples) in machine.host_inputs.iter_mut().enumerate() {
                for (sample, &input) in samples.iter_mut().zip(&audio.input(port)[start..start + q])
                {
                    *sample = Fixed::from_f32(input);
                }
            }
            let silence = machine.writer.silence()?;
            machine.output_ids.fill(silence);
            for &instruction in &program.instructions {
                match instruction {
                    NodeInstruction::BeginNode(node) => {
                        machine.inputs[node.slot() as usize].fill(silence);
                        machine.outputs[node.slot() as usize].fill(silence);
                    }
                    NodeInstruction::RouteInput { source, node, port } => {
                        let source = resolve(&mut machine, source)?;
                        let old = machine.inputs[node.slot() as usize][port];
                        machine.inputs[node.slot() as usize][port] = if old == silence {
                            source
                        } else {
                            machine.writer.mix(old, source)?
                        };
                        increment(
                            &mut runtime.diagnostics.connections_processed,
                            &mut runtime.diagnostics.counter_overflow,
                        );
                    }
                    NodeInstruction::ProcessNode(node) => {
                        let slot = node.slot() as usize;
                        runtime.slots[slot]
                            .as_mut()
                            .ok_or(Error::InvalidId)?
                            .processor
                            .as_mut()
                            .ok_or(Error::InvalidId)?
                            .process_fpga_state(&mut StateBlock {
                                node,
                                frame,
                                graph_epoch: runtime.epoch,
                                quantum,
                                inputs: &machine.inputs[slot],
                                outputs: &mut machine.outputs[slot],
                                jobs: &mut machine.writer,
                                domain: &mut runtime.planner,
                            })?;
                    }
                    NodeInstruction::RouteOutput { source, port } => {
                        let source = resolve(&mut machine, source)?;
                        let old = machine.output_ids[port];
                        machine.output_ids[port] = if old == silence {
                            source
                        } else {
                            machine.writer.mix(old, source)?
                        };
                        increment(
                            &mut runtime.diagnostics.connections_processed,
                            &mut runtime.diagnostics.counter_overflow,
                        );
                    }
                    NodeInstruction::EmitOutput { port } => machine.writer.emit(Job::Output {
                        source: machine.output_ids[port],
                        port,
                    })?,
                }
            }
            machine.execute(&mut runtime.resources)?;
            // Feedback follows the same fixed node program; it never visits graph edges.
            for &instruction in &program.instructions {
                let NodeInstruction::ProcessNode(node) = instruction else {
                    continue;
                };
                let slot = node.slot() as usize;
                let entry = runtime.slots[slot].as_mut().ok_or(Error::InvalidId)?;
                let buffers = &mut entry.buffers;
                for (port, &id) in machine.inputs[slot].iter().enumerate() {
                    for (sample, value) in buffers.input[port * q..(port + 1) * q]
                        .iter_mut()
                        .zip(machine.jobs.buffer(id).map_err(|_| Error::InvalidState)?)
                    {
                        *sample = value.to_f32();
                    }
                }
                for (port, &id) in machine.outputs[slot].iter().enumerate() {
                    for (sample, value) in buffers.output[port * q..(port + 1) * q]
                        .iter_mut()
                        .zip(machine.jobs.buffer(id).map_err(|_| Error::InvalidState)?)
                    {
                        *sample = value.to_f32();
                    }
                }
                entry
                    .processor
                    .as_mut()
                    .ok_or(Error::InvalidId)?
                    .finish_fpga(&mut ProcessBlock {
                        node,
                        graph_epoch: runtime.epoch,
                        callback_start,
                        frame,
                        frames: q,
                        quantum,
                        audio: AudioPorts::new(&buffers.input, &mut buffers.output, q, 0, q)?,
                        midi_inputs: &buffers.midi_input,
                        midi_outputs: &mut buffers.midi_output,
                        midi_forwarded: &mut buffers.midi_forwarded,
                        resources: runtime.resources.io(),
                        domain: &mut runtime.planner,
                    })?;
                increment(
                    &mut runtime.diagnostics.nodes_processed,
                    &mut runtime.diagnostics.counter_overflow,
                );
            }
            for (port, output) in machine.host_outputs.iter().enumerate() {
                for (sample, value) in audio.output(port)[start..start + q]
                    .iter_mut()
                    .zip(output.iter())
                {
                    *sample = value.to_f32();
                }
            }
            Ok(())
        })();
        runtime.fpga = Some(machine);
        result?;
        runtime.planner.finish_tick(FinishContext {
            interval: Interval { start: frame, end },
            resources: runtime.resources.planner_io(end, end),
            receipts: runtime.scheduler.receipts(end),
        })
    }
}

fn resolve(machine: &mut Machine, source: Source) -> Result<crate::fpga::BufferId, Error> {
    match source {
        Source::Node { node, port } => machine
            .outputs
            .get(node.slot() as usize)
            .and_then(|ports| ports.get(port))
            .copied()
            .ok_or(Error::InvalidGraph),
        Source::Host(port) => {
            if machine.host_ids[port].is_none() {
                machine.host_ids[port] = Some(machine.writer.input(port)?);
            }
            machine.host_ids[port].ok_or(Error::InvalidState)
        }
    }
}
