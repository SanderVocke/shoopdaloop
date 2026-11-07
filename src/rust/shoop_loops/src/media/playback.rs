use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlaybackChange {
    pub owner: LoopId,
    pub media: MediaId,
}

pub struct PlaybackActivation<'a, 'store, I> {
    ledger: &'a mut Ledger,
    io: &'a mut ResourceIo<'store>,
    changes: I,
}

impl Ledger {
    pub fn prepare_playback_activation<'a, 'store, I>(
        &'a mut self,
        io: &'a mut ResourceIo<'store>,
        changes: I,
    ) -> Result<PlaybackActivation<'a, 'store, I>, Error>
    where
        I: Iterator<Item = PlaybackChange> + Clone,
    {
        for (index, change) in changes.clone().enumerate() {
            if index >= self.owners.len() {
                return Err(Error::Capacity);
            }
            if changes
                .clone()
                .take(index)
                .any(|earlier| earlier.owner == change.owner)
            {
                return Err(Error::InvalidId);
            }
            let owner = self.owner(change.owner)?;
            let media = self.media(change.media)?;
            if !media.ready || media.view.description.owner != change.owner {
                return Err(Error::InvalidId);
            }
            Self::check_sources(io, media)?;
            if owner.playback != Some(change.media) {
                if media.references == u32::MAX {
                    return Err(Error::Capacity);
                }
                if let Some(previous) = owner.playback {
                    self.check_drop(io, previous)?;
                }
            }
        }
        Ok(PlaybackActivation {
            ledger: self,
            io,
            changes,
        })
    }
}

impl<I: Iterator<Item = PlaybackChange>> PlaybackActivation<'_, '_, I> {
    pub fn commit(self) {
        for change in self.changes {
            self.ledger
                .set_playback(self.io, change.owner, Some(change.media))
                .expect("preflighted playback change");
        }
    }
}
