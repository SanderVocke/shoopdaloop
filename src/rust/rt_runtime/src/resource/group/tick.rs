//! All-member interval reservation, staging, and publication.
use super::*;
use crate::buffer::MidiBuffer;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptureTickId {
    take: CaptureTakeId,
    sequence: u64,
}
#[derive(Clone, Copy)]
pub(super) struct TickReservation {
    pages: usize,
    revision: Revision,
    ready: bool,
    bytes: usize,
    events: usize,
}
impl ResourceStore {
    pub(in crate::resource) fn capture_tick_end(
        &self,
        interval: CaptureTickId,
    ) -> Result<Frame, Error> {
        let group = self.capture_tick_info(interval)?;
        group
            .next_frame
            .ok_or(Error::InvalidState)?
            .checked_add(self.quantum.frames())
    }
    fn capture_tick_info(&self, interval: CaptureTickId) -> Result<CaptureGroupInfo, Error> {
        let group = self.capture_take_info(interval.take)?;
        if group.tick_sequence != interval.sequence {
            return Err(Error::InvalidId);
        }
        if !matches!(
            group.phase,
            CapturePhase::Reserved | CapturePhase::PreparedReserved
        ) {
            return Err(Error::InvalidState);
        }
        Ok(group)
    }
    fn tick_member(
        &self,
        interval: CaptureTickId,
        ordinal: usize,
    ) -> Result<(CaptureGroupInfo, Member), Error> {
        let group = self.capture_tick_info(interval)?;
        if group.phase != CapturePhase::Reserved {
            return Err(Error::InvalidState);
        }
        if ordinal >= group.members {
            return Err(Error::InvalidOffset);
        }
        Ok((
            group,
            self.capture_groups.member(interval.take.group, ordinal),
        ))
    }
    fn append_pages(&self, written: usize, bytes: usize) -> Result<usize, Error> {
        let end = written.checked_add(bytes).ok_or(Error::Capacity)?;
        let page = self.config.limits().page_bytes;
        Ok(end.div_ceil(page) - written.div_ceil(page))
    }
    fn tick_bound(&self, member: Member) -> usize {
        match member.spec.format {
            Format::Audio { channels } => self.quantum.frames() * channels as usize * 4,
            Format::Midi => member.staging_bytes,
        }
    }
    /// Reserve the first interval without replacing active resource versions.
    /// Staging and commit remain unavailable until activation at `at`.
    pub fn reserve_prepared_capture_tick(
        &mut self,
        take: CaptureTakeId,
        at: Frame,
    ) -> Result<CaptureTickId, Error> {
        self.reserve_capture_tick_with(take, at, true)
    }
    pub fn reserve_capture_tick(
        &mut self,
        take: CaptureTakeId,
        at: Frame,
    ) -> Result<CaptureTickId, Error> {
        self.reserve_capture_tick_with(take, at, false)
    }
    fn reserve_capture_tick_with(
        &mut self,
        take: CaptureTakeId,
        at: Frame,
        prepared: bool,
    ) -> Result<CaptureTickId, Error> {
        self.note_operation(WorkClass::Render);
        let frames = self.quantum.frames();
        let mut group = self.capture_take_info(take)?;
        if group.phase
            != if prepared {
                CapturePhase::Prepared
            } else {
                CapturePhase::Recording
            }
        {
            return Err(Error::InvalidState);
        }
        if !self.quantum.aligned(at) {
            return Err(Error::InvalidFrame);
        }
        if !prepared && group.next_frame != Some(at) {
            return Err(Error::InvalidFrame);
        }
        at.checked_add(frames)?;
        let sequence = group.tick_sequence.checked_add(1).ok_or(Error::Exhausted)?;
        let mut pages = 0usize;
        for index in 0..group.members {
            let member = self.capture_groups.member(take.group, index);
            let id = member.take.ok_or(Error::InvalidState)?;
            let version = get(&self.versions, id.slot, id.generation)?;
            let resource = self.check_writer(member.spec.resource, Some(take.group))?;
            if prepared {
                if version.state != State::PreparedCapture {
                    return Err(Error::InvalidState);
                }
                if resource.revision != version.expected_revision {
                    return Err(Error::StaleRevision);
                }
                version.revision.checked_next()?.checked_next()?;
            } else {
                if resource.active != Some(id)
                    || version.state != State::Capturing
                    || version.capture.ok_or(Error::InvalidState)?.next_frame != at
                {
                    return Err(Error::InvalidState);
                }
                version.revision.checked_next()?;
            }
            version
                .description
                .frames
                .checked_add(frames as u64)
                .ok_or(Error::Exhausted)?;
            pages = pages
                .checked_add(self.append_pages(version.written, self.tick_bound(member))?)
                .ok_or(Error::Capacity)?;
            if let Some(owner) = version.midi {
                self.midi_indexes
                    .preflight(owner, id, member.spec.midi_events as u64)?;
            }
        }
        if pages > self.free_pages.len() - self.reserved_pages {
            return Err(Error::Capacity);
        }
        for index in 0..group.members {
            let member = self.capture_groups.member(take.group, index);
            let id = member.take.expect("validated take");
            let version = get(&self.versions, id.slot, id.generation).expect("validated version");
            if let Some(audio) = version.audio {
                let export = member.audio_export.ok_or(Error::InvalidState)?;
                let result = self.with_audio(audio.store, |store, pool| {
                    pool.reserve_capture_block(&mut store.render_io(), audio.take, export)
                });
                match result {
                    Ok(write) => {
                        let slot = self.capture_groups.member_index(take.group, index);
                        self.capture_groups.audio_writes[slot] = Some(CaptureAudioWrite {
                            store: audio.store,
                            write,
                        });
                    }
                    Err(error) => {
                        for previous in 0..index {
                            let slot = self.capture_groups.member_index(take.group, previous);
                            if let Some(write) = self.capture_groups.audio_writes[slot].take() {
                                self.with_audio(write.store, |store, pool| {
                                    pool.cancel_capture_block(&mut store.render_io(), write.write)
                                })?;
                            }
                        }
                        return Err(error);
                    }
                }
            }
        }
        self.reserved_pages += pages;
        for index in 0..group.members {
            let mut member = self.capture_groups.member(take.group, index);
            let id = member.take.expect("validated take");
            let version = get(&self.versions, id.slot, id.generation).expect("validated take");
            member.append = Some(TickReservation {
                pages: self
                    .append_pages(version.written, self.tick_bound(member))
                    .expect("validated append size"),
                revision: if prepared {
                    version
                        .revision
                        .checked_next()
                        .expect("reserved activation revision")
                } else {
                    version.revision
                },
                ready: false,
                bytes: 0,
                events: 0,
            });
            self.capture_groups.set_member(take.group, index, member);
        }
        group.phase = if prepared {
            CapturePhase::PreparedReserved
        } else {
            CapturePhase::Reserved
        };
        group.next_frame = Some(at);
        group.tick_sequence = sequence;
        self.capture_groups.groups[take.group.slot as usize].value = Some(group);
        Ok(CaptureTickId { take, sequence })
    }
    pub fn cancel_capture_tick(&mut self, interval: CaptureTickId) -> Result<(), Error> {
        self.note_operation(WorkClass::Render);
        let mut group = self.capture_tick_info(interval)?;
        for index in 0..group.members {
            let mut member = self.capture_groups.member(interval.take.group, index);
            self.reserved_pages -= member.append.take().expect("interval reservation").pages;
            let slot = self.capture_groups.member_index(interval.take.group, index);
            if let Some(write) = self.capture_groups.audio_writes[slot].take() {
                self.with_audio(write.store, |store, pool| {
                    pool.cancel_capture_block(&mut store.render_io(), write.write)
                })?;
            }
            self.capture_groups
                .set_member(interval.take.group, index, member);
        }
        if group.phase == CapturePhase::PreparedReserved {
            group.phase = CapturePhase::Prepared;
            group.next_frame = None;
        } else {
            group.phase = CapturePhase::Recording;
        }
        self.capture_groups.groups[interval.take.group.slot as usize].value = Some(group);
        Ok(())
    }
    pub fn stage_capture_audio(
        &mut self,
        interval: CaptureTickId,
        ordinal: usize,
        samples: &[f32],
    ) -> Result<(), Error> {
        self.note_operation(WorkClass::Render);
        let (_, mut member) = self.tick_member(interval, ordinal)?;
        let Format::Audio { channels } = member.spec.format else {
            return Err(Error::InvalidShape);
        };
        let mut reservation = member.append.ok_or(Error::InvalidState)?;
        if reservation.ready {
            return Err(Error::InvalidState);
        }
        if samples.len() != self.quantum.frames() * channels as usize {
            return Err(Error::InvalidShape);
        }
        let bytes = samples.len() * 4;
        let slot = self
            .capture_groups
            .member_index(interval.take.group, ordinal);
        if let Some(write) = self.capture_groups.audio_writes[slot].take() {
            let result = self.with_audio(write.store, |store, pool| {
                pool.fill_capture_block(&mut store.render_io(), &write.write, samples)
            });
            self.capture_groups.audio_writes[slot] = Some(write);
            result?;
        } else {
            self.charge_work(WorkClass::Render, bytes, 0)?;
            for (index, sample) in samples.iter().enumerate() {
                let start = member.staging_start + index * 4;
                self.capture_groups.staging[start..start + 4]
                    .copy_from_slice(&sample.to_le_bytes());
            }
        }
        reservation.ready = true;
        reservation.bytes = bytes;
        member.append = Some(reservation);
        self.capture_groups
            .set_member(interval.take.group, ordinal, member);
        Ok(())
    }
    pub fn stage_capture_midi(
        &mut self,
        interval: CaptureTickId,
        ordinal: usize,
        input: &MidiBuffer,
    ) -> Result<(), Error> {
        self.note_operation(WorkClass::Render);
        let (_, mut member) = self.tick_member(interval, ordinal)?;
        if member.spec.format != Format::Midi {
            return Err(Error::InvalidShape);
        }
        let mut reservation = member.append.ok_or(Error::InvalidState)?;
        if reservation.ready {
            return Err(Error::InvalidState);
        }
        if input.len() > member.spec.midi_events || input.byte_len() > member.spec.midi_bytes {
            return Err(Error::Capacity);
        }
        self.charge_work(WorkClass::Render, 0, input.len())?;
        for index in 0..input.len() {
            let (offset, data) = input.get(index).ok_or(Error::InvalidEvent)?;
            if offset as usize >= self.quantum.frames()
                || data.len() > self.config.limits().midi_bytes_per_port
            {
                return Err(Error::InvalidEvent);
            }
        }
        let id = member.take.ok_or(Error::InvalidState)?;
        let base = get(&self.versions, id.slot, id.generation)?
            .description
            .frames;
        let bytes = input.len() * 12 + input.byte_len();
        self.charge_work(WorkClass::Render, bytes, 0)?;
        let mut start = member.staging_start;
        for index in 0..input.len() {
            let (offset, data) = input.get(index).expect("validated event");
            let frame = base + u64::from(offset);
            self.capture_groups.staging[start..start + 8].copy_from_slice(&frame.to_le_bytes());
            self.capture_groups.staging[start + 8..start + 12]
                .copy_from_slice(&(data.len() as u32).to_le_bytes());
            start += 12;
            self.capture_groups.staging[start..start + data.len()].copy_from_slice(data);
            start += data.len();
        }
        reservation.ready = true;
        reservation.bytes = bytes;
        reservation.events = input.len();
        member.append = Some(reservation);
        self.capture_groups
            .set_member(interval.take.group, ordinal, member);
        Ok(())
    }
    fn append_group_staging(&mut self, version: &mut Version, mut start: usize, mut bytes: usize) {
        let mut chunk = [0; 64];
        while bytes > 0 {
            let count = bytes.min(chunk.len());
            chunk[..count].copy_from_slice(&self.capture_groups.staging[start..start + count]);
            self.append_reserved(version, &chunk[..count], WorkClass::Render);
            start += count;
            bytes -= count;
        }
    }
    pub fn commit_capture_tick(&mut self, interval: CaptureTickId) -> Result<(), Error> {
        self.note_operation(WorkClass::Render);
        let mut group = self.capture_tick_info(interval)?;
        if group.phase != CapturePhase::Reserved {
            return Err(Error::InvalidState);
        }
        for index in 0..group.members {
            let member = self.capture_groups.member(interval.take.group, index);
            let reservation = member.append.ok_or(Error::InvalidState)?;
            if !reservation.ready {
                return Err(Error::Incomplete);
            }
            let id = member.take.ok_or(Error::InvalidState)?;
            let version = get(&self.versions, id.slot, id.generation)?;
            if version.revision != reservation.revision {
                return Err(Error::StaleRevision);
            }
            let actual = self.append_pages(version.written, reservation.bytes)?;
            if actual > reservation.pages {
                return Err(Error::InvalidState);
            }
            if let Some(owner) = version.midi {
                self.midi_indexes
                    .preflight(owner, id, reservation.events as u64)?;
            }
        }
        for index in 0..group.members {
            let mut member = self.capture_groups.member(interval.take.group, index);
            let reservation = member.append.take().expect("validated append");
            let id = member.take.expect("validated take");
            let mut version = get(&self.versions, id.slot, id.generation).expect("validated take");
            let actual = self
                .append_pages(version.written, reservation.bytes)
                .expect("validated append size");
            self.reserved_pages -= reservation.pages - actual;
            version.reserved += actual;
            version.description.bytes += reservation.bytes;
            version.description.frames += self.quantum.frames() as u64;
            version.revision = version.revision.checked_next().expect("reserved revision");
            version
                .capture
                .as_mut()
                .expect("capture metadata")
                .next_frame = group
                .next_frame
                .expect("running boundary")
                .checked_add(self.quantum.frames())
                .expect("reserved frame");
            let slot = self.capture_groups.member_index(interval.take.group, index);
            if let Some(write) = self.capture_groups.audio_writes[slot].take() {
                self.with_audio(write.store, |store, pool| {
                    pool.commit_capture_block(&mut store.render_io(), write.write)
                })
                .expect("reserved capture block");
                version.written += reservation.bytes;
            } else {
                self.charge_work(WorkClass::Render, reservation.bytes * 2, 0)
                    .expect("required copies are not budgeted");
                if let Some(owner) = version.midi {
                    let mut start = member.staging_start;
                    for _ in 0..reservation.events {
                        let frame = u64::from_le_bytes(
                            self.capture_groups.staging[start..start + 8]
                                .try_into()
                                .expect("staged frame"),
                        );
                        let bytes = u32::from_le_bytes(
                            self.capture_groups.staging[start + 8..start + 12]
                                .try_into()
                                .expect("staged length"),
                        ) as usize;
                        let mut short = [0; 3];
                        let count = bytes.min(3);
                        short[..count].copy_from_slice(
                            &self.capture_groups.staging[start + 12..start + 12 + count],
                        );
                        self.append_group_staging(&mut version, start, 12 + bytes);
                        self.midi_indexes.append(
                            owner,
                            format::RecordSummary {
                                frame,
                                end: version.written,
                                short,
                            },
                            WorkClass::Render,
                        );
                        start += 12 + bytes;
                    }
                } else {
                    self.append_group_staging(
                        &mut version,
                        member.staging_start,
                        reservation.bytes,
                    );
                }
            }
            self.publish_capture(
                CaptureId {
                    slot: id.slot,
                    generation: id.generation,
                },
                version,
            );
            self.capture_groups
                .set_member(interval.take.group, index, member);
        }
        group.next_frame = Some(
            group
                .next_frame
                .expect("running boundary")
                .checked_add(self.quantum.frames())
                .expect("reserved frame"),
        );
        group.phase = CapturePhase::Recording;
        self.capture_groups.groups[interval.take.group.slot as usize].value = Some(group);
        Ok(())
    }
}
