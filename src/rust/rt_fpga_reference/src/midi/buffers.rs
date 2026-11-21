use super::{BufferId, BufferLimits, Error, Event, Record, ViewId, Work};
use alloc::{boxed::Box, vec, vec::Vec};

#[derive(Clone, Copy, Default)]
pub(crate) struct Span {
    pub start: usize,
    pub count: usize,
    pub ready: bool,
}
#[derive(Clone, Copy, Default)]
pub(crate) struct Cursor {
    pub owner: Option<crate::records::Identity>,
    pub sequence: Option<super::SequenceId>,
    pub view: Option<ViewId>,
    pub index: u32,
    pub next: Option<Record>,
    pub ended: bool,
}
pub(crate) struct Buffers {
    pub limits: BufferLimits,
    pub spans: Box<[Span]>,
    pub cursors: Box<[Cursor]>,
    pub events: Vec<Event>,
    pub work: Work,
    pub observed_inputs: Box<[bool]>,
}
impl Buffers {
    pub fn prepare(limits: BufferLimits) -> Result<Self, Error> {
        if limits.buffers == 0 || limits.scratch_events == 0 {
            return Err(Error::Capacity);
        }
        let mut events = Vec::new();
        events
            .try_reserve_exact(limits.scratch_events)
            .map_err(|_| Error::Capacity)?;
        Ok(Self {
            limits,
            spans: vec![Span::default(); usize::from(limits.buffers)].into_boxed_slice(),
            cursors: vec![Cursor::default(); usize::from(limits.cursors)].into_boxed_slice(),
            events,
            work: Work::default(),
            observed_inputs: vec![false; limits.input_ports as usize].into_boxed_slice(),
        })
    }
    pub fn begin(&mut self) {
        self.events.clear();
        self.observed_inputs.fill(false);
        self.spans.fill(Span::default());
    }
    pub fn source(&self, id: BufferId) -> Result<Span, Error> {
        self.spans
            .get(usize::from(id.0))
            .copied()
            .filter(|s| s.ready)
            .ok_or(Error::InvalidState)
    }
    pub fn destination(&self, id: BufferId) -> Result<(), Error> {
        if self.spans.get(usize::from(id.0)).is_none_or(|s| s.ready) {
            return Err(Error::InvalidState);
        }
        Ok(())
    }
    pub fn push(&mut self, event: Event) -> Result<(), Error> {
        if self.events.len() == self.limits.scratch_events {
            return Err(Error::Capacity);
        }
        self.events.push(event);
        Ok(())
    }
    pub fn publish(&mut self, id: BufferId, start: usize) {
        self.spans[usize::from(id.0)] = Span {
            start,
            count: self.events.len() - start,
            ready: true,
        };
    }
}
