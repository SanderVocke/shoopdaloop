//! Allocation-free construction of the active graph from a model.

use crate::model::{Model, Strip};
use crate::pool::Pool;
use rt_runtime::graph::{Connection, Endpoint, NodeStep, PortType, PreparedPlan};
use rt_runtime::planner::{DecisionBinding, PlannerSpec, PlanningStep};
use rt_runtime::{Error, NodeId};

pub(crate) struct Shape {
    pub channels: usize,
    pub master_channels: usize,
    pub host_audio_outputs: usize,
}

/// Prepared scratch for sorting one node's input edges and the loop bindings.
pub(crate) struct Builder {
    edges: Box<[Connection]>,
    bindings: Box<[DecisionBinding]>,
}

fn audio(node: NodeId, port: usize) -> Endpoint {
    Endpoint::node(node, PortType::Audio, port as u32)
}
fn midi(node: NodeId, port: usize) -> Endpoint {
    Endpoint::node(node, PortType::Midi, port as u32)
}
fn bits(mask: u64) -> impl Iterator<Item = usize> {
    (0..64).filter(move |bit| mask & (1 << bit) != 0)
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
    pub(crate) fn new(max_edges: usize, max_loops: usize) -> Self {
        let edge = link(
            Endpoint::host(PortType::Audio, 0),
            Endpoint::host(PortType::Audio, 0),
        );
        let binding = DecisionBinding {
            planner: rt_runtime::PlannerId::from_parts(0, 1).expect("fixed identity"),
            consumer: NodeId::from_parts(0, 1).expect("fixed identity"),
            decision_slot: 0,
        };
        Self {
            edges: vec![edge; max_edges].into_boxed_slice(),
            bindings: vec![binding; 2 * max_loops].into_boxed_slice(),
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
        planner: PlannerSpec,
    ) -> Result<(), Error> {
        plan.begin_descriptor()?;
        plan.set_planning_step(Some(PlanningStep {
            planner,
            before_step: 0,
        }));
        let channels = shape.channels;
        let mut end = 0;
        let push = |edges: &mut [Connection], count: &mut usize, edge| -> Result<(), Error> {
            *edges.get_mut(*count).ok_or(Error::Capacity)? = edge;
            *count += 1;
            Ok(())
        };
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
            }
            self.step(plan, node, count, &mut end)?;
        }
        let mut bindings = 0;
        for (slot, entry) in model.loops.iter().enumerate() {
            let Some(entry) = entry else { continue };
            let strip_slot = entry.strip.0.index as usize;
            let strip = model.strips[strip_slot]
                .as_ref()
                .ok_or(Error::InvalidState)?;
            let input = pool.strip_input(strip_slot);
            let capture = pool.capture(slot);
            let mut count = 0;
            for channel in 0..strip.shape.audio_inputs as usize {
                push(
                    &mut self.edges,
                    &mut count,
                    link(audio(input, channel), audio(capture, channel)),
                )?;
            }
            if strip.shape.midi_input {
                push(
                    &mut self.edges,
                    &mut count,
                    link(midi(input, 0), midi(capture, 0)),
                )?;
            }
            self.step(plan, capture, count, &mut end)?;
            let playback = pool.playback(slot);
            self.step(plan, playback, 0, &mut end)?;
            let fader = pool.fader(slot);
            let mut count = 0;
            for channel in 0..channels {
                push(
                    &mut self.edges,
                    &mut count,
                    link(audio(playback, channel), audio(fader, channel)),
                )?;
            }
            self.step(plan, fader, count, &mut end)?;
            for consumer in [playback, capture] {
                *self.bindings.get_mut(bindings).ok_or(Error::Capacity)? = DecisionBinding {
                    planner: planner.id,
                    consumer,
                    decision_slot: slot,
                };
                bindings += 1;
            }
        }
        for (slot, strip) in model.strips.iter().enumerate() {
            let Some(strip) = strip else { continue };
            let input = pool.strip_input(slot);
            let node = pool.strip_output(slot);
            let mut count = 0;
            for channel in 0..strip.shape.audio_outputs as usize {
                let source = source_channel(strip, channel);
                if (strip.shape.audio_inputs as usize) > 0 {
                    push(
                        &mut self.edges,
                        &mut count,
                        link(audio(input, channels + source), audio(node, channel)),
                    )?;
                }
                for (loop_slot, entry) in model.loops.iter().enumerate() {
                    if entry.is_some_and(|entry| entry.strip == strip.id) {
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
                    if entry.is_some_and(|entry| entry.strip == strip.id) {
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
        let master = pool.master();
        let mut count = 0;
        for (slot, strip) in model.strips.iter().enumerate() {
            let Some(strip) = strip else { continue };
            let node = pool.strip_output(slot);
            let outputs = strip.shape.audio_outputs as usize;
            for bank in 0..2 {
                if !strip.outputs.master[bank] {
                    continue;
                }
                for channel in 0..outputs {
                    for target in 0..shape.master_channels {
                        let feeds = if outputs == 1 {
                            true
                        } else {
                            channel % shape.master_channels == target
                        };
                        if feeds {
                            push(
                                &mut self.edges,
                                &mut count,
                                link(
                                    audio(node, bank * channels + channel),
                                    audio(master, target),
                                ),
                            )?;
                        }
                    }
                }
            }
        }
        self.step(plan, master, count, &mut end)?;
        let mut count = 0;
        for (slot, strip) in model.strips.iter().enumerate() {
            let Some(strip) = strip else { continue };
            let node = pool.strip_output(slot);
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
        for channel in 0..shape.master_channels.min(shape.host_audio_outputs) {
            push(
                &mut self.edges,
                &mut count,
                link(
                    audio(master, channel),
                    Endpoint::host(PortType::Audio, channel as u32),
                ),
            )?;
        }
        let edges = &mut self.edges[..count];
        edges.sort_unstable();
        for edge in edges.iter() {
            plan.push_connection(*edge)?;
        }
        let bindings = &mut self.bindings[..bindings];
        bindings.sort_unstable_by_key(|binding| binding.consumer);
        for binding in bindings.iter() {
            plan.push_binding(*binding)?;
        }
        Ok(())
    }
}
