//! Control-owned graph preparation and explicit execution-program handoff.
use crate::*;
use shoop_backend::BackendMutationCompletion;
use shoop_rt_protocol::{
    immediate::{Batch, Code, Command},
    Direction, HostPort, HostPortId, HostPortKind, Name, Outcome, Refusal, Request,
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
                    Request::CreateStrip { .. }
                    | Request::CreateProcessedStrip { .. }
                    | Request::CreateBus { .. }
                    | Request::CreateProcessedBus { .. }
                    | Request::RemoveBus { .. }
                    | Request::CreateLoop { .. }
                    | Request::RemoveStrip { .. }
                    | Request::LoopSync { .. }
                    | Request::Connect { .. }
                    | Request::Route { .. }
                    | Request::StripControl { .. }
                    | Request::BusControl { .. }
                    | Request::LoopBalance { .. } => true,
                    Request::SetSmoothing { .. } => true,
                    Request::LoopGain { .. } => !self.prepared_graph.as_ref().unwrap().active,
                    Request::CancelBulk => true,
                    _ => false,
                };
            if supported && !matches!(pending.request, Request::CancelBulk) {
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
        let mut outgoing =
            shoop_rt_protocol::graph::upload(&self.mirror, self.limits().quantum, state.revision)?
                .into_iter()
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
                    || {
                        response
                            .header
                            .code()
                            .unsupported_graph_feature()
                            .map_or_else(
                                || "prepared graph was refused".into(),
                                |feature| {
                                    alloc::format!("graph compiler does not support {feature}")
                                },
                            )
                    },
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
