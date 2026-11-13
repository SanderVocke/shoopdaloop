use super::*;
use rt_runtime::Sample;

impl<S: Sample> LoopPlanner<S> {
    pub(super) fn append_node_pair(
        &mut self,
        id: NodePairStageId,
        previous: Option<LoopId>,
        initial: Option<NodePairInitial<S>>,
        io: &mut ResourceIo<'_, S>,
    ) -> Result<(), Error> {
        if initial.is_some_and(|i| i.frames % self.resolver.quantum().frames() as u64 != 0) {
            return Err(Error::InvalidFrame);
        }
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
        replace_all: bool,
        graphs: &mut rt_runtime::graph::activation::GraphAccess<'_>,
        io: &mut PlannerResourceIo<'_, S>,
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
        if replace_all
            && self.rows.iter().flatten().any(|row| {
                row.assigned
                    && !bank
                        .edits
                        .iter()
                        .any(|edit| edit.previous == Some(row.binding.target))
            })
        {
            return Err(Error::InvalidShape.into());
        }
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
                let mode = self.resolver.state(previous)?.mode;
                if mode != Mode::Stopped && !(replace_all && mode == Mode::Playing) {
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
            for bank in row.groups {
                for (roles, group) in bank.into_iter().enumerate() {
                    let Some(group) = group else {
                        continue;
                    };
                    let info = io.capture().info(group)?;
                    let members = usize::from(row.binding.channels > 0)
                        + usize::from(row.binding.midi)
                        + usize::from(roles == 1 && row.binding.auxiliary_channels > 0);
                    if info.phase != CapturePhase::Idle || info.members != members {
                        return Err(Error::InvalidState.into());
                    }
                    if replace_all {
                        io.capture().check_idle_group(group)?;
                    }
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
                    if !permit.candidate().contains(consumer) {
                        return Err(Error::InvalidShape.into());
                    }
                }
            } else if [row.binding.playback, row.binding.capture]
                .into_iter()
                .any(|consumer| permit.candidate().contains(consumer))
                || candidate
                    .nodes()
                    .iter()
                    .any(|n| n.id.slot() as usize == slot && n.kind == LoopKind::Primitive)
            {
                return Err(Error::InvalidShape.into());
            }
        }

        let bank = self.node_pairs.bank_mut(node_pairs)?;
        let media = self
            .ledger
            .prepare_owner_activation(io, &mut bank.changes)?;
        self.topologies.activate_configuration(
            topology,
            topology_epoch,
            &mut self.resolver,
            replace_all,
        )?;
        media.commit();
        for edit in &bank.edits {
            let slot = edit.slot().expect("validated edit");
            let row = self.rows[slot].as_mut().expect("prepared node_pair");
            if replace_all {
                for group in row.groups.into_iter().flatten().flatten() {
                    io.capture()
                        .clear_idle_group(group)
                        .expect("prevalidated idle capture group");
                }
            }
            row.assigned = edit.initial.is_some();
            row.windows = Windows::default();
            row.timing_revision = 0;
            row.length_override = None;
            if let Some(initial) = edit.initial {
                row.windows = self
                    .ledger
                    .playback(initial.target)?
                    .map(|id| self.ledger.view(id))
                    .transpose()?
                    .and_then(|view| view.windows)
                    .unwrap_or_default();
                row.binding.target = initial.target;
                row.dry_wet = initial.dry_wet;
                row.history_source = initial.history_source;
                row.audio_widths = initial
                    .audio_widths
                    .map(|widths| widths.map(usize::from))
                    .unwrap_or([row.binding.channels, row.binding.auxiliary_channels]);
                row.binding.gain = initial.gain;
                row.balance = initial.balance;
                row.reset_input |= !initial.preserve_input;
                if row.reset_input {
                    self.inputs[slot] = MidiState::default();
                }
            } else {
                row.reset_input = false;
            }
            row.failure = None;
            row.aborted = false;
            row.overflows = 0;
        }
        self.node_pairs.applied(node_pairs);
        self.graph_epoch = if replace_all {
            permit.commit_with_midi_reset()
        } else {
            permit.commit()
        };
        Ok((self.graph_epoch, self.resolver.epoch()))
    }
}
