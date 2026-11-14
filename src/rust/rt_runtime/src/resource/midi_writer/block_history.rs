use super::*;
use core::ops::Range;

pub(in crate::resource) struct Source {
    pub(in crate::resource) media: Media,
    horizon: u64,
    events: usize,
    bytes: usize,
    current: MidiState,
}
pub(in crate::resource) struct Grab {
    source: Media,
    output: Media,
    start: u64,
    end: u64,
    phase: u8,
}
impl Source {
    pub(in crate::resource) fn configure_horizon(&mut self, frames: u64) -> Result<(), Error> {
        if self.media.frames != 0 || frames == 0 || frames > self.horizon {
            return Err(Error::InvalidConfig);
        }
        self.horizon = frames;
        Ok(())
    }
    pub(in crate::resource) fn coverage(&self) -> Range<Frame> {
        self.media.origin..Frame(self.media.origin.0 + self.media.frames)
    }
    pub(in crate::resource) fn state(&self) -> &MidiState {
        &self.current
    }
}
impl Core {
    pub fn grab_replay_bound(&mut self, grab: &Grab) -> Result<usize, Error> {
        let count = usize::try_from(self.blocks.totals(grab.source.version)?.events)
            .map_err(|_| Error::Capacity)?;
        count
            .checked_mul(crate::midi_state::BASIC_STATE_CELLS + 1)
            .and_then(|n| n.checked_add(crate::midi_state::BASIC_STATE_CELLS + 2))
            .ok_or(Error::Capacity)
    }
    pub fn begin_grab<S: Sample>(
        &mut self,
        io: &mut RenderIo<'_, S>,
        source: &Source,
        range: Range<Frame>,
        frames: u64,
    ) -> Result<Grab, Error> {
        self.maintain(io, self.required_maintenance(1))?;
        let coverage = source.coverage();
        if range.start < coverage.start
            || range.end > coverage.end
            || range.start > range.end
            || range.end.0 - range.start.0 > frames
            || range.start.0.checked_add(frames).is_none()
        {
            return Err(Error::InvalidFrame);
        }
        if self.caches.iter().filter(|slot| slot.is_none()).count() < 2 {
            return Err(Error::Capacity);
        }
        let mut pinned = self.retain(&source.media)?;
        let count = self.blocks.totals(pinned.version)?.events as usize;
        if let Err(error) = self.charge_owner(&mut pinned, count) {
            self.release(pinned)?;
            return Err(error);
        }
        let result = (|| {
            let clipped = self.blocks.clip(pinned.version, range.clone())?;
            let normalized = self.blocks.normalize(clipped);
            self.blocks.release(clipped)?;
            let version = normalized?;
            let count = self.blocks.totals(version)?.events as usize;
            let mut output = self
                .own(MediaImage {
                    version,
                    origin: range.start,
                    frames,
                    revision: 0,
                    cut: range.end,
                    initial: *self.initial(&pinned),
                    frontier: *self.initial(&pinned),
                    frontier_position: 0,
                })
                .map_err(|(error, _)| error)
                .expect("reserved cache");
            if let Err(error) = self.charge_owner(&mut output, count) {
                self.release(output)?;
                return Err(error);
            }
            Ok(output)
        })();
        let output = match result {
            Ok(output) => output,
            Err(error) => {
                self.release(pinned)?;
                return Err(error);
            }
        };
        let start = range.start.0 - coverage.start.0;
        let end = range.end.0 - coverage.start.0;
        self.account(io)?;
        Ok(Grab {
            source: pinned,
            output,
            start,
            end,
            phase: 0,
        })
    }
    pub fn progress_grab<S: Sample>(
        &mut self,
        io: &mut RenderIo<'_, S>,
        grab: &mut Grab,
        steps: usize,
    ) -> Result<bool, Error> {
        let target = match grab.phase {
            0 => grab.start,
            1 => grab.end,
            _ => return Ok(true),
        };
        if !self.prepare_state(io, &grab.source, target, steps)? {
            return Ok(false);
        }
        let state = self.state(io, &grab.source, target)?;
        if grab.phase == 0 {
            self.cache_states[grab.output.cache * STATE_BANKS + 4] = state;
        } else {
            self.cache_states[grab.output.cache * STATE_BANKS + 5] = state;
            grab.output.image.frontier_position = grab.output.frames;
        }
        grab.phase += 1;
        self.invalidate(&grab.output)?;
        Ok(grab.phase == 2)
    }
    pub fn grab_ready(&self, grab: &Grab) -> bool {
        grab.phase == 2
    }
    pub fn finish_grab(&mut self, grab: Grab) -> Media {
        assert!(self.grab_ready(&grab));
        self.release(grab.source).expect("owned preparation source");
        grab.output
    }
    pub fn cancel_grab(&mut self, grab: Grab) -> Result<(), Error> {
        self.release(grab.source)?;
        self.release(grab.output)
    }
    pub fn begin_source_with_bytes(
        &mut self,
        origin: Frame,
        horizon: u64,
        events: usize,
        bytes: usize,
        initial: &MidiState,
    ) -> Result<Source, Error> {
        let tick = self
            .config
            .events_per_tick
            .checked_mul(12)
            .and_then(|n| n.checked_add(self.config.bytes_per_tick))
            .ok_or(Error::Capacity)?;
        if horizon == 0
            || events < self.config.events_per_tick
            || bytes < tick
            || origin.0 % self.q as u64 != 0
        {
            return Err(Error::InvalidConfig);
        }
        Ok(Source {
            media: self.empty(origin, 0, initial)?,
            horizon: Quantum::new(self.q)?.ceil(Frame(horizon))?.0,
            events,
            bytes,
            current: *initial,
        })
    }
    pub fn commit_source<S: Sample>(
        &mut self,
        io: &mut RenderIo<'_, S>,
        source: &mut Source,
        staged: &Staging,
    ) -> Result<(), Error> {
        let next = source.coverage().end;
        if staged.frame != next {
            return Err(Error::InvalidFrame);
        }
        let end = next.checked_add(self.q)?;
        let revision = source
            .media
            .revision
            .checked_add(1)
            .ok_or(Error::Exhausted)?;
        let incoming_bytes = staged.records[..staged.count]
            .iter()
            .map(|row| 12 + row.message().as_bytes().len())
            .sum::<usize>();
        let keep_events = source
            .events
            .checked_sub(staged.count)
            .ok_or(Error::Capacity)?;
        let keep_bytes = source
            .bytes
            .checked_sub(incoming_bytes)
            .ok_or(Error::Capacity)?;
        let total = self.blocks.totals(source.media.version)?;
        let mut events = total.events as usize;
        let mut bytes = total.bytes as usize;
        let mut origin = source
            .media
            .origin
            .0
            .max(end.0.saturating_sub(source.horizon));
        let mut initial = None;
        let mut after = None;
        let mut discarded = 0;
        loop {
            let Some(record) = self.blocks.first(source.media.version, after)? else {
                break;
            };
            if record.frame.0 >= origin && events <= keep_events && bytes <= keep_bytes {
                break;
            }
            if discarded >= 3 * self.config.events_per_tick + 1 {
                return Err(Error::Capacity);
            }
            origin = origin.max(Quantum::new(self.q)?.ceil(record.frame.checked_add(1)?)?.0);
            Self::apply_state(
                io,
                record.message(),
                initial.get_or_insert(*self.initial(&source.media)),
            )?;
            events -= 1;
            bytes -= 12 + record.message().as_bytes().len();
            discarded += 1;
            after = Some((record.frame, record.order));
        }
        if staged.count == 0 && discarded == 0 {
            source.media.image.origin = Frame(origin);
            source.media.image.frames = end.0 - origin;
            source.media.image.frontier_position = source.media.frames;
            source.media.image.revision = revision;
            source.media.image.cut = end;
            return self.account(io);
        }
        let clipped = self
            .blocks
            .clip(source.media.version, Frame(origin)..next)?;
        let normalized = self.blocks.normalize(clipped);
        self.blocks.release(clipped)?;
        let normalized = normalized?;
        let changed = self
            .blocks
            .append(normalized, &staged.records[..staged.count]);
        self.blocks.release(normalized)?;
        let changed = changed?;
        let mut current = source.current;
        for record in &staged.records[..staged.count] {
            Self::apply_state(io, record.message(), &mut current)?;
        }
        self.invalidate(&source.media)?;
        self.blocks.release(source.media.version)?;
        source.media.image.version = changed;
        source.media.identity = changed;
        source.media.image.origin = Frame(origin);
        source.media.image.frames = end.0 - origin;
        if let Some(initial) = initial {
            self.cache_states[source.media.cache * STATE_BANKS + 4] = initial;
        }
        self.cache_states[source.media.cache * STATE_BANKS + 5] = current;
        source.media.image.frontier_position = source.media.frames;
        source.media.image.revision = revision;
        source.media.image.cut = end;
        let prior = source.current;
        source.current = current;
        self.publish_position(&source.media, next.0 - origin, &prior)?;
        io.store
            .charge_work(WorkClass::Render, 3 * size_of::<MidiState>(), 0)?;
        self.account(io)
    }
    pub fn grab_source<S: Sample>(
        &mut self,
        io: &mut RenderIo<'_, S>,
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
        if self.caches.iter().all(Option::is_some) {
            return Err(Error::Capacity);
        }
        let initial = self.state(io, &source.media, range.start.0 - coverage.start.0)?;
        let clipped = self.blocks.clip(source.media.version, range.clone())?;
        let normalized = self.blocks.normalize(clipped);
        self.blocks.release(clipped)?;
        let version = normalized?;
        let pinned_events = self.blocks.totals(version)?.events as usize;
        let complete_end = range.end == coverage.end;
        let mut media = self
            .own(MediaImage {
                version,
                origin: range.start,
                frames,
                revision: 0,
                cut: range.end,
                initial,
                frontier: if complete_end {
                    source.current
                } else {
                    initial
                },
                frontier_position: if complete_end { frames } else { 0 },
            })
            .map_err(|(error, _)| error)
            .expect("reserved cache");
        if let Err(error) = self.charge_owner(&mut media, pinned_events) {
            self.release(media)?;
            return Err(error);
        }
        self.account(io)?;
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
