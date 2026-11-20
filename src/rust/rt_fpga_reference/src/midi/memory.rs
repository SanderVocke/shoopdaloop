use super::{ContentStore, Error, SequenceId, StoreLimits, ViewId};
use alloc::{boxed::Box, vec};

/// Sequence slots own views; cursors borrow their identity until the next ordered job.
pub struct MidiMemory {
    pub store: ContentStore,
    sequences: Box<[Option<ViewId>]>,
}
impl MidiMemory {
    pub fn prepare(limits: StoreLimits, sequences: u16) -> Result<Self, Error> {
        if sequences == 0 {
            return Err(Error::Capacity);
        }
        Ok(Self {
            store: ContentStore::prepare(limits)?,
            sequences: vec![None; usize::from(sequences)].into_boxed_slice(),
        })
    }
    pub fn view(&self, id: SequenceId) -> Result<Option<ViewId>, Error> {
        self.sequences
            .get(usize::from(id.0))
            .copied()
            .ok_or(Error::InvalidState)
    }
    /// Replace at a completed quantum boundary; retained exports remain valid.
    pub fn replace(&mut self, id: SequenceId, view: Option<ViewId>) -> Result<(), Error> {
        let old = self.view(id)?;
        if let Some(v) = view {
            self.store.can_retain(v, 1)?;
        }
        if let Some(v) = old {
            self.store.can_release(core::slice::from_ref(&v))?;
        }
        if let Some(v) = view {
            self.store.retain(v)?;
        }
        if let Some(v) = old {
            self.store.release(v)?;
        }
        self.sequences[usize::from(id.0)] = view;
        Ok(())
    }
    pub(crate) fn begin_capture(&mut self, id: SequenceId) -> Result<(), Error> {
        let old = self.view(id)?;
        if let Some(v) = old {
            self.store.can_release(core::slice::from_ref(&v))?;
        }
        let view = self.store.writer()?;
        if let Some(v) = old {
            self.store.release(v)?;
        }
        self.sequences[usize::from(id.0)] = Some(view);
        Ok(())
    }
    pub(crate) fn finish_capture(&mut self, id: SequenceId) -> Result<(), Error> {
        let view = self.view(id)?.ok_or(Error::InvalidState)?;
        self.store.finish(view, self.store.describe(view)?.frames)
    }
}
