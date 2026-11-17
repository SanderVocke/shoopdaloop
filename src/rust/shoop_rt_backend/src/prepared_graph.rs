//! Control-owned graph preparation and explicit execution-program handoff.
use crate::mirror::PortTarget;
use crate::*;
use shoop_backend::{BackendMutationCompletion, BackendTrackTopology, MASTER_BUS_ID};
use shoop_rt_protocol::{
    immediate::{Batch, Code, Command, GraphLoop, COMMANDS},
    Direction, HostPort, HostPortId, HostPortKind, Name, Outcome, PortKind, Refusal, Request,
};

#[derive(Default)]
pub(crate) struct PreparedGraph {
    pub active: bool,
    pub pending: Option<u64>,
    pub revision: u64,
    pub dirty: bool,
    pub committed: Option<Mirror>,
    outgoing: alloc::collections::VecDeque<Batch>,
}
impl<C: Carrier, K: EngineClock> RtBackend<C, K> {
    pub(crate) fn prepared_graph_welcome(&mut self) {
        self.prepared_graph = Some(PreparedGraph::default());
        self.link = Link::Ready;
        self.telemetry_fresh = true;
        self.mirror.host_ports = (0..3)
            .map(|index| HostPort {
                id: HostPortId(index),
                kind: HostPortKind::Audio,
                direction: if index == 0 {
                    Direction::Input
                } else {
                    Direction::Output
                },
                name: Name::new(if index == 0 {
                    "reference_in"
                } else if index == 1 {
                    "reference_left"
                } else {
                    "reference_right"
                })
                .unwrap(),
                preferred: index != 0,
            })
            .collect();
        self.prepared_graph.as_mut().unwrap().committed = Some(self.mirror.clone());
    }
    pub(crate) fn structure_mutable(&self) -> Result<()> {
        if self
            .prepared_graph
            .as_ref()
            .is_some_and(|g| g.pending.is_some())
        {
            return Err(anyhow!("a program ownership handoff is pending"));
        }
        if self.prepared_graph.as_ref().is_some_and(|g| {
            g.active
                && !self
                    .capabilities
                    .contains(shoop_rt_protocol::Capabilities::LIVE_PROGRAMS)
        }) {
            return Err(anyhow!(
                "the prepared graph is immutable; restart the reference host to change it"
            ));
        }
        Ok(())
    }
    pub(crate) fn send_prepared_graph_requests(&mut self) -> usize {
        let draft = self
            .prepared_graph
            .as_ref()
            .is_some_and(|g| g.pending.is_none());
        let mut count = 0;
        while let Some(pending) = self.requests.outgoing.pop_front() {
            let supported = draft
                && match pending.request {
                    Request::CreateStrip { shape, .. } => {
                        shape.audio_inputs == 1
                            && shape.audio_outputs == 1
                            && !shape.midi_input
                            && !shape.midi_output
                    }
                    Request::CreateLoop { .. }
                    | Request::RemoveStrip { .. }
                    | Request::LoopSync { .. }
                    | Request::Connect { .. }
                    | Request::Route { .. } => true,
                    Request::SetSmoothing { frames } => frames == 0,
                    Request::LoopGain { .. } => !self.prepared_graph.as_ref().unwrap().active,
                    Request::CancelBulk => true,
                    _ => false,
                };
            if supported
                && !matches!(
                    pending.request,
                    Request::SetSmoothing { .. } | Request::CancelBulk
                )
            {
                self.prepared_graph.as_mut().unwrap().dirty = true;
            }
            self.answered(
                pending,
                if supported {
                    Outcome::Done
                } else {
                    Outcome::Refused(Refusal::Unsupported)
                },
            );
            count += 1;
        }
        count
    }
    pub(crate) fn install_prepared_graph(&mut self) -> Result<()> {
        self.ready()?;
        self.structure_mutable()?;
        self.pump();
        let state = self
            .prepared_graph
            .as_ref()
            .ok_or_else(|| anyhow!("this backend does not support prepared graph installation"))?;
        if state.pending.is_some() || !self.requests.is_empty() {
            return Err(anyhow!("initial layout has not settled"));
        }
        if state.active && !state.dirty {
            return Err(anyhow!("the prepared layout has not changed"));
        }
        if self.mirror.loops.is_empty() || self.mirror.buses.len() != 1 {
            return Err(anyhow!(
                "prepared reference requires mono loops and the Master bus"
            ));
        }
        let mut commands = Vec::new();
        commands.push(Command::BeginGraph {
            quantum: self.limits().quantum,
            loops: self.mirror.loops.len() as u32,
        });
        for entry in self.mirror.loops.values() {
            let track = &self.mirror.tracks[&entry
                .track
                .ok_or_else(|| anyhow!("composites are unsupported"))?];
            if !matches!(
                track.topology,
                BackendTrackTopology::Direct {
                    audio_channels: 1,
                    midi: false
                }
            ) || !entry.confirmed
                || entry.program.is_some()
                || entry.balance != 0.0
                || track.removing
                || !track.confirmed
                || track.controls.output_gain_db != 0.0
                || track.controls.output_balance != 0.0
                || track.controls.output_muted
                || track.controls.input_gain_db != 0.0
                || track.controls.input_balance != 0.0
                || track.controls.input_monitoring
                || track.automatic_enabled
            {
                return Err(anyhow!("prepared reference requires mono primitive loops"));
            }
            let mut input = None;
            let mut output = false;
            for &port in &track.ports {
                match self.mirror.ports[&port].target {
                    PortTarget::Strip {
                        kind: PortKind::AudioIn,
                        index: 0,
                        ..
                    } => {
                        if self
                            .mirror
                            .links
                            .iter()
                            .any(|l| l.application_port_id == port)
                        {
                            input = Some(0);
                        }
                    }
                    PortTarget::Strip {
                        kind: PortKind::AudioOut,
                        index: 0,
                        ..
                    } => {
                        let channels = &self.mirror.buses[&MASTER_BUS_ID].channels;
                        let routed = channels
                            .iter()
                            .filter(|c| {
                                self.mirror.routes.iter().any(|r| {
                                    r.source_port_id == port && r.destination_channel_id == c.id
                                })
                            })
                            .count();
                        if routed != 0 && routed != 2 {
                            return Err(anyhow!(
                                "mono outputs must route to both Master channels or neither"
                            ));
                        }
                        output = routed == 2;
                    }
                    _ => {}
                }
            }
            commands.push(Command::ConfigureLoop(GraphLoop {
                slot: entry.slot.0.index,
                generation: entry.slot.0.generation,
                source: entry.sync.map(|s| self.mirror.loops[&s].slot.0.index),
                input,
                output,
                gain: entry.gain,
                repeat_sync: entry.repeat_sync,
            }));
        }
        let master = &self.mirror.buses[&MASTER_BUS_ID];
        if master.gain_db != 0.0 || master.balance != 0.0 || master.muted {
            return Err(anyhow!("only default Master controls are supported"));
        }
        for link in &self.mirror.links {
            let valid = match self.mirror.ports[&link.application_port_id].target {
                PortTarget::Strip {
                    kind: PortKind::AudioIn,
                    index: 0,
                    ..
                } => link.host_port_id == "reference_in",
                PortTarget::Bus(bus, channel) if bus == MASTER_BUS_ID => {
                    link.host_port_id
                        == if channel == 0 {
                            "reference_left"
                        } else {
                            "reference_right"
                        }
                }
                _ => false,
            };
            if !valid {
                return Err(anyhow!("unsupported prepared graph host connection"));
            }
        }
        if !master.channels.iter().enumerate().all(|(i, c)| {
            self.mirror.links.iter().any(|l| {
                l.application_port_id == c.output_port_id
                    && l.host_port_id
                        == if i == 0 {
                            "reference_left"
                        } else {
                            "reference_right"
                        }
            })
        }) {
            return Err(anyhow!(
                "prepared reference requires stereo Master host outputs"
            ));
        }
        commands.push(Command::ActivateGraph);
        let revision = state.revision;
        let mut outgoing = commands
            .chunks(COMMANDS)
            .map(|chunk| Batch::new(revision, revision, chunk).unwrap())
            .collect::<alloc::collections::VecDeque<_>>();
        self.client
            .immediate(*outgoing.front().unwrap())
            .map_err(|e| anyhow!("graph submission failed: {e:?}"))?;
        outgoing.pop_front();
        let sequence = self.requests.reserve_mutation();
        let graph = self.prepared_graph.as_mut().unwrap();
        graph.pending = Some(sequence);
        graph.outgoing = outgoing;
        Ok(())
    }
    pub(crate) fn send_prepared_graph_chunks(&mut self) -> usize {
        if self.link != Link::Ready {
            return 0;
        }
        let Some(graph) = &mut self.prepared_graph else {
            return 0;
        };
        let Some(batch) = graph.outgoing.front().copied() else {
            return 0;
        };
        match self.client.immediate(batch) {
            Ok(()) => {
                graph.outgoing.pop_front();
                1
            }
            Err(shoop_rt_transport::client::ClientError::Carrier(
                shoop_rt_transport::carrier::CarrierError::Full,
            ))
            | Err(shoop_rt_transport::client::ClientError::Busy) => 0,
            Err(error) => {
                self.lose(alloc::format!("graph submission failed: {error:?}"));
                0
            }
        }
    }
    pub(crate) fn prepared_graph_response(
        &mut self,
        response: &shoop_rt_protocol::immediate::Response,
    ) {
        if !response
            .records()
            .iter()
            .any(|r| r.command() == Command::ActivateGraph)
        {
            return;
        }
        let Some(graph) = &mut self.prepared_graph else {
            return;
        };
        let Some(sequence) = graph.pending.take() else {
            return;
        };
        let success = response.header.code() == Code::Applied
            && response.records().iter().all(|r| r.code() == Code::Applied);
        if success {
            graph.active = true;
            graph.revision = response.header.graph();
            graph.dirty = false;
            graph.committed = Some(self.mirror.clone());
        } else {
            graph.dirty = false;
            if let Some(previous) = &graph.committed {
                self.mirror.restore_prepared(previous.clone());
            }
        }
        self.mutation_completions.push(BackendMutationCompletion {
            sequence,
            result: if success {
                Ok(())
            } else {
                Err(response.header.code().capacity_resource().map_or_else(
                    || "prepared graph was refused".into(),
                    |resource| {
                        alloc::format!(
                            "insufficient capacity for {resource} while staging the graph"
                        )
                    },
                ))
            },
        });
    }
}
