use super::*;

#[derive(Clone, Copy, Debug)]
pub enum Immediate<S: Sample = f32> {
    Gain(S),
    Balance(S),
    Playback(bool),
}

impl<S: Sample> LoopPlanner<S> {
    pub fn validate_immediate(&self, target: LoopId, command: Immediate<S>) -> Result<(), Error> {
        let row = self.row(target)?;
        match command {
            Immediate::Gain(value) if !value.is_finite() => Err(Error::InvalidParameter),
            Immediate::Balance(value) if !(-S::ONE..=S::ONE).contains(&value) => {
                Err(Error::InvalidParameter)
            }
            Immediate::Playback(_) => {
                let media = self.ledger.playback(target)?.ok_or(Error::InvalidState)?;
                let view = self.ledger.view(media)?;
                if view
                    .midi
                    .is_some_and(|source| source.snapshot.description.frames != 0)
                    || view.audio.is_none()
                    || row.dry_wet
                    || row.active.is_some()
                    || row.prepared.is_some()
                    || row.failure.is_some()
                {
                    return Err(Error::InvalidState);
                }
                self.resolver
                    .validate_immediate_playback(target)
                    .map_err(runtime_error)
            }
            _ => Ok(()),
        }
    }
    pub fn apply_immediate(&mut self, target: LoopId, command: Immediate<S>) -> Result<(), Error> {
        self.validate_immediate(target, command)?;
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
            Immediate::Playback(start) => self
                .resolver
                .immediate_playback(target, start)
                .map_err(runtime_error)?,
        }
        Ok(())
    }
}
