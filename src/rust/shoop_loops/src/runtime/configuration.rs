use super::*;

impl LoopPlanner {
    pub(super) fn append_node_pair(
        &mut self,
        id: NodePairStageId,
        previous: Option<LoopId>,
        initial: Option<NodePairInitial>,
        io: &mut ResourceIo<'_>,
    ) -> Result<(), Error> {
        let edit = node_pairs::Edit { previous, initial };
        let row = self
            .rows
            .get(edit.slot()?)
            .and_then(|r| *r)
            .ok_or(Error::InvalidId)?;
        if row.assigned.then_some(row.binding.target) != previous {
            return Err(Error::InvalidId);
        }
        if initial.is_some_and(|i| i.preserve_input && !row.assigned) {
            return Err(Error::InvalidState);
        }
        self.node_pairs
            .append(id, edit, row.binding, &mut self.ledger, io)
    }
    pub(super) fn activate_configuration(
        &mut self,
        node_pairs: NodePairStageId,
        topology: StageId,
        graph: GraphStageId,
        graph_epoch: Epoch,
        topology_epoch: Epoch,
        graphs: &mut rt_runtime::graph::activation::GraphAccess<'_>,
        io: &mut PlannerResourceIo<'_>,
    ) -> Result<(Epoch, Epoch), Failure> {
        if self.resolver.epoch() != topology_epoch {
            return Err(crate::Error::InvalidState.into());
        }
        if self.node_pairs.status(node_pairs)? != NodePairStatus::Ready {
            return Err(Error::InvalidState.into());
        }
        let permit = graphs.prepare_activation(graph, graph_epoch)?;
        let candidate = self.topologies.candidate(topology)?;
        let bank = self.node_pairs.bank(node_pairs)?;
        for edit in &bank.edits {
            let row = self
                .rows
                .get(edit.slot()?)
                .and_then(|r| *r)
                .ok_or(Error::InvalidId)?;
            if row.assigned.then_some(row.binding.target) != edit.previous {
                return Err(Error::InvalidId.into());
            }
            if let Some(previous) = edit.previous {
                if self.resolver.state(previous)?.mode != Mode::Stopped {
                    return Err(crate::Error::InvalidState.into());
                }
            }
            if row.active.is_some()
                || row.prepared.is_some()
                || row.interval.is_some()
                || row.frozen.iter().any(Option::is_some)
            {
                return Err(Error::InvalidState.into());
            }
            for group in row.groups.into_iter().flatten() {
                let info = io.capture().info(group)?;
                if info.phase != CapturePhase::Idle
                    || info.members
                        != usize::from(row.binding.channels > 0) + usize::from(row.binding.midi)
                {
                    return Err(Error::InvalidState.into());
                }
            }
            if let Some(initial) = edit.initial {
                if initial.preserve_input && !row.assigned {
                    return Err(Error::InvalidState.into());
                }
                let node = candidate.node(initial.target)?;
                if node.kind != LoopKind::Primitive || node.length != initial.frames {
                    return Err(Error::InvalidShape.into());
                }
            }
        }
        for node in candidate.nodes() {
            if node.kind == LoopKind::Primitive && self.rows[node.id.slot() as usize].is_none() {
                return Err(Error::InvalidId.into());
            }
        }
        let mut bindings = 0usize;
        for (slot, row) in self.rows.iter().enumerate() {
            let Some(row) = row else { continue };
            let owner = if let Some(edit) = bank.edits.iter().find(|e| e.slot() == Ok(slot)) {
                edit.initial.map(|i| i.target)
            } else {
                row.assigned.then_some(row.binding.target)
            };
            if let Some(owner) = owner {
                if candidate.node(owner)?.kind != LoopKind::Primitive {
                    return Err(Error::InvalidShape.into());
                }
                for consumer in [row.binding.playback, row.binding.capture] {
                    if !permit.candidate().contains(consumer)
                        || !permit.candidate().planning().bindings.iter().any(|b| {
                            b.consumer == consumer
                                && b.planner == self.settings.planner
                                && b.decision_slot == slot
                        })
                    {
                        return Err(Error::InvalidShape.into());
                    }
                    bindings += 1;
                }
            } else if candidate
                .nodes()
                .iter()
                .any(|n| n.id.slot() as usize == slot && n.kind == LoopKind::Primitive)
            {
                return Err(Error::InvalidShape.into());
            }
        }
        if bindings != permit.candidate().planning().bindings.len() {
            return Err(Error::InvalidShape.into());
        }
        let bank = self.node_pairs.bank_mut(node_pairs)?;
        let media = self
            .ledger
            .prepare_owner_activation(io, &mut bank.changes)?;
        self.topologies
            .activate_configuration(topology, topology_epoch, &mut self.resolver)?;
        media.commit();
        for edit in &bank.edits {
            let slot = edit.slot().expect("validated edit");
            let row = self.rows[slot].as_mut().expect("prepared node_pair");
            row.assigned = edit.initial.is_some();
            if let Some(initial) = edit.initial {
                row.binding.target = initial.target;
                row.binding.gain = initial.gain;
                row.reset_input |= !initial.preserve_input;
                if row.reset_input {
                    self.inputs[slot] = MidiState::default();
                }
            } else {
                row.reset_input = false;
            }
            row.failure = None;
            row.overflows = 0;
        }
        self.node_pairs.applied(node_pairs);
        self.graph_epoch = permit.commit();
        Ok((self.graph_epoch, self.resolver.epoch()))
    }
}
