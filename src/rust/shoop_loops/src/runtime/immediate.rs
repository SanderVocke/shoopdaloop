use super::*;

#[derive(Clone, Copy, Debug)]
pub enum Immediate<S: Sample = f32> {
    Gain(S),
    Balance(S),
    Playback(bool),
    Record,
}

impl<S: Sample> LoopPlanner<S> {
    pub fn apply_immediate(&mut self, target: LoopId, command: Immediate<S>) -> Result<(), Error> {
        let row = self.row(target)?;
        match command {
            Immediate::Record => return Err(Error::InvalidState),
            Immediate::Gain(value) if !value.is_finite() => Err(Error::InvalidParameter),
            Immediate::Balance(value) if !(-S::ONE..=S::ONE).contains(&value) => {
                Err(Error::InvalidParameter)
            }
            Immediate::Playback(_) => {
                if matches!(command, Immediate::Playback(false))
                    && !row.dry_wet
                    && self.resolver.state(target).map_err(runtime_error)?.mode == Mode::Stopped
                {
                    return Ok(());
                }
                let media = self.ledger.playback(target)?.ok_or(Error::InvalidState)?;
                let view = self.ledger.view(media)?;
                if view
                    .midi
                    .is_some_and(|source| source.snapshot.description.bytes != 0)
                    || view.audio.is_none()
                    || row.dry_wet
                    || row.active.is_some()
                    || row.prepared.is_some()
                {
                    return Err(Error::InvalidState);
                }
                Ok(())
            }
            _ => Ok(()),
        }?;
        match command {
            Immediate::Gain(value) => {
                self.rows[target.slot() as usize]
                    .as_mut()
                    .expect("row")
                    .binding
                    .gain = value
            }
            Immediate::Balance(value) => {
                self.rows[target.slot() as usize]
                    .as_mut()
                    .expect("row")
                    .balance = value
            }
            Immediate::Playback(true) => self
                .resolver
                .immediate_playback(target, true)
                .map_err(runtime_error)?,
            Immediate::Playback(false) => {
                let decision = self
                    .resolver
                    .immediate_capture_decision(target, Transition::Stop)
                    .map_err(runtime_error)?;
                self.resolver.commit_immediate_capture(decision);
            }
            Immediate::Record => unreachable!(),
        }
        Ok(())
    }

    /// Capture changes are settled here, before reporting command completion.
    pub fn apply_immediate_capture(
        &mut self,
        target: LoopId,
        command: Immediate<S>,
        frame: Frame,
        io: &mut PlannerResourceIo<'_, S>,
    ) -> Result<(), Error> {
        let state = self.resolver.state(target).map_err(runtime_error)?;
        if !matches!(command, Immediate::Record)
            && !(matches!(command, Immediate::Playback(_)) && state.mode == Mode::Recording)
        {
            return self.apply_immediate(target, command);
        }
        let row = self.row(target)?;
        if frame != self.resolver.frame()
            || row.dry_wet
            || row.binding.channels == 0
            || row.prepared.is_some()
            || row.aborted
        {
            return Err(Error::InvalidState);
        }
        let operation = match command {
            Immediate::Record => Transition::Record {
                monitor_previous: false,
            },
            Immediate::Playback(true) => Transition::Play {
                position: 0,
                retrigger: true,
            },
            Immediate::Playback(false) => Transition::Stop,
            _ => unreachable!(),
        };
        let decision = self
            .resolver
            .immediate_capture_decision(target, operation)
            .map_err(runtime_error)?;
        let slot = target.slot() as usize;
        if state.mode == Mode::Recording {
            let take = row.active.ok_or(Error::InvalidState)?;
            if (take.token.is_none() && !(take.audio_only && take.writer.is_some()))
                || take.revision != state.capture_revision
            {
                return Err(Error::InvalidState);
            }
            if matches!(command, Immediate::Record) {
                return Ok(());
            }
            if let Some(interval) = row.interval {
                io.capture().cancel_tick(interval)?;
                self.rows[slot].as_mut().expect("recording row").interval = None;
            }
            let media = self.freeze(slot, frame, io)?;
            self.ledger.set_playback(io, target, Some(media))?;
            let row = self.rows[slot].as_mut().expect("completed take");
            row.windows = take.windows;
            row.length_override = None;
        } else {
            if row.active.is_some()
                || self
                    .shared
                    .as_ref()
                    .is_some_and(|shared| shared.live_for(target).is_some())
            {
                return Err(Error::InvalidState);
            }
            if row.groups[row.next_bank][0].is_none() {
                return Err(Error::Capacity);
            }
            self.prepare_take(slot, decision, io)?;
            let mut take = self.rows[slot]
                .as_mut()
                .expect("prepared row")
                .prepared
                .take()
                .expect("prepared take");
            take.publish_incomplete = true;
            let token = take.token.expect("audio capture group");
            let interval = match io.capture().reserve_prepared(token, frame) {
                Ok(interval) => interval,
                Err(error) => {
                    io.capture().cancel_take(token)?;
                    self.ledger.retire(io, take.media)?;
                    return Err(error);
                }
            };
            if let Err(error) = io.capture().activate(token, frame) {
                io.capture().cancel_tick(interval)?;
                io.capture().cancel_take(token)?;
                self.ledger.retire(io, take.media)?;
                return Err(error);
            }
            let row = self.rows[slot].as_mut().expect("recording row");
            row.windows = take.windows;
            row.active = Some(take);
            row.interval = Some(interval);
            row.next_bank = 1 - take.bank;
            row.failure = None;
        }
        self.resolver.commit_immediate_capture(decision);
        Ok(())
    }
}
