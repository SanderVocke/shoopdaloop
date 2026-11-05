use super::*;

impl Service {
    pub(super) fn require_input(&self, capability: BackendCapability) -> Result<(), CommandError> {
        if self.backend.capabilities().supports(capability) {
            Ok(())
        } else {
            Err(CommandError::unsupported(capability))
        }
    }

    pub(super) fn input_target(&self, id: LoopId) -> Result<BackendLoopId, CommandError> {
        let entry = self.loop_entry(id)?;
        if self.composites.contains_key(&id)
            || self.track(entry.track)?.view.topology != TrackTopology::Direct
        {
            return Err(CommandError::invalid(
                "input edits require direct primitive loops",
            ));
        }
        Ok(entry.backend)
    }

    fn input_solo(&self, ids: &[LoopId], edits: &mut Vec<BackendInputEdit>) {
        if !self.global.solo {
            return;
        }
        let tracks: BTreeSet<_> = ids.iter().map(|id| self.loops[id].track).collect();
        for id in self.ordered_loops() {
            if tracks.contains(&self.loops[&id].track) && !ids.contains(&id) {
                edits.push(BackendInputEdit::Mode {
                    loop_id: self.loops[&id].backend,
                    mode: BackendLoopMode::Stopped,
                    position: None,
                });
            }
        }
    }

    pub(super) fn grab_clicked(
        &mut self,
        initiating: LoopId,
        selection: bool,
    ) -> Result<(), CommandError> {
        self.loop_entry(initiating)?;
        let ids = if selection && self.global.sync && self.loops[&initiating].view.selected {
            self.ordered_loops()
                .into_iter()
                .filter(|id| self.loops[id].view.selected)
                .collect()
        } else {
            vec![initiating]
        };
        self.grab_gesture(ids)
    }

    pub(super) fn grab_gesture(&mut self, ids: Vec<LoopId>) -> Result<(), CommandError> {
        if ids.is_empty() {
            return Ok(());
        }
        self.require_input(BackendCapability::Grab)?;
        let sync = self
            .sync_backend()
            .ok_or_else(|| CommandError::invalid("Grab requires a sync loop"))?;
        let target = self
            .loops
            .values()
            .find(|entry| entry.view.targeted)
            .map(|entry| entry.backend);
        let mut unique = BTreeSet::new();
        let mut edits = Vec::with_capacity(ids.len());
        for id in &ids {
            if !unique.insert(*id) {
                return Err(CommandError::invalid("duplicate Grab target"));
            }
            edits.push(BackendInputEdit::Gesture {
                loop_id: self.input_target(*id)?,
                sync,
                target,
                synced: self.global.sync,
                cycles: self.global.apply_n_cycles.max(1),
                play_after_record: self.global.play_after_record,
            });
        }
        self.input_solo(&ids, &mut edits);
        self.backend.input_edits(&edits, None)?;
        Ok(())
    }

    pub(super) fn replacing(
        &mut self,
        ids: Vec<LoopId>,
        delay: Option<u32>,
        solo: bool,
    ) -> Result<(), CommandError> {
        if ids.is_empty() {
            return Ok(());
        }
        self.require_input(BackendCapability::ReplacingMode)?;
        let mut unique = BTreeSet::new();
        let mut edits = Vec::with_capacity(ids.len());
        for id in &ids {
            if !unique.insert(*id) {
                return Err(CommandError::invalid("duplicate Replacing target"));
            }
            edits.push(BackendInputEdit::Mode {
                loop_id: self.input_target(*id)?,
                mode: BackendLoopMode::Replacing,
                position: None,
            });
        }
        if solo {
            self.input_solo(&ids, &mut edits);
        }
        self.backend.input_edits(&edits, delay)?;
        Ok(())
    }
}
