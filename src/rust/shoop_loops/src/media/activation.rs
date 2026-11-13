use super::*;
use rt_runtime::Sample;

/// An owning, single-use logical-owner edit. Unapplied input leases remain the
/// caller's responsibility; successful publication transfers them to the ledger.
#[derive(Debug)]
pub struct OwnerChange {
    windows: Option<crate::windows::Windows>,
    previous: Option<LoopId>,
    input: Option<Input>,
    published: Option<MediaId>,
    applied: bool,
}
impl OwnerChange {
    pub fn replace(previous: Option<LoopId>, input: Input) -> Self {
        Self {
            windows: None,
            previous,
            input: Some(input),
            published: None,
            applied: false,
        }
    }
    pub fn replace_windowed(
        previous: Option<LoopId>,
        input: Input,
        windows: crate::windows::Windows,
    ) -> Self {
        Self {
            windows: Some(windows),
            ..Self::replace(previous, input)
        }
    }
    pub fn remove(previous: LoopId) -> Self {
        Self {
            windows: None,
            previous: Some(previous),
            input: None,
            published: None,
            applied: false,
        }
    }
    pub fn input(&self) -> Option<&Input> {
        self.input.as_ref()
    }
    pub(crate) fn view(&self) -> Option<View> {
        self.input.as_ref().map(|input| View {
            windows: self.windows,
            description: input.description,
            audio: input.audio,
            auxiliary: input.auxiliary,
            midi: input.midi,
        })
    }
    pub fn published(&self) -> Option<MediaId> {
        self.published
    }
    pub fn is_applied(&self) -> bool {
        self.applied
    }
    pub fn into_input(self) -> Option<Input> {
        self.input
    }
    fn slot(&self) -> Result<usize, Error> {
        if self.applied {
            return Err(Error::InvalidState);
        }
        match (self.previous, self.input.as_ref()) {
            (Some(old), Some(new)) if old.slot() != new.description.owner.slot() => {
                Err(Error::InvalidShape)
            }
            (_, Some(new)) => Ok(new.description.owner.slot() as usize),
            (Some(old), None) => Ok(old.slot() as usize),
            (None, None) => Err(Error::InvalidState),
        }
    }
}

/// Exclusive preflight authority for one atomic batch. Dropping an unused permit
/// changes neither ownership nor generation watermarks. It does not stop music,
/// clean MIDI, change a resolver, or claim capture resources.
pub struct OwnerActivation<'a, 'store, S: Sample = f32> {
    ledger: &'a mut Ledger,
    io: &'a mut ResourceIo<'store, S>,
    changes: &'a mut [OwnerChange],
}
impl Ledger {
    /// Validate all edits and initial media before publication. Each slot appears
    /// once; replacement generations strictly exceed its committed watermark.
    /// New initial media needs free metadata now, without counting later retirement.
    pub fn prepare_owner_activation<'a, 'store, S: Sample>(
        &'a mut self,
        io: &'a mut ResourceIo<'store, S>,
        changes: &'a mut [OwnerChange],
    ) -> Result<OwnerActivation<'a, 'store, S>, Error> {
        if changes.len() > self.owners.len()
            || changes.iter().filter(|c| c.input.is_some()).count() > self.free_media.len()
        {
            return Err(Error::Capacity);
        }
        for (index, change) in changes.iter().enumerate() {
            let slot = change.slot()?;
            let row = self.owners.get(slot).ok_or(Error::InvalidId)?;
            if row.value.map(|owner| owner.id) != change.previous {
                return Err(Error::InvalidId);
            }
            for earlier in &changes[..index] {
                if earlier.slot()? == slot {
                    return Err(Error::InvalidId);
                }
            }
            if let Some(owner) = row.value {
                for media in [owner.playback, owner.latest].into_iter().flatten() {
                    Self::check_sources(io, self.media(media)?)?;
                }
            }
            if let Some(input) = &change.input {
                if input.description.owner.generation() <= row.watermark {
                    return Err(Error::InvalidId);
                }
                self.validate_input(io, input, change.windows)?;
                for earlier in changes[..index].iter().filter_map(|c| c.input.as_ref()) {
                    for source in [earlier.audio, earlier.auxiliary, earlier.midi]
                        .into_iter()
                        .flatten()
                    {
                        if [input.audio, input.auxiliary, input.midi]
                            .into_iter()
                            .flatten()
                            .any(|new| new.snapshot.id == source.snapshot.id)
                        {
                            return Err(Error::InvalidState);
                        }
                    }
                }
            }
        }
        Ok(OwnerActivation {
            ledger: self,
            io,
            changes,
        })
    }
}
impl<S: Sample> OwnerActivation<'_, '_, S> {
    /// Publish preflighted owner/media changes without an ordinary failure path.
    /// Old residency, pins and exports remain independently owned. New initial
    /// media has one resident reference and one playback reference, no latest take.
    pub fn commit(self) {
        for change in self.changes {
            if let Some(previous) = change.previous {
                self.ledger
                    .unbind(self.io, previous)
                    .expect("preflighted owner release");
            }
            if let Some(input) = change.input.take() {
                let owner = input.description.owner;
                self.ledger.bind(owner).expect("preflighted generation");
                let id = self
                    .ledger
                    .allocate(
                        View {
                            windows: change.windows,
                            description: input.description,
                            audio: input.audio,
                            auxiliary: input.auxiliary,
                            midi: input.midi,
                        },
                        true,
                    )
                    .expect("reserved metadata capacity");
                self.ledger
                    .set_playback(self.io, owner, Some(id))
                    .expect("new initial reference");
                change.published = Some(id);
            }
            change.applied = true;
        }
    }
}
