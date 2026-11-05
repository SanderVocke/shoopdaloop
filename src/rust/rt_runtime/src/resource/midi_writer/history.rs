use super::*;
use core::ops::Range;

pub(in crate::resource) struct Source {
    media: Media,
    horizon: u64,
    events: usize,
    bytes: usize,
    pages: usize,
    current: MidiState,
}
impl Source {
    pub(in crate::resource) fn coverage(&self) -> Range<Frame> {
        self.media.origin..Frame(self.media.origin.0 + self.media.frames)
    }
    pub(in crate::resource) fn state(&self) -> &MidiState {
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
        self.begin_source_with_bytes(origin, horizon, events, usize::MAX, initial)
    }
    pub fn begin_source_with_bytes(
        &mut self,
        origin: Frame,
        horizon: u64,
        events: usize,
        bytes: usize,
        initial: &MidiState,
    ) -> Result<Source, Error> {
        let tick_bytes = self
            .config
            .events_per_tick
            .checked_mul(12)
            .and_then(|n| n.checked_add(self.config.bytes_per_tick))
            .ok_or(Error::Capacity)?;
        if bytes < tick_bytes {
            return Err(Error::InvalidConfig);
        }
        if horizon == 0 || events < self.config.events_per_tick || origin.0 % self.q as u64 != 0 {
            return Err(Error::InvalidConfig);
        }
        let horizon = Quantum::new(self.q)?.ceil(Frame(horizon))?.0;
        Ok(Source {
            media: self.empty(origin, 0, initial)?,
            horizon,
            events,
            bytes,
            pages: bytes.div_ceil(self.packets.page_bytes()),
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
        if result == Ok(true) {
            io.store
                .charge_work(WorkClass::Render, 2 * size_of::<MidiState>(), 0)?;
        }
        self.account(io);
        result.map(|_| ())
    }
    fn commit_source_inner(
        &mut self,
        source: &mut Source,
        staged: &Staging,
    ) -> Result<bool, Error> {
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
        let totals = self.indexes.totals(
            source.media.root,
            Cut::frame(source.media.origin),
            Cut::frame(next),
            &self.packets,
        )?;
        let incoming_bytes =
            staged.values[..staged.count]
                .iter()
                .try_fold(0usize, |n, &value| {
                    n.checked_add(12)
                        .and_then(|n| n.checked_add(self.packets.packet_len(value)))
                        .ok_or(Error::Capacity)
                })?;
        let keep_events = source
            .events
            .checked_sub(staged.count)
            .ok_or(Error::Capacity)?;
        let keep_bytes = source
            .bytes
            .checked_sub(incoming_bytes)
            .ok_or(Error::Capacity)?;
        let mut origin = source
            .media
            .origin
            .0
            .max(end.saturating_sub(source.horizon));
        let mut discard_through = None;
        if totals.events > keep_events {
            discard_through = self
                .indexes
                .record_number(source.media.root, totals.events - keep_events - 1)?;
        }
        if totals.bytes > keep_bytes {
            let (value, _) = self
                .indexes
                .record_at(
                    source.media.root,
                    totals.bytes - keep_bytes - 1,
                    &self.packets,
                )?
                .ok_or(Error::InvalidState)?;
            if discard_through.is_none_or(|old| self.packets.point(old) < self.packets.point(value))
            {
                discard_through = Some(value);
            }
        }
        if let Some(value) = discard_through {
            origin = origin.max(
                Quantum::new(self.q)?
                    .ceil(Frame(self.packets.point(value).frame.0 + 1))?
                    .0,
            );
        }
        let mut incoming = Totals::default();
        for &value in &staged.values[..staged.count] {
            incoming = <Totals as crate::midi_state::indexed::tree::Summary<NonZeroU32>>::then(
                incoming,
                <Totals as crate::midi_state::indexed::tree::Summary<NonZeroU32>>::value(
                    value,
                    &self.packets,
                ),
            );
        }
        let keep_pages = source
            .pages
            .checked_sub(incoming.pages)
            .ok_or(Error::Capacity)?;
        let mut low = origin.div_ceil(self.q as u64);
        let mut high = next.0 / self.q as u64;
        while low < high {
            let middle = low + (high - low) / 2;
            let pages = self
                .indexes
                .totals(
                    source.media.root,
                    Cut::frame(Frame(middle * self.q as u64)),
                    Cut::frame(next),
                    &self.packets,
                )?
                .pages;
            if pages > keep_pages {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        origin = low * self.q as u64;
        let discarded = self
            .indexes
            .totals(
                source.media.root,
                Cut::frame(source.media.origin),
                Cut::frame(Frame(origin)),
                &self.packets,
            )?
            .events;
        let bulk = discarded > self.config.events_per_tick;
        self.removals.clear();
        let initial = if discarded == 0 {
            None
        } else if bulk {
            let timeline = Window {
                inner: self.indexes.timeline(source.media.root, &self.packets)?,
                origin: source.media.origin.0,
                frames: source.media.frames,
            };
            Some(indexed::state(
                &timeline,
                &source.media.initial,
                Cut::frame(Frame(origin - source.media.origin.0)),
            ))
        } else {
            let mut initial = source.media.initial;
            let mut after = Cut::frame(source.media.origin);
            for _ in 0..discarded {
                let value = self
                    .indexes
                    .first(
                        source.media.root,
                        after,
                        Cut::frame(Frame(origin)),
                        &self.packets,
                    )?
                    .ok_or(Error::InvalidState)?;
                self.removals.push(value);
                self.packets.apply(value, &mut initial);
                after = Cut::at(self.packets.point(value));
            }
            Some(initial)
        };
        if bulk {
            let root = self.indexes.retain(source.media.root)?;
            let clipped = self.indexes.window(
                root,
                Cut::frame(Frame(origin)),
                Cut::frame(Frame(u64::MAX)),
                &mut self.packets,
            );
            let clipped = match clipped {
                Ok(root) => root,
                Err(error) => {
                    self.indexes.release(root).unwrap();
                    return Err(error);
                }
            };
            let changed = self.indexes.replace_records(
                clipped,
                &[],
                &staged.values[..staged.count],
                &mut self.packets,
            );
            let changed = match changed {
                Ok(root) => root,
                Err(error) => {
                    self.indexes.release(clipped).unwrap();
                    return Err(error);
                }
            };
            self.indexes.release(source.media.root)?;
            source.media.root = changed;
        } else {
            source.media.root = self.indexes.replace_records(
                source.media.root,
                &self.removals,
                &staged.values[..staged.count],
                &mut self.packets,
            )?;
        }
        if let Some(initial) = initial {
            source.media.initial = initial;
        }
        for &value in &staged.values[..staged.count] {
            self.packets.apply(value, &mut source.current);
        }
        for &value in &staged.values[..staged.count] {
            source.media.categories |= self.packet_categories(value);
        }
        source.media.origin = Frame(origin);
        source.media.frames = end - origin;
        source.media.revision = Revision(revision);
        Ok(initial.is_some())
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
        let totals = self.indexes.totals(
            source.media.root,
            Cut::frame(coverage.start),
            Cut::frame(coverage.end),
            &self.packets,
        )?;
        let kept = self.indexes.totals(
            source.media.root,
            Cut::frame(range.start),
            Cut::frame(range.end),
            &self.packets,
        )?;
        let mut media = Media {
            root: source.media.root,
            origin: range.start,
            frames,
            initial,
            revision: Revision(0),
            density_checked: range.start.0 % self.q as u64 == 0,
            credit: None,
            retained_events: totals.events - kept.events,
            retained_pages: totals.pages,
            retained_epoch: self.retirement_epoch,
            categories: source.media.categories,
        };
        self.owner_credit(
            &mut media,
            totals.events,
            totals
                .pages
                .checked_add(kept.pages)
                .ok_or(Error::Capacity)?,
        )?;
        self.indexes.reset_work();
        let root = match self.indexes.retain(source.media.root) {
            Ok(root) => root,
            Err(error) => {
                self.release_credit(&media);
                return Err(error);
            }
        };
        let result = self.indexes.window(
            root,
            Cut::frame(range.start),
            Cut::frame(range.end),
            &mut self.packets,
        );
        if let Err(error) = result {
            media.root = root;
            self.release(media)?;
            return Err(error);
        }
        self.account(io);
        let root = result?;

        media.root = root;
        Ok(media)
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
