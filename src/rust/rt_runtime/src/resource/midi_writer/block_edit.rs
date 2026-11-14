use super::*;

pub(in crate::resource) struct Patch {
    media: Media,
    slot: usize,
    count: usize,
    position: u64,
    frames: u64,
    done: u64,
}
impl Core {
    pub fn begin_patch<S: Sample>(
        &mut self,
        io: &mut RenderIo<'_, S>,
        source: &Media,
        position: u64,
        frames: usize,
        input: &MidiBuffer,
        initial: &MidiState,
    ) -> Result<Patch, Error> {
        let frames = u64::try_from(frames).map_err(|_| Error::Capacity)?;
        if position >= source.frames
            || frames > source.frames
            || !initial.is_basic()
            || input.input_lost()
        {
            return Err(Error::InvalidFrame);
        }
        position.checked_add(frames).ok_or(Error::Exhausted)?;
        if input.len() > self.config.events_per_tick
            || input.byte_len() > self.config.bytes_per_tick
        {
            return Err(Error::Capacity);
        }
        for index in 0..input.len() {
            let (at, bytes) = input.get(index).ok_or(Error::InvalidEvent)?;
            if u64::from(at) >= frames {
                return Err(Error::InvalidFrame);
            }
            ShortMessage::managed(bytes).map_err(Error::from)?;
        }
        let slot = self
            .edit_busy
            .iter()
            .position(|busy| !busy)
            .ok_or(Error::Capacity)?;
        self.maintain(io, self.required_maintenance(1))?;
        let mut media = self.retain(source)?;
        let count = self.blocks.totals(media.version)?.events as usize;
        if let Err(error) = self.charge_owner(
            &mut media,
            count.checked_add(input.len()).ok_or(Error::Capacity)?,
        ) {
            self.release(media)?;
            return Err(error);
        }
        if frames != 0 && (position == 0 || position + frames > source.frames) {
            let zero = if position == 0 {
                0
            } else {
                source.frames - position
            };
            self.cache_states[media.cache * STATE_BANKS + 4] = *initial;
            io.store.charge_work(
                WorkClass::Render,
                size_of::<MidiState>(),
                crate::midi_state::BASIC_STATE_CELLS,
            )?;
            for index in 0..input.len() {
                let (at, bytes) = input.get(index).unwrap();
                if u64::from(at) >= zero {
                    break;
                }
                Self::apply_state(
                    io,
                    ShortMessage::managed(bytes).map_err(Error::from)?,
                    &mut self.cache_states[media.cache * STATE_BANKS + 4],
                )?;
            }
        }
        for index in 0..input.len() {
            let (at, bytes) = input.get(index).unwrap();
            self.edit_records[slot * self.config.events_per_tick + index] =
                EventRecord::new(Frame(u64::from(at)), index as u64, bytes)?;
        }
        self.edit_busy[slot] = true;
        self.invalidate(&media)?;
        self.account(io)?;
        Ok(Patch {
            media,
            slot,
            count: input.len(),
            position,
            frames,
            done: 0,
        })
    }
    pub fn progress_patch<S: Sample>(
        &mut self,
        io: &mut RenderIo<'_, S>,
        patch: &mut Patch,
        steps: usize,
    ) -> Result<bool, Error> {
        for _ in 0..steps {
            if patch.done == patch.frames {
                break;
            }
            self.maintain(io, self.required_maintenance(1))?;
            let position = (patch.position + patch.done) % patch.media.frames;
            let remaining = (patch.frames - patch.done).min(patch.media.frames - position);
            let start = patch.media.origin.0 + position;
            let after = start.checked_sub(1).map(|frame| (Frame(frame), u16::MAX));
            let old = self
                .blocks
                .first(patch.media.version, after)?
                .filter(|record| record.frame.0 < start + remaining)
                .map(|record| record.frame.0 - start);
            let records =
                &self.edit_records[patch.slot * self.config.events_per_tick..][..patch.count];
            let new = records
                .iter()
                .find(|record| {
                    record.frame.0 >= patch.done && record.frame.0 < patch.done + remaining
                })
                .map(|record| record.frame.0 - patch.done);
            let skip = old.into_iter().chain(new).min().unwrap_or(remaining);
            patch.done += skip;
            if skip == remaining {
                continue;
            }
            let width = (remaining - skip).min(self.q as u64);
            let start = start + skip;
            let mut count = 0;
            for record in records.iter().filter(|record| {
                record.frame.0 >= patch.done && record.frame.0 < patch.done + width
            }) {
                let mut mapped = *record;
                mapped.frame = Frame(start + record.frame.0 - patch.done);
                self.edit_scratch[count] = mapped;
                count += 1;
            }
            for bucket in (start - patch.media.origin.0) / self.q as u64
                ..=(start + width - 1 - patch.media.origin.0) / self.q as u64
            {
                let low = patch.media.origin.0 + bucket * self.q as u64;
                let high = (low + self.q as u64).min(patch.media.origin.0 + patch.media.frames);
                let old = self
                    .blocks
                    .range_totals(patch.media.version, Frame(low)..Frame(high))?;
                let removed = self.blocks.range_totals(
                    patch.media.version,
                    Frame(low.max(start))..Frame(high.min(start + width)),
                )?;
                let mut events = old.events - removed.events;
                let mut bytes = old.bytes - removed.bytes;
                for record in self.edit_scratch[..count]
                    .iter()
                    .filter(|r| r.frame.0 >= low && r.frame.0 < high)
                {
                    events += 1;
                    bytes += 12 + record.message().as_bytes().len() as u64;
                }
                if events > self.config.events_per_tick as u64 * 2
                    || bytes - 12 * events > self.config.bytes_per_tick as u64 * 2
                {
                    return Err(Error::Capacity);
                }
            }
            let next = self.blocks.edit(
                patch.media.version,
                Frame(start)..Frame(start + width),
                &self.edit_scratch[..count],
            )?;
            self.blocks.release(patch.media.version)?;
            patch.media.image.version = next;
            patch.media.identity = next;
            patch.done += width;
            self.account(io)?;
        }
        Ok(patch.done == patch.frames)
    }
    pub fn finish_patch(&mut self, mut patch: Patch) -> Result<Media, (Error, Patch)> {
        if patch.done != patch.frames {
            return Err((Error::Busy, patch));
        }
        self.cache_states[patch.media.cache * STATE_BANKS + 5] = *self.initial(&patch.media);
        patch.media.image.frontier_position = 0;
        self.edit_busy[patch.slot] = false;
        Ok(patch.media)
    }
    pub fn cancel_patch(&mut self, patch: Patch) -> Result<(), Error> {
        self.edit_busy[patch.slot] = false;
        self.release(patch.media)
    }
}
