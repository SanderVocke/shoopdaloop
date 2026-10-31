//! All-member interval reservation, staging, and publication.
use super::*;
use crate::buffer::MidiBuffer;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptureIntervalId {
    take: CaptureTakeId,
    sequence: u64,
}
#[derive(Clone, Copy)]
pub(super) struct AppendReservation {
    pages: usize,
    revision: Revision,
    ready: bool,
    bytes: usize,
    events: usize,
}
impl ResourceStore {
    pub(in crate::resource) fn capture_interval_end(
        &self,
        interval: CaptureIntervalId,
    ) -> Result<Frame, Error> {
        let group = self.capture_interval_info(interval)?;
        group
            .next_frame
            .ok_or(Error::InvalidState)?
            .checked_add(group.interval_frames)
    }
    fn capture_interval_info(
        &self,
        interval: CaptureIntervalId,
    ) -> Result<CaptureGroupInfo, Error> {
        let group = self.capture_take_info(interval.take)?;
        if group.interval_sequence != interval.sequence {
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
    fn interval_member(
        &self,
        interval: CaptureIntervalId,
        ordinal: usize,
    ) -> Result<(CaptureGroupInfo, Member), Error> {
        let group = self.capture_interval_info(interval)?;
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
    fn interval_bound(&self, member: Member, frames: usize) -> usize {
        match member.spec.format {
            Format::Audio { channels } => frames * channels as usize * 4,
            Format::Midi => member.staging_bytes,
        }
    }
    /// Reserve the first interval without replacing active resource versions.
    /// Staging and commit remain unavailable until activation at `at`.
    pub fn reserve_prepared_capture_interval(
        &mut self,
        take: CaptureTakeId,
        at: Frame,
        frames: usize,
    ) -> Result<CaptureIntervalId, Error> {
        self.reserve_capture_interval_with(take, at, frames, true)
    }
    pub fn reserve_capture_interval(
        &mut self,
        take: CaptureTakeId,
        at: Frame,
        frames: usize,
    ) -> Result<CaptureIntervalId, Error> {
        self.reserve_capture_interval_with(take, at, frames, false)
    }
    fn reserve_capture_interval_with(
        &mut self,
        take: CaptureTakeId,
        at: Frame,
        frames: usize,
        prepared: bool,
    ) -> Result<CaptureIntervalId, Error> {
        self.note_operation(WorkClass::Render);
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
        if frames == 0 || frames > self.config.limits().max_frames {
            return Err(Error::InvalidShape);
        }
        if !prepared && group.next_frame != Some(at) {
            return Err(Error::InvalidFrame);
        }
        at.checked_add(frames)?;
        let sequence = group
            .interval_sequence
            .checked_add(1)
            .ok_or(Error::Exhausted)?;
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
                .checked_add(
                    self.append_pages(version.written, self.interval_bound(member, frames))?,
                )
                .ok_or(Error::Capacity)?;
            if let Some(owner) = version.midi {
                self.midi_indexes
                    .preflight(owner, id, member.spec.midi_events as u64)?;
            }
        }
        if pages > self.free_pages.len() - self.reserved_pages {
            return Err(Error::Capacity);
        }
        self.reserved_pages += pages;
        for index in 0..group.members {
            let mut member = self.capture_groups.member(take.group, index);
            let id = member.take.expect("validated take");
            let version = get(&self.versions, id.slot, id.generation).expect("validated take");
            member.append = Some(AppendReservation {
                pages: self
                    .append_pages(version.written, self.interval_bound(member, frames))
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
        group.interval_frames = frames;
        group.interval_sequence = sequence;
        self.capture_groups.groups[take.group.slot as usize].value = Some(group);
        Ok(CaptureIntervalId { take, sequence })
    }
    pub fn cancel_capture_interval(&mut self, interval: CaptureIntervalId) -> Result<(), Error> {
        self.note_operation(WorkClass::Render);
        let mut group = self.capture_interval_info(interval)?;
        for index in 0..group.members {
            let mut member = self.capture_groups.member(interval.take.group, index);
            self.reserved_pages -= member.append.take().expect("interval reservation").pages;
            self.capture_groups
                .set_member(interval.take.group, index, member);
        }
        if group.phase == CapturePhase::PreparedReserved {
            group.phase = CapturePhase::Prepared;
            group.next_frame = None;
        } else {
            group.phase = CapturePhase::Recording;
        }
        group.interval_frames = 0;
        self.capture_groups.groups[interval.take.group.slot as usize].value = Some(group);
        Ok(())
    }
    pub fn stage_capture_audio(
        &mut self,
        interval: CaptureIntervalId,
        ordinal: usize,
        samples: &[f32],
    ) -> Result<(), Error> {
        self.note_operation(WorkClass::Render);
        let (group, mut member) = self.interval_member(interval, ordinal)?;
        let Format::Audio { channels } = member.spec.format else {
            return Err(Error::InvalidShape);
        };
        let mut reservation = member.append.ok_or(Error::InvalidState)?;
        if reservation.ready {
            return Err(Error::InvalidState);
        }
        if samples.len() != group.interval_frames * channels as usize {
            return Err(Error::InvalidShape);
        }
        let bytes = samples.len() * 4;
        self.charge_work(WorkClass::Render, bytes, 0)?;
        for (index, sample) in samples.iter().enumerate() {
            let start = member.staging_start + index * 4;
            self.capture_groups.staging[start..start + 4].copy_from_slice(&sample.to_le_bytes());
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
        interval: CaptureIntervalId,
        ordinal: usize,
        input: &MidiBuffer,
    ) -> Result<(), Error> {
        self.note_operation(WorkClass::Render);
        let (group, mut member) = self.interval_member(interval, ordinal)?;
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
            if offset as usize >= group.interval_frames
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
    pub fn commit_capture_interval(&mut self, interval: CaptureIntervalId) -> Result<(), Error> {
        self.note_operation(WorkClass::Render);
        let mut group = self.capture_interval_info(interval)?;
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
            version.description.frames += group.interval_frames as u64;
            version.revision = version.revision.checked_next().expect("reserved revision");
            version
                .capture
                .as_mut()
                .expect("capture metadata")
                .next_frame = group
                .next_frame
                .expect("running boundary")
                .checked_add(group.interval_frames)
                .expect("reserved frame");
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
                self.append_group_staging(&mut version, member.staging_start, reservation.bytes);
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
                .checked_add(group.interval_frames)
                .expect("reserved frame"),
        );
        group.interval_frames = 0;
        group.phase = CapturePhase::Recording;
        self.capture_groups.groups[interval.take.group.slot as usize].value = Some(group);
        Ok(())
    }
}
