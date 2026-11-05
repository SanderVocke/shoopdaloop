use super::*;
use crate::buffer::MidiBuffer;

/// Owning chunk buffers are prepared by control and returned in the reliable
/// completion, including on refusal/cancellation. Nothing is freed by rendering.
pub enum Request {
    Create,
    AudioGrab {
        store: AudioStoreId,
        start: Frame,
        end: Frame,
    },
    AudioFork {
        store: AudioStoreId,
        snapshot: SnapshotId,
    },
    MidiGrab {
        store: MidiStoreId,
        start: Frame,
        end: Frame,
    },
    MidiFork {
        store: MidiStoreId,
        snapshot: SnapshotId,
    },
    Remove {
        resource: ResourceId,
    },
    BeginUpload {
        resource: ResourceId,
        expected_revision: Revision,
        description: Description,
    },
    BeginIndexedMidiUpload {
        resource: ResourceId,
        expected_revision: Revision,
        description: Description,
        initial: Box<crate::midi_state::MidiState>,
    },
    Write {
        upload: UploadId,
        offset: usize,
        bytes: Box<[u8]>,
    },
    /// Write a prefix while returning the complete reusable buffer to control.
    WritePrefix {
        upload: UploadId,
        offset: usize,
        bytes: Box<[u8]>,
        used: usize,
    },
    Finish {
        upload: UploadId,
    },
    MidiInitial {
        upload: UploadId,
        packet: [u8; 3],
        len: u8,
    },
    Commit {
        upload: UploadId,
    },
    CancelUpload {
        upload: UploadId,
    },
    Snapshot {
        resource: ResourceId,
    },
    Cursor {
        snapshot: SnapshotId,
    },
    Read {
        cursor: Cursor,
        bytes: Box<[u8]>,
    },
    ReleaseSnapshot {
        snapshot: SnapshotId,
    },
    Currentness {
        snapshot: SnapshotId,
    },
    Info {
        resource: ResourceId,
    },
    BeginCapture {
        resource: ResourceId,
        expected_revision: Revision,
        format: Format,
    },
    StopCapture {
        capture: CaptureId,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reply {
    Done,
    Created(ResourceId),
    Upload(UploadId),
    Revision(Revision),
    Snapshot(Snapshot),
    Cursor(Cursor),
    Read { written: usize, eof: bool },
    Currentness(Currentness),
    Info(ResourceInfo),
    Capture(CaptureId),
}

pub struct ResourceCommand {
    request: Request,
    reply: Option<Reply>,
}
impl ResourceCommand {
    pub fn new(request: Request) -> Self {
        Self {
            request,
            reply: None,
        }
    }
    pub fn reply(&self) -> Option<Reply> {
        self.reply
    }
    pub fn request(&self) -> &Request {
        &self.request
    }
    pub fn into_request(self) -> Request {
        self.request
    }
    pub(crate) fn reset_reply(&mut self) {
        self.reply = None;
    }
    pub(crate) fn recovery_safe(&self) -> bool {
        matches!(
            self.request,
            Request::CancelUpload { .. }
                | Request::ReleaseSnapshot { .. }
                | Request::StopCapture { .. }
        )
    }
    pub(crate) fn envelope(&self, config: PreparedConfig) -> Result<(), Error> {
        if let Request::Write { bytes, .. }
        | Request::WritePrefix { bytes, .. }
        | Request::Read { bytes, .. } = &self.request
        {
            if bytes.len() > config.limits().page_bytes {
                return Err(Error::Capacity);
            }
        }
        if matches!(&self.request, Request::WritePrefix { bytes, used, .. } if *used == 0 || *used > bytes.len())
        {
            return Err(Error::InvalidShape);
        }
        Ok(())
    }
    pub(crate) fn apply(&mut self, store: &mut ResourceStore, frame: Frame) -> Result<(), Error> {
        self.reply = None;
        self.envelope(store.config())?;
        let reply = match &mut self.request {
            Request::Create => Reply::Created(store.create()?),
            Request::AudioGrab {
                store: audio,
                start,
                end,
            } => Reply::Created(store.grab_audio(*audio, *start..*end)?),
            Request::AudioFork {
                store: audio,
                snapshot,
            } => Reply::Created(store.fork_audio(*audio, *snapshot)?),
            Request::MidiGrab {
                store: midi,
                start,
                end,
            } => Reply::Created(store.grab_midi(*midi, *start..*end)?),
            Request::MidiFork {
                store: midi,
                snapshot,
            } => Reply::Created(store.fork_midi(*midi, *snapshot)?),
            Request::Remove { resource } => {
                store.remove(*resource)?;
                Reply::Done
            }
            Request::BeginUpload {
                resource,
                expected_revision,
                description,
            } => Reply::Upload(store.begin_upload(*resource, *expected_revision, *description)?),
            Request::BeginIndexedMidiUpload {
                resource,
                expected_revision,
                description,
                initial,
            } => Reply::Upload(store.begin_indexed_midi_upload(
                *resource,
                *expected_revision,
                *description,
                initial,
            )?),
            Request::Write {
                upload,
                offset,
                bytes,
            } => {
                store.write_upload(*upload, *offset, bytes)?;
                Reply::Done
            }
            Request::WritePrefix {
                upload,
                offset,
                bytes,
                used,
            } => {
                store.write_upload(*upload, *offset, &bytes[..*used])?;
                Reply::Done
            }
            Request::Finish { upload } => {
                store.finish_upload(*upload)?;
                Reply::Done
            }
            Request::MidiInitial {
                upload,
                packet,
                len,
            } => {
                let bytes = packet.get(..usize::from(*len)).ok_or(Error::InvalidShape)?;
                store.upload_midi_initial(*upload, bytes)?;
                Reply::Done
            }
            Request::Commit { upload } => Reply::Revision(store.commit(*upload)?),
            Request::CancelUpload { upload } => {
                store.cancel_upload(*upload)?;
                Reply::Done
            }
            Request::Snapshot { resource } => Reply::Snapshot(store.snapshot(*resource, frame)?),
            Request::Cursor { snapshot } => Reply::Cursor(store.cursor(*snapshot)?),
            Request::Read { cursor, bytes } => {
                let written = store.read(cursor, bytes)?;
                let lease = get(
                    &store.snapshots,
                    cursor.snapshot.slot,
                    cursor.snapshot.generation,
                )?;
                Reply::Read {
                    written,
                    eof: cursor.position == lease.snapshot.description.bytes,
                }
            }
            Request::ReleaseSnapshot { snapshot } => {
                store.release_snapshot(*snapshot)?;
                Reply::Done
            }
            Request::Currentness { snapshot } => Reply::Currentness(store.currentness(*snapshot)?),
            Request::Info { resource } => Reply::Info(store.info(*resource)?),
            Request::BeginCapture {
                resource,
                expected_revision,
                format,
            } => Reply::Capture(store.begin_capture(
                *resource,
                *expected_revision,
                *format,
                frame,
            )?),
            Request::StopCapture { capture } => Reply::Revision(store.stop_capture(*capture)?),
        };
        self.reply = Some(reply);
        Ok(())
    }
}

/// A processor view cannot replenish budgets or apply structural resource edits.
/// Reborrowing it for another node/subrange keeps the same callback counters.
pub struct ResourceIo<'a> {
    pub(super) store: &'a mut ResourceStore,
}
impl ResourceStore {
    pub fn io(&mut self) -> ResourceIo<'_> {
        ResourceIo { store: self }
    }
}
impl ResourceIo<'_> {
    pub fn remove(&mut self, resource: ResourceId) -> Result<(), Error> {
        self.store.remove(resource)
    }
    pub fn validate_writer(
        &mut self,
        resource: ResourceId,
        revision: Revision,
    ) -> Result<(), Error> {
        let owner = self.store.check_writer(resource, None)?;
        if owner.revision != revision {
            return Err(Error::StaleRevision);
        }
        self.store.note_operation(WorkClass::Render);
        self.store
            .charge_work(WorkClass::Render, 0, self.store.versions.len())?;
        if self
            .store
            .versions
            .iter()
            .filter_map(|slot| slot.value)
            .any(|version| {
                version.resource == resource
                    && matches!(
                        version.state,
                        State::Receiving
                            | State::Complete
                            | State::PreparedCapture
                            | State::Capturing
                    )
            })
        {
            return Err(Error::Busy);
        }
        Ok(())
    }
    /// Borrow required data access without consuming or replenishing background quotas.
    pub fn render(&mut self) -> RenderIo<'_> {
        self.store.render_io()
    }
    pub fn copy_remaining(&self) -> usize {
        self.store.copy_remaining()
    }
    pub fn steps_remaining(&self) -> usize {
        self.store.steps_remaining()
    }
    pub fn info(&self, resource: ResourceId) -> Result<ResourceInfo, Error> {
        self.store.info(resource)
    }
    pub fn midi_index_info(&self, resource: ResourceId) -> Result<MidiIndexInfo, Error> {
        self.store.midi_index_info(resource)
    }
    pub fn active_capture(&self, resource: ResourceId) -> Result<CaptureId, Error> {
        self.store.active_capture(resource)
    }
    pub fn snapshot(&mut self, resource: ResourceId, frame: Frame) -> Result<Snapshot, Error> {
        self.store.snapshot(resource, frame)
    }
    pub fn currentness(&self, snapshot: SnapshotId) -> Result<Currentness, Error> {
        self.store.currentness(snapshot)
    }
    pub fn cursor(&self, snapshot: SnapshotId) -> Result<Cursor, Error> {
        self.store.cursor(snapshot)
    }
    pub fn read_index(&self, snapshot: SnapshotId) -> Result<ReadIndex, Error> {
        self.store.read_index(snapshot)
    }
    pub fn read(&mut self, cursor: &mut Cursor, bytes: &mut [u8]) -> Result<usize, Error> {
        self.store.read(cursor, bytes)
    }
    pub fn reserve_snapshot(&mut self) -> Result<SnapshotReservation, Error> {
        self.store.reserve_snapshot()
    }
    pub fn cancel_snapshot_reservation(
        &mut self,
        reservation: SnapshotReservation,
    ) -> Result<(), Error> {
        self.store.cancel_snapshot_reservation(reservation)
    }
    pub fn snapshot_reserved(
        &mut self,
        reservation: SnapshotReservation,
        resource: ResourceId,
        frame: Frame,
    ) -> Result<Snapshot, Error> {
        self.store.snapshot_reserved(reservation, resource, frame)
    }
    pub fn retain_snapshot(&mut self, snapshot: SnapshotId) -> Result<Snapshot, Error> {
        self.store.retain_snapshot(snapshot)
    }
    pub fn release_snapshot(&mut self, snapshot: SnapshotId) -> Result<(), Error> {
        self.store.release_snapshot(snapshot)
    }
    pub fn capture_audio(
        &mut self,
        capture: CaptureId,
        frame: Frame,
        samples: &[f32],
    ) -> Result<Revision, Error> {
        self.store.capture_audio(capture, frame, samples)
    }
    pub fn capture_midi(
        &mut self,
        capture: CaptureId,
        frame: Frame,
        frames: usize,
        input: &MidiBuffer,
    ) -> Result<Revision, Error> {
        self.store.capture_midi(capture, frame, frames, input)
    }
}
