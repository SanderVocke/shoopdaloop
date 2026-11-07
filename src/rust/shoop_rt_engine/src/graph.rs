//! Allocation-free construction of the active graph from a model.

use alloc::boxed::Box;
use alloc::vec;

use crate::model::{Model, Strip};
use crate::pool::Pool;
use rt_runtime::graph::{Connection, Endpoint, NodeStep, PortType, PreparedPlan};
use rt_runtime::{Error, NodeId};

pub(crate) struct Shape {
    pub channels: usize,
    pub master_channels: usize,
    pub processor_channels: usize,
}

/// Prepared scratch for sorting one node's input edges and the loop bindings.
pub(crate) struct Builder {
    edges: Box<[Connection]>,
}

fn audio(node: NodeId, port: usize) -> Endpoint {
    Endpoint::node(node, PortType::Audio, port as u32)
}
fn midi(node: NodeId, port: usize) -> Endpoint {
    Endpoint::node(node, PortType::Midi, port as u32)
}
fn bits(mask: crate::model::HostMask) -> impl Iterator<Item = usize> {
    mask.bits()
}
fn link(source: Endpoint, destination: Endpoint) -> Connection {
    Connection {
        source,
        destination,
    }
}

/// The input channel feeding output channel `channel` of a strip: inputs repeat
/// across wider outputs.
pub(crate) fn source_channel(strip: &Strip, channel: usize) -> usize {
    let inputs = strip.shape.audio_inputs as usize;
    if inputs == 0 {
        channel
    } else {
        channel % inputs
    }
}

impl Builder {
    pub(crate) fn new(max_edges: usize) -> Self {
        let edge = link(
            Endpoint::host(PortType::Audio, 0),
            Endpoint::host(PortType::Audio, 0),
        );
        Self {
            edges: vec![edge; max_edges].into_boxed_slice(),
        }
    }

    fn step(
        &mut self,
        plan: &mut PreparedPlan,
        node: NodeId,
        count: usize,
        input_end: &mut usize,
    ) -> Result<(), Error> {
        let edges = &mut self.edges[..count];
        edges.sort_unstable();
        for edge in edges.iter() {
            plan.push_connection(*edge)?;
        }
        *input_end += count;
        plan.push_step(NodeStep {
            node,
            input_end: *input_end,
        })
    }

