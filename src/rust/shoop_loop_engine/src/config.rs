//! Storage limits, not per-callback musical-work quotas.

use crate::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub loop_slots: usize,
    pub relations: usize,
    pub program_entries: usize,
    pub program_segments: usize,
    pub pending_requests: usize,
    pub journal_events: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            loop_slots: 64,
            relations: 256,
            program_entries: 256,
            program_segments: 1024,
            pending_requests: 64,
            journal_events: 1024,
        }
    }
}

impl Limits {
    pub fn validate(self) -> Result<Self, Error> {
        if [
            self.loop_slots,
            self.relations,
            self.program_entries,
            self.program_segments,
            self.pending_requests,
        ]
        .contains(&0)
        {
            return Err(Error::InvalidConfig);
        }
        for count in [
            self.loop_slots,
            self.relations,
            self.program_entries,
            self.program_segments,
            self.pending_requests,
            self.journal_events,
        ] {
            checked_storage(count, 1)?;
        }
        checked_storage(
            self.loop_slots,
            size_of::<Option<crate::topology::LoopSpec>>(),
        )?;
        checked_storage(self.relations, size_of::<crate::topology::Trigger>())?;
        checked_storage(self.program_entries, size_of::<crate::program::Entry>())?;
        checked_storage(self.program_segments, size_of::<crate::program::Segment>())?;
        Ok(self)
    }
}

pub(crate) fn checked_storage(count: usize, element: usize) -> Result<usize, Error> {
    let bytes = count.checked_mul(element).ok_or(Error::Capacity)?;
    if bytes > i32::MAX as usize {
        return Err(Error::Capacity);
    }
    Ok(bytes)
}
