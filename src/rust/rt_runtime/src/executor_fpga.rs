use super::*;
use crate::fpga::{CompileBlock, Fixed, Job};

impl<P: Processor<L, Q>, L: Planner, Q: ProcessorOperation> GraphRuntime<P, L, Q> {
    pub(super) fn run_graph_fpga(
        &mut self,
        audio: &mut impl crate::host_buffer::AudioIo,
        _midi: &mut MidiBlock<'_>,
        start: usize,
        callback_start: Frame,
        quantum: crate::Quantum,
    ) -> Result<(), Error> {
        let mut machine = self.fpga.take().ok_or(Error::InvalidState)?;
        let result = (|| {
            machine.reset();
            let Some(plan) = &self.active else {
                return Ok(());
            };
            let frame = audio.frame().checked_add(start)?;
            let q = quantum.frames();
            for (port, samples) in machine.host_inputs.iter_mut().enumerate() {
                for (sample, &input) in samples.iter_mut().zip(&audio.input(port)[start..start + q])
                {
                    *sample = Fixed::from_f32(input);
                }
            }
            let silence = machine.compiler.silence()?;
            let mut first = 0;
            for step in &plan.descriptor().steps {
                let slot = step.node.slot() as usize;
                machine.inputs[slot].fill(silence);
                machine.outputs[slot].fill(silence);
                for edge in &plan.descriptor().connections[first..step.input_end] {
                    if edge.source.kind != PortType::Audio {
                        return Err(Error::InvalidConfig);
                    }
                    let source = if let Some(source) = edge.source.node {
                        machine.outputs[source.slot() as usize][edge.source.port as usize]
                    } else {
                        let port = edge.source.port as usize;
                        if machine.host_ids[port].is_none() {
                            machine.host_ids[port] = Some(machine.compiler.input(port)?);
                        }
                        machine.host_ids[port].ok_or(Error::InvalidState)?
                    };
                    let destination = &mut machine.inputs[slot][edge.destination.port as usize];
                    *destination = if *destination == silence {
                        source
                    } else {
                        machine.compiler.mix(*destination, source)?
                    };
                }
                self.slots[slot]
                    .as_mut()
                    .ok_or(Error::InvalidId)?
                    .processor
                    .as_mut()
                    .ok_or(Error::InvalidId)?
                    .compile_fpga(&mut CompileBlock {
                        node: step.node,
                        frame,
                        graph_epoch: self.epoch,
                        quantum,
                        inputs: &machine.inputs[slot],
                        outputs: &mut machine.outputs[slot],
                        compiler: &mut machine.compiler,
                        domain: &mut self.planner,
                    })?;
                first = step.input_end;
            }
            // Accumulate all contributors before assigning each physical output once.
            for port in 0..machine.host_outputs.len() {
                let mut source = silence;
                for edge in &plan.descriptor().connections[first..] {
                    if edge.source.kind != PortType::Audio {
                        return Err(Error::InvalidConfig);
                    }
                    if edge.destination.port as usize != port {
                        continue;
                    }
                    let input = if let Some(node) = edge.source.node {
                        machine.outputs[node.slot() as usize][edge.source.port as usize]
                    } else {
                        let input = edge.source.port as usize;
                        if machine.host_ids[input].is_none() {
                            machine.host_ids[input] = Some(machine.compiler.input(input)?);
                        }
                        machine.host_ids[input].ok_or(Error::InvalidState)?
                    };
                    source = if source == silence {
                        input
                    } else {
                        machine.compiler.mix(source, input)?
                    };
                }
                machine.compiler.emit(Job::Output { source, port })?;
            }
            machine.execute(&mut self.resources)?;
            for step in &plan.descriptor().steps {
                let slot = step.node.slot() as usize;
                let entry = self.slots[slot].as_mut().ok_or(Error::InvalidId)?;
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
                        node: step.node,
                        graph_epoch: self.epoch,
                        callback_start,
                        frame,
                        frames: q,
                        quantum,
                        audio: AudioPorts::new(&buffers.input, &mut buffers.output, q, 0, q)?,
                        midi_inputs: &buffers.midi_input,
                        midi_outputs: &mut buffers.midi_output,
                        midi_forwarded: &mut buffers.midi_forwarded,
                        resources: self.resources.io(),
                        domain: &mut self.planner,
                    })?;
                increment(
                    &mut self.diagnostics.nodes_processed,
                    &mut self.diagnostics.counter_overflow,
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
        self.fpga = Some(machine);
        result
    }
}
