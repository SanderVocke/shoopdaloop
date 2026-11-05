use super::*;
use core::ops::Range;

pub(in crate::resource) struct Source {
    media: Media,
    horizon: u64,
    events: usize,
    current: MidiState,
}
impl Source {
    pub(super) fn coverage(&self) -> Range<Frame> {
        self.media.origin..Frame(self.media.origin.0 + self.media.frames)
    }
    pub(super) fn state(&self) -> &MidiState {
        &self.current
    }
}
impl Core {
    pub fn begin_source(
        &mut self,
        origin: Frame,
        horizon: u64,
        events: usize,
        initial: &MidiState,
    ) -> Result<Source, Error> {
        if horizon == 0 || events < self.config.events_per_tick || origin.0 % self.q as u64 != 0 {
            return Err(Error::InvalidConfig);
        }
        let horizon = Quantum::new(self.q)?.ceil(Frame(horizon))?.0;
        Ok(Source {
            media: self.empty(origin, 0, initial)?,
            horizon,
            events,
            current: *initial,
        })
    }
    pub fn commit_source(
        &mut self,
        io: &mut RenderIo<'_>,
        source: &mut Source,
        staged: &Staging,
    ) -> Result<(), Error> {
        self.indexes.reset_work();
        let result = self.commit_source_inner(source, staged);
        self.account(io);
        result
    }
    fn commit_source_inner(&mut self, source: &mut Source, staged: &Staging) -> Result<(), Error> {
        let next = source.coverage().end;
        if staged.frame != next {
            return Err(Error::InvalidFrame);
        }
        let end = next
            .0
            .checked_add(self.q as u64)
            .ok_or(Error::InvalidFrame)?;
        let revision = source
            .media
            .revision
            .0
            .checked_add(1)
            .ok_or(Error::Exhausted)?;
        for record in &staged.records[..staged.count] {
            self.packets.value(record.ok_or(Error::InvalidId)?)?;
        }
        let total = self
            .indexes
            .len(source.media.root)?
            .checked_add(staged.count)
            .ok_or(Error::Capacity)?;
        let mut origin = source
            .media
            .origin
            .0
            .max(end.saturating_sub(source.horizon));
        let mut after = Cut::frame(source.media.origin);
        self.removals.clear();
        while let Some(value) =
            self.indexes
                .first(source.media.root, after, Cut::frame(next), &self.packets)?
        {
            let point = self.packets.point(value);
            if point.frame.0 >= origin && total - self.removals.len() <= source.events {
                break;
            }
            if self.removals.len() == self.config.events_per_tick * 2 {
                return Err(Error::Capacity);
            }
            self.removals.push(value);
            if total - self.removals.len() > source.events || point.frame.0 >= origin {
                origin = origin.max(Quantum::new(self.q)?.ceil(Frame(point.frame.0 + 1))?.0);
            }
            after = Cut::at(point);
        }
        let initial = if self.removals.is_empty() {
            None
        } else {
            let mut initial = source.media.initial;
            for &value in &self.removals {
                self.packets.apply(value, &mut initial);
            }
            Some(initial)
        };
        source.media.root = self.indexes.replace_records(
            source.media.root,
            &self.removals,
            &staged.values[..staged.count],
            &mut self.packets,
        )?;
        if let Some(initial) = initial {
            source.media.initial = initial;
        }
        for &value in &staged.values[..staged.count] {
            self.packets.apply(value, &mut source.current);
        }
        source.media.origin = Frame(origin);
        source.media.frames = end - origin;
        source.media.revision = Revision(revision);
        Ok(())
    }
    pub fn grab_source(
        &mut self,
        io: &mut RenderIo<'_>,
        source: &Source,
        range: Range<Frame>,
        frames: u64,
    ) -> Result<Media, Error> {
        let coverage = source.coverage();
        if range.start < coverage.start
            || range.end > coverage.end
            || range.start > range.end
            || range.end.0 - range.start.0 > frames
            || range.start.0.checked_add(frames).is_none()
        {
            return Err(Error::InvalidFrame);
        }
        let initial = self.state(io, &source.media, range.start.0 - coverage.start.0)?;
        self.indexes.reset_work();
        let root = self.indexes.retain(source.media.root)?;
        let result = self.indexes.window(
            root,
            Cut::frame(range.start),
            Cut::frame(range.end),
            &mut self.packets,
        );
        if result.is_err() {
            self.indexes.release(root).unwrap();
        }
        self.account(io);
        Ok(Media {
            root: result?,
            origin: range.start,
            frames,
            initial,
            revision: Revision(0),
        })
    }
    pub fn reset_source(
        &mut self,
        source: &mut Source,
        origin: Frame,
        initial: &MidiState,
    ) -> Result<(), Error> {
        if origin.0 % self.q as u64 != 0 {
            return Err(Error::InvalidFrame);
        }
        let media = self.empty(origin, 0, initial)?;
        let old = core::mem::replace(&mut source.media, media);
        source.current = *initial;
        self.release(old)
    }
    pub fn release_source(&mut self, source: Source) -> Result<(), Error> {
        self.release(source.media)
    }
}