    pub(crate) fn build(
        &mut self,
        plan: &mut PreparedPlan,
        model: &Model,
        pool: &Pool,
        shape: &Shape,
    ) -> Result<(), Error> {
        plan.begin_descriptor()?;
        let channels = shape.channels;
        let mut end = 0;
        let push = |edges: &mut [Connection], count: &mut usize, edge| -> Result<(), Error> {
            *edges.get_mut(*count).ok_or(Error::Capacity)? = edge;
            *count += 1;
            Ok(())
        };
        for (slot, entry) in model.loops.iter().enumerate() {
            if entry.is_some_and(|entry| entry.program.is_none()) {
                self.step(plan, pool.playback(slot), 0, &mut end)?;
            }
        }
        for (slot, entry) in model.loops.iter().enumerate() {
            if entry.is_none_or(|entry| entry.program.is_some()) {
                continue;
            }
            let mut count = 0;
            for channel in 0..2 * channels {
                push(
                    &mut self.edges,
                    &mut count,
                    link(
                        audio(pool.playback(slot), channel),
                        audio(pool.fader(slot), channel),
                    ),
                )?;
            }
            self.step(plan, pool.fader(slot), count, &mut end)?;
        }
        for (slot, strip) in model.strips.iter().enumerate() {
            let Some(strip) = strip else { continue };
            let node = pool.strip_input(slot);
            let mut count = 0;
            for bank in 0..2 {
                for channel in 0..strip.shape.audio_inputs as usize {
                    for host in bits(strip.inputs.masks[bank][channel]) {
                        push(
                            &mut self.edges,
                            &mut count,
                            link(
                                Endpoint::host(PortType::Audio, host as u32),
                                audio(node, bank * channels + channel),
                            ),
                        )?;
                    }
                }
            }
            if strip.shape.midi_input {
                for host in bits(strip.midi_inputs) {
                    push(
                        &mut self.edges,
                        &mut count,
                        link(Endpoint::host(PortType::Midi, host as u32), midi(node, 0)),
                    )?;
                }
                if strip.processor.is_some() {
                    for host in bits(model.global_midi) {
                        push(
                            &mut self.edges,
                            &mut count,
                            link(Endpoint::host(PortType::Midi, host as u32), midi(node, 2)),
                        )?;
                    }
                }
            }
            if strip
                .processor
                .is_some_and(|owner| owner.kind == shoop_rt_protocol::ProcessorKind::External)
            {
                for bank in 0..2 {
                    for channel in 0..strip.shape.audio_outputs as usize {
                        for host in bits(strip.returns.masks[bank][channel]) {
                            push(
                                &mut self.edges,
                                &mut count,
                                link(
                                    Endpoint::host(PortType::Audio, host as u32),
                                    audio(node, (3 + bank) * channels + channel),
                                ),
                            )?;
                        }
                    }
                }
            }
            if strip.processor.is_some() {
                for (loop_slot, entry) in model.loops.iter().enumerate() {
                    if entry.is_some_and(|entry| {
                        entry.strip == Some(strip.id) && entry.program.is_none()
                    }) {
                        for channel in 0..strip.shape.audio_inputs as usize {
                            push(
                                &mut self.edges,
                                &mut count,
                                link(
                                    audio(pool.fader(loop_slot), channels + channel),
                                    audio(node, 2 * channels + channel),
                                ),
                            )?;
                        }
                        if strip.shape.midi_input {
                            push(
                                &mut self.edges,
                                &mut count,
                                link(midi(pool.playback(loop_slot), 0), midi(node, 1)),
                            )?;
                        }
                    }
                }
            }
            self.step(plan, node, count, &mut end)?;
        }
        for slot in 0..pool.processors() {
            if model.buses.iter().flatten().any(|bus| {
                bus.processor
                    .and_then(|owner| owner.slot)
                    .is_some_and(|owner| owner.index as usize == slot)
            }) {
                continue;
            }
            let mut count = 0;
            if let Some((strip_slot, strip)) = model
                .strips
                .iter()
                .enumerate()
                .filter_map(|(slot, strip)| strip.as_ref().map(|s| (slot, s)))
                .find(|(_, strip)| {
                    strip
                        .processor
                        .and_then(|owner| owner.slot)
                        .is_some_and(|owner| owner.index as usize == slot)
                })
            {
                for bank in 0..2 {
                    for channel in 0..strip.shape.audio_inputs as usize {
                        push(
                            &mut self.edges,
                            &mut count,
                            link(
                                audio(
                                    pool.strip_input(strip_slot),
                                    (2 + bank) * channels + channel,
                                ),
                                audio(pool.processor(slot), channel),
                            ),
                        )?;
                    }
                }
                push(
                    &mut self.edges,
                    &mut count,
                    link(
                        midi(pool.strip_input(strip_slot), 2),
                        midi(pool.processor(slot), 0),
                    ),
                )?;
                for (source, target) in [(0, 1), (3, 2)] {
                    push(
                        &mut self.edges,
                        &mut count,
                        link(
                            midi(pool.strip_input(strip_slot), source),
                            midi(pool.processor(slot), target),
                        ),
                    )?;
                }
            }
            self.step(plan, pool.processor(slot), count, &mut end)?;
        }
        for (slot, entry) in model.loops.iter().enumerate() {
            let Some(entry) = entry.filter(|entry| entry.program.is_none()) else {
                continue;
            };
            let strip_slot = entry.strip.ok_or(Error::InvalidState)?.0.index as usize;
            let strip = model.strips[strip_slot]
                .as_ref()
                .ok_or(Error::InvalidState)?;
            let input = pool.strip_input(strip_slot);
            let capture = pool.capture(slot);
            let mut count = 0;
            if let Some(owner) = strip.processor {
                let prepared_width = if owner.kind == shoop_rt_protocol::ProcessorKind::OxiSynth {
                    2
                } else {
                    shape.processor_channels
                };
                for channel in 0..strip.shape.audio_outputs as usize {
                    let source = owner
                        .slot
                        .map_or(audio(input, 4 * channels + channel), |slot| {
                            audio(
                                pool.processor(slot.index as usize),
                                prepared_width + channel,
                            )
                        });
                    push(
                        &mut self.edges,
                        &mut count,
                        link(source, audio(capture, channel)),
                    )?;
                }
                for channel in 0..strip.shape.audio_inputs as usize {
                    push(
                        &mut self.edges,
                        &mut count,
                        link(audio(input, channel), audio(capture, channels + channel)),
                    )?;
                }
            } else {
                for channel in 0..strip.shape.audio_inputs as usize {
                    push(
                        &mut self.edges,
                        &mut count,
                        link(audio(input, channel), audio(capture, channel)),
                    )?;
                }
            }
            if strip.shape.midi_input {
                push(
                    &mut self.edges,
                    &mut count,
                    link(midi(input, 0), midi(capture, 0)),
                )?;
            }
            self.step(plan, capture, count, &mut end)?;
        }
        for (slot, strip) in model.strips.iter().enumerate() {
            let Some(strip) = strip else { continue };
            let input = pool.strip_input(slot);
            let node = pool.strip_output(slot);
            let mut count = 0;
            for channel in 0..strip.shape.audio_outputs as usize {
                let source = if strip.processor.is_some() {
                    channel
                } else {
                    source_channel(strip, channel)
                };
                if strip.processor.is_none() && (strip.shape.audio_inputs as usize) > 0 {
                    push(
                        &mut self.edges,
                        &mut count,
                        link(audio(input, channels + source), audio(node, channel)),
                    )?;
                }
                if let Some(owner) = strip.processor {
                    let wet = owner
                        .slot
                        .map_or(audio(input, 4 * channels + channel), |slot| {
                            audio(pool.processor(slot.index as usize), channel)
                        });
                    push(
                        &mut self.edges,
                        &mut count,
                        link(wet, audio(node, channels + channel)),
                    )?;
                }
                for (loop_slot, entry) in model.loops.iter().enumerate() {
                    if entry.is_some_and(|entry| {
                        entry.strip == Some(strip.id) && entry.program.is_none()
                    }) {
                        push(
                            &mut self.edges,
                            &mut count,
                            link(audio(pool.fader(loop_slot), source), audio(node, channel)),
                        )?;
                    }
                }
            }
            if strip.shape.midi_output {
                if strip.shape.midi_input {
                    push(
                        &mut self.edges,
                        &mut count,
                        link(midi(input, 1), midi(node, 0)),
                    )?;
                }
                for (loop_slot, entry) in model.loops.iter().enumerate() {
                    if entry.is_some_and(|entry| {
                        entry.strip == Some(strip.id) && entry.program.is_none()
                    }) {
                        push(
                            &mut self.edges,
                            &mut count,
                            link(midi(pool.playback(loop_slot), 0), midi(node, 0)),
                        )?;
                    }
                }
            }
            self.step(plan, node, count, &mut end)?;
        }
        for (bus_slot, bus) in model.buses.iter().enumerate() {
            let Some(bus) = bus else { continue };
            let node = pool.bus_input(bus_slot);
            let mut count = 0;
            for (slot, strip) in model.strips.iter().enumerate() {
                let Some(strip) = strip else { continue };
                for bank in 0..2 {
                    for channel in 0..strip.shape.audio_outputs as usize {
                        let routes = strip.outputs.buses[bank][channel];
                        for target in 0..bus.channels as usize {
                            if routes
                                & (1u64
                                    << (bus_slot * shoop_rt_protocol::MAX_STRIP_CHANNELS + target))
                                != 0
                            {
                                push(
                                    &mut self.edges,
                                    &mut count,
                                    link(
                                        audio(pool.strip_output(slot), bank * channels + channel),
                                        audio(node, target),
                                    ),
                                )?;
                            }
                        }
                    }
                }
            }
            self.step(plan, node, count, &mut end)?;
            if let Some(owner) = bus.processor.and_then(|owner| owner.slot) {
                let processor = pool.processor(owner.index as usize);
                let mut count = 0;
                for channel in 0..bus.channels as usize {
                    push(
                        &mut self.edges,
                        &mut count,
                        link(audio(node, channel), audio(processor, channel)),
                    )?;
                }
                for host in bits(model.global_midi) {
                    push(
                        &mut self.edges,
                        &mut count,
                        link(
                            Endpoint::host(PortType::Midi, host as u32),
                            midi(processor, 2),
                        ),
                    )?;
                }
                self.step(plan, processor, count, &mut end)?;
            }
            let mut count = 0;
            for channel in 0..bus.channels as usize {
                let source = bus
                    .processor
                    .and_then(|owner| owner.slot)
                    .map_or(node, |owner| pool.processor(owner.index as usize));
                push(
                    &mut self.edges,
                    &mut count,
                    link(audio(source, channel), audio(pool.bus(bus_slot), channel)),
                )?;
            }
            self.step(plan, pool.bus(bus_slot), count, &mut end)?;
        }
        self.step(plan, pool.reader(), 0, &mut end)?;
        let mut count = 0;
        for (slot, strip) in model.strips.iter().enumerate() {
            let Some(strip) = strip else { continue };
            let node = pool.strip_output(slot);
            if strip
                .processor
                .is_some_and(|owner| owner.kind == shoop_rt_protocol::ProcessorKind::External)
            {
                for bank in 0..2 {
                    for channel in 0..strip.shape.audio_inputs as usize {
                        for host in bits(strip.sends.masks[bank][channel]) {
                            push(
                                &mut self.edges,
                                &mut count,
                                link(
                                    audio(pool.strip_input(slot), (2 + bank) * channels + channel),
                                    Endpoint::host(PortType::Audio, host as u32),
                                ),
                            )?;
                        }
                    }
                }
                for host in bits(strip.midi_send) {
                    push(
                        &mut self.edges,
                        &mut count,
                        link(
                            midi(pool.strip_input(slot), 2),
                            Endpoint::host(PortType::Midi, host as u32),
                        ),
                    )?;
                }
            }
            for bank in 0..2 {
                for channel in 0..strip.shape.audio_outputs as usize {
                    for host in bits(strip.outputs.masks[bank][channel]) {
                        push(
                            &mut self.edges,
                            &mut count,
                            link(
                                audio(node, bank * channels + channel),
                                Endpoint::host(PortType::Audio, host as u32),
                            ),
                        )?;
                    }
                }
            }
            if strip.shape.midi_output {
                for host in bits(strip.midi_outputs) {
                    push(
                        &mut self.edges,
                        &mut count,
                        link(midi(node, 0), Endpoint::host(PortType::Midi, host as u32)),
                    )?;
                }
            }
        }
        for (slot, bus) in model.buses.iter().enumerate() {
            let Some(bus) = bus else { continue };
            for bank in 0..2 {
                for channel in 0..bus.channels as usize {
                    for host in bits(bus.outputs.masks[bank][channel]) {
                        push(
                            &mut self.edges,
                            &mut count,
                            link(
                                audio(pool.bus(slot), bank * shape.master_channels + channel),
                                Endpoint::host(PortType::Audio, host as u32),
                            ),
                        )?;
                    }
                }
            }
        }
        let edges = &mut self.edges[..count];
        edges.sort_unstable();
        for edge in edges.iter() {
            plan.push_connection(*edge)?;
        }
        Ok(())
    }
}
