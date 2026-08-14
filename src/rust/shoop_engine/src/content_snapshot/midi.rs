use super::manifest::{manifest_pair, ManifestPublisher, ManifestReader};
use super::transport::{bounded_transport, ProcessSender, PublisherReceiver};
use super::{
    ContentMutation, ContentRevision, ContentStatus, MidiContentSnapshot, MidiSnapshotMetadata,
    SessionContentEpoch,
};
use crate::midi_event::MidiEvent;
use crate::midi_storage::{MidiStorageElem, MAX_MSG_BYTES};
use std::collections::HashMap;
use std::fmt;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, Default)]
struct WireMidiEvent {
    time: i32,
    size: u8,
    data: [u8; MAX_MSG_BYTES],
}

impl WireMidiEvent {
    fn set(&mut self, time: i32, data: &[u8]) -> bool {
        if data.is_empty() || data.len() > MAX_MSG_BYTES {
            return false;
        }
        self.time = time;
        self.size = data.len() as u8;
        self.data[..data.len()].copy_from_slice(data);
        true
    }

    fn event(&self) -> MidiEvent {
        MidiEvent {
            time: self.time,
            data: self.data[..self.size as usize].to_vec(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum MidiUpdateKind {
    BeginWorking,
    Append,
    Clear,
    TruncateAfter(i32),
    RemoveRange(i32, i32),
    Publish,
    Install(ContentRevision),
}

#[derive(Debug)]
struct MidiUpdateBlock {
    events: Box<[WireMidiEvent]>,
    used: usize,
    kind: MidiUpdateKind,
    total_length: u32,
    revision: ContentRevision,
    final_block: bool,
    publish: bool,
}

impl MidiUpdateBlock {
    fn new(block_events: usize) -> Self {
        Self {
            events: vec![WireMidiEvent::default(); block_events].into_boxed_slice(),
            used: 0,
            kind: MidiUpdateKind::Publish,
            total_length: 0,
            revision: ContentRevision(0),
            final_block: false,
            publish: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreparedMidiSnapshot {
    revision: ContentRevision,
    length: u32,
}

impl PreparedMidiSnapshot {
    pub fn revision(self) -> ContentRevision {
        self.revision
    }
}

struct PreparedMidiManifest {
    token: PreparedMidiSnapshot,
    chunks: Arc<[Arc<[MidiEvent]>]>,
}

#[derive(Clone)]
pub struct MidiSnapshotControl {
    status: Arc<ContentStatus>,
    prepared: Sender<PreparedMidiManifest>,
    chunk_events: usize,
}

pub struct MidiProcessSnapshotWriter {
    updates: ProcessSender<MidiUpdateBlock>,
    returned: PublisherReceiver<MidiUpdateBlock>,
    free: Vec<MidiUpdateBlock>,
    status: Arc<ContentStatus>,
    latest_revision: ContentRevision,
    latest_length: u32,
    block_events: usize,
    retirement: ProcessSender<Vec<MidiUpdateBlock>>,
}

impl fmt::Debug for MidiProcessSnapshotWriter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MidiProcessSnapshotWriter")
            .field("latest_revision", &self.latest_revision)
            .field("latest_length", &self.latest_length)
            .finish_non_exhaustive()
    }
}

pub struct MidiSnapshotPublisher {
    updates: PublisherReceiver<MidiUpdateBlock>,
    returned: ProcessSender<MidiUpdateBlock>,
    prepared: Receiver<PreparedMidiManifest>,
    prepared_by_revision: HashMap<ContentRevision, PreparedMidiManifest>,
    manifest: ManifestPublisher<MidiContentSnapshot>,
    events: Vec<MidiEvent>,
    committed_events: Vec<MidiEvent>,
    snapshot_chunk_events: usize,
    retirement: PublisherReceiver<Vec<MidiUpdateBlock>>,
    retired: bool,
}

pub type MidiSnapshotReader = ManifestReader<MidiContentSnapshot>;

pub fn midi_snapshot_channel(
    epoch: Arc<SessionContentEpoch>,
    block_events: usize,
    transport_blocks: usize,
) -> (
    MidiProcessSnapshotWriter,
    MidiSnapshotControl,
    MidiSnapshotPublisher,
    MidiSnapshotReader,
) {
    assert!(block_events > 0, "MIDI snapshot block must hold events");
    assert!(
        transport_blocks > 0,
        "MIDI snapshot transport must have blocks"
    );
    let status = Arc::new(ContentStatus::new(epoch));
    let (updates_tx, updates_rx) = bounded_transport(transport_blocks, Arc::clone(&status));
    let (returned_tx, returned_rx) = bounded_transport(transport_blocks, Arc::clone(&status));
    let (retirement_tx, retirement_rx) = bounded_transport(1, Arc::clone(&status));
    let (prepared_tx, prepared_rx) = mpsc::channel();
    let initial = MidiContentSnapshot::new(
        ContentRevision(0),
        MidiSnapshotMetadata { length: 0 },
        Arc::from([]),
    );
    let (manifest, reader) = manifest_pair(initial, Arc::clone(&status));
    let mut free = Vec::with_capacity(transport_blocks);
    for _ in 0..transport_blocks {
        free.push(MidiUpdateBlock::new(block_events));
    }
    (
        MidiProcessSnapshotWriter {
            updates: updates_tx,
            returned: returned_rx,
            free,
            status: Arc::clone(&status),
            latest_revision: ContentRevision(0),
            latest_length: 0,
            block_events,
            retirement: retirement_tx,
        },
        MidiSnapshotControl {
            status,
            prepared: prepared_tx,
            chunk_events: block_events,
        },
        MidiSnapshotPublisher {
            updates: updates_rx,
            returned: returned_tx,
            prepared: prepared_rx,
            prepared_by_revision: HashMap::new(),
            manifest,
            events: Vec::new(),
            committed_events: Vec::new(),
            snapshot_chunk_events: block_events,
            retirement: retirement_rx,
            retired: false,
        },
        reader,
    )
}

impl Drop for MidiProcessSnapshotWriter {
    fn drop(&mut self) {
        let resources = std::mem::take(&mut self.free);
        if let Err(resources) = self.retirement.try_send(resources) {
            // The single producer retires only once. Never destroy pooled payloads here.
            std::mem::forget(resources);
        }
    }
}

impl MidiProcessSnapshotWriter {
    pub fn begin_mutation(&self, mutation: ContentMutation) -> bool {
        self.status.begin_mutation(mutation)
    }

    fn reclaim(&mut self) {
        while let Some(block) = self.returned.try_recv() {
            self.free.push(block);
        }
    }

    pub fn begin_working_generation(&mut self) -> bool {
        self.reclaim();
        if self.free.is_empty() || self.updates.slots() == 0 {
            self.status.mark_saturated();
            return false;
        }
        let mut block = self.free.pop().expect("capacity checked");
        block.used = 0;
        block.kind = MidiUpdateKind::BeginWorking;
        if let Err(block) = self.updates.try_send(block) {
            self.free.push(block);
            self.status.mark_saturated();
            return false;
        }
        true
    }

    fn reserve_blocks(&mut self, count: usize) -> bool {
        self.reclaim();
        if self.free.len() < count || self.updates.slots() < count {
            self.status.mark_saturated();
            return false;
        }
        true
    }

    pub fn append_storage_events(
        &mut self,
        events: &[MidiStorageElem],
        total_length: u32,
        publish: bool,
    ) -> Option<ContentRevision> {
        self.append_storage_events_with_time(events, total_length, publish, false)
    }

    pub fn append_state_events(
        &mut self,
        events: &[MidiStorageElem],
        total_length: u32,
    ) -> Option<ContentRevision> {
        self.append_storage_events_with_time(events, total_length, false, true)
    }

    fn append_storage_events_with_time(
        &mut self,
        events: &[MidiStorageElem],
        total_length: u32,
        publish: bool,
        state_events: bool,
    ) -> Option<ContentRevision> {
        let count = events.len().max(1).div_ceil(self.block_events);
        if !self.reserve_blocks(count) {
            return None;
        }
        let revision = self.status.next_revision();
        if events.is_empty() {
            self.send_control(MidiUpdateKind::Append, total_length, revision, publish)?;
        } else {
            for (index, source) in events.chunks(self.block_events).enumerate() {
                let mut block = self.free.pop().expect("capacity checked");
                for (destination, event) in block.events.iter_mut().zip(source) {
                    destination.set(
                        if state_events { -1 } else { event.time as i32 },
                        event.data(),
                    );
                }
                block.used = source.len();
                block.kind = MidiUpdateKind::Append;
                block.total_length = total_length;
                block.revision = revision;
                block.final_block = index + 1 == count;
                block.publish = publish;
                if let Err(block) = self.updates.try_send(block) {
                    self.free.push(block);
                    self.status.mark_saturated();
                    return None;
                }
            }
        }
        self.latest_revision = revision;
        self.latest_length = total_length;
        Some(revision)
    }

    pub fn append_raw_event(
        &mut self,
        time: i32,
        data: &[u8],
        total_length: u32,
        publish: bool,
    ) -> Option<ContentRevision> {
        if !self.reserve_blocks(1) {
            return None;
        }
        let revision = self.status.next_revision();
        let mut block = self.free.pop().expect("capacity checked");
        if !block.events[0].set(time, data) {
            self.free.push(block);
            return None;
        }
        block.used = 1;
        block.kind = MidiUpdateKind::Append;
        block.total_length = total_length;
        block.revision = revision;
        block.final_block = true;
        block.publish = publish;
        if let Err(block) = self.updates.try_send(block) {
            self.free.push(block);
            return None;
        }
        self.latest_revision = revision;
        self.latest_length = total_length;
        Some(revision)
    }

    pub fn clear(&mut self, publish: bool) -> Option<ContentRevision> {
        if !self.reserve_blocks(1) {
            return None;
        }
        let revision = self.status.next_revision();
        self.send_control(MidiUpdateKind::Clear, 0, revision, publish)?;
        self.latest_revision = revision;
        self.latest_length = 0;
        Some(revision)
    }

    pub fn truncate_after(
        &mut self,
        time: i32,
        total_length: u32,
        publish: bool,
    ) -> Option<ContentRevision> {
        if !self.reserve_blocks(1) {
            return None;
        }
        let revision = self.status.next_revision();
        self.send_control(
            MidiUpdateKind::TruncateAfter(time),
            total_length,
            revision,
            publish,
        )?;
        self.latest_revision = revision;
        self.latest_length = total_length;
        Some(revision)
    }

    pub fn remove_range(
        &mut self,
        start: i32,
        end: i32,
        total_length: u32,
    ) -> Option<ContentRevision> {
        if start >= end || !self.reserve_blocks(1) {
            return None;
        }
        let revision = self.status.next_revision();
        self.send_control(
            MidiUpdateKind::RemoveRange(start, end),
            total_length,
            revision,
            false,
        )?;
        self.latest_revision = revision;
        self.latest_length = total_length;
        Some(revision)
    }

    fn send_control(
        &mut self,
        kind: MidiUpdateKind,
        total_length: u32,
        revision: ContentRevision,
        publish: bool,
    ) -> Option<()> {
        let mut block = self.free.pop().expect("capacity checked");
        block.used = 0;
        block.kind = kind;
        block.total_length = total_length;
        block.revision = revision;
        block.final_block = true;
        block.publish = publish;
        if let Err(block) = self.updates.try_send(block) {
            self.free.push(block);
            return None;
        }
        Some(())
    }

    pub fn install_prepared(&mut self, prepared: PreparedMidiSnapshot) -> bool {
        if !self.reserve_blocks(1) {
            return false;
        }
        let mut block = self.free.pop().expect("capacity checked");
        block.used = 0;
        block.kind = MidiUpdateKind::Install(prepared.revision);
        block.total_length = prepared.length;
        block.revision = prepared.revision;
        block.final_block = true;
        block.publish = true;
        if let Err(block) = self.updates.try_send(block) {
            self.free.push(block);
            return false;
        }
        self.latest_revision = prepared.revision;
        self.latest_length = prepared.length;
        self.status.finish_mutation(prepared.revision);
        true
    }

    pub fn finish_mutation(&mut self, publish_final: bool) -> ContentRevision {
        if publish_final && self.reserve_blocks(1) {
            let revision = self.latest_revision;
            let _ = self.send_control(MidiUpdateKind::Publish, self.latest_length, revision, true);
        }
        self.status.finish_mutation(self.latest_revision);
        self.latest_revision
    }

    pub fn cancel_mutation(&self) {
        self.status.cancel_mutation();
    }
}

impl MidiSnapshotControl {
    pub fn begin_mutation(&self, mutation: ContentMutation) -> bool {
        self.status.begin_mutation(mutation)
    }

    pub fn cancel_mutation(&self) {
        self.status.cancel_mutation();
    }

    pub fn prepare(
        &self,
        events: &[MidiEvent],
        length: u32,
        mutation: ContentMutation,
    ) -> Option<PreparedMidiSnapshot> {
        if !self.status.begin_mutation(mutation) {
            return None;
        }
        let revision = self.status.next_revision();
        let chunks: Vec<Arc<[MidiEvent]>> = events
            .chunks(self.chunk_events)
            .map(|events| Arc::from(events))
            .collect();
        let token = PreparedMidiSnapshot { revision, length };
        if self
            .prepared
            .send(PreparedMidiManifest {
                token,
                chunks: Arc::from(chunks),
            })
            .is_err()
        {
            self.status.cancel_mutation();
            return None;
        }
        Some(token)
    }

    pub fn cancel(&self) {
        self.status.cancel_mutation();
    }
}

impl MidiSnapshotPublisher {
    pub fn is_retired(&self) -> bool {
        self.retired
    }

    pub fn pump(&mut self) -> usize {
        let retired_resources = self.retirement.try_recv();
        self.drain_prepared();
        let processed = self.pump_updates();
        if let Some(resources) = retired_resources {
            drop(resources);
            self.retired = true;
        }
        processed
    }

    fn drain_prepared(&mut self) {
        while let Ok(prepared) = self.prepared.try_recv() {
            self.prepared_by_revision
                .insert(prepared.token.revision, prepared);
        }
    }

    fn pump_updates(&mut self) -> usize {
        let mut processed = 0;
        while let Some(block) = self.updates.try_recv() {
            let mut installed = false;
            let recovers_saturation = matches!(
                block.kind,
                MidiUpdateKind::Clear | MidiUpdateKind::Install(_)
            );
            match block.kind {
                MidiUpdateKind::BeginWorking => {
                    self.events.clone_from(&self.committed_events);
                }
                MidiUpdateKind::Append => {
                    self.events
                        .extend(block.events[..block.used].iter().map(WireMidiEvent::event));
                }
                MidiUpdateKind::Clear => self.events.clear(),
                MidiUpdateKind::TruncateAfter(time) => {
                    self.events
                        .retain(|event| event.time < 0 || event.time <= time);
                }
                MidiUpdateKind::RemoveRange(start, end) => {
                    self.events
                        .retain(|event| event.time < 0 || event.time < start || event.time >= end);
                }
                MidiUpdateKind::Publish => self.events.sort_by_key(|event| event.time),
                MidiUpdateKind::Install(prepared_revision) => {
                    // Preparation and process commands use different transports. The command can
                    // arrive after this pump's initial prepared drain, so close that race here.
                    self.drain_prepared();
                    if let Some(prepared) = self.prepared_by_revision.remove(&prepared_revision) {
                        self.events = prepared
                            .chunks
                            .iter()
                            .flat_map(|chunk| chunk.iter().cloned())
                            .collect();
                        self.committed_events.clone_from(&self.events);
                        self.manifest.publish(MidiContentSnapshot::new(
                            prepared.token.revision,
                            MidiSnapshotMetadata {
                                length: prepared.token.length,
                            },
                            prepared.chunks,
                        ));
                        self.manifest.recover_saturation();
                        installed = true;
                    }
                }
            }
            let final_block = block.final_block;
            let publish = block.publish;
            let revision = block.revision;
            let length = block.total_length;
            self.returned
                .try_send(block)
                .expect("return queue capacity matches the transport pool");
            if final_block && publish && !installed {
                let chunks: Vec<Arc<[MidiEvent]>> = self
                    .events
                    .chunks(self.snapshot_chunk_events)
                    .map(|events| Arc::from(events))
                    .collect();
                self.manifest.publish(MidiContentSnapshot::new(
                    revision,
                    MidiSnapshotMetadata { length },
                    Arc::from(chunks),
                ));
                self.committed_events.clone_from(&self.events);
                if recovers_saturation {
                    self.manifest.recover_saturation();
                }
            }
            processed += 1;
        }
        processed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content_snapshot::{CurrentDataError, SnapshotCurrentness, StaleReason};

    fn event(time: u32, data: &[u8]) -> MidiStorageElem {
        MidiStorageElem::new(time, data).expect("valid event")
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn recording_publishes_ordered_complete_event_blocks() {
        let (mut writer, _control, mut publisher, reader) =
            midi_snapshot_channel(Arc::new(SessionContentEpoch::default()), 2, 3);
        assert!(writer.begin_mutation(ContentMutation::Recording));
        let revision = writer
            .append_storage_events(
                &[
                    event(1, &[0x90, 60, 100]),
                    event(1, &[0x80, 60, 0]),
                    event(4, &[0x90, 64, 100]),
                ],
                8,
                true,
            )
            .expect("event update");
        assert_eq!(publisher.pump(), 2);
        let read = reader.latest();
        assert_eq!(read.snapshot.revision, revision);
        assert_eq!(
            read.snapshot
                .events()
                .map(|event| event.time)
                .collect::<Vec<_>>(),
            vec![1, 1, 4]
        );
        assert_eq!(read.snapshot.metadata.length, 8);
        assert_eq!(
            read.currentness,
            SnapshotCurrentness::Stale(StaleReason::MutationActive(ContentMutation::Recording))
        );
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn exact_midi_requires_final_publication() {
        let (mut writer, _control, mut publisher, reader) =
            midi_snapshot_channel(Arc::new(SessionContentEpoch::default()), 2, 2);
        assert!(writer.begin_mutation(ContentMutation::Loading));
        let revision = writer
            .append_raw_event(-1, &[0xb0, 64, 0], 20, true)
            .expect("state event");
        writer.finish_mutation(false);
        assert_eq!(
            reader.try_current().unwrap_err(),
            CurrentDataError::PublicationPending {
                settled: revision,
                published: ContentRevision(0),
            }
        );
        publisher.pump();
        let snapshot = reader.try_current().expect("current snapshot");
        assert_eq!(snapshot.metadata.length, 20);
        assert_eq!(snapshot.events().next().map(|event| event.time), Some(-1));
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn prepared_generation_installs_with_explicit_duration() {
        let (mut writer, control, mut publisher, reader) =
            midi_snapshot_channel(Arc::new(SessionContentEpoch::default()), 2, 2);
        let events = [MidiEvent {
            time: 3,
            data: vec![0x90, 60, 100],
        }];
        let prepared = control
            .prepare(&events, 100, ContentMutation::Loading)
            .expect("prepare generation");
        assert!(writer.install_prepared(prepared));
        assert!(matches!(
            reader.try_current(),
            Err(CurrentDataError::PublicationPending { .. })
        ));
        publisher.pump();
        let snapshot = reader.try_current().expect("installed");
        assert_eq!(snapshot.metadata.length, 100);
        assert_eq!(snapshot.events().next().map(|event| event.time), Some(3));
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn midi_install_closes_the_cross_transport_preparation_race() {
        let (mut writer, control, mut publisher, reader) =
            midi_snapshot_channel(Arc::new(SessionContentEpoch::default()), 2, 2);
        publisher.drain_prepared();
        let token = control
            .prepare(
                &[MidiEvent::new(1, vec![0x90, 60, 100])],
                8,
                ContentMutation::Loading,
            )
            .expect("prepare after the initial drain");
        assert!(writer.install_prepared(token));
        assert_eq!(publisher.pump_updates(), 1);
        assert_eq!(reader.try_current().expect("installed").events().count(), 1);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn cancelled_midi_work_is_reset_before_the_next_generation() {
        let (mut writer, _control, mut publisher, reader) =
            midi_snapshot_channel(Arc::new(SessionContentEpoch::default()), 2, 6);
        assert!(writer.begin_mutation(ContentMutation::Loading));
        writer.append_storage_events(&[event(1, &[0x90, 60, 100])], 8, true);
        writer.finish_mutation(false);
        publisher.pump();

        assert!(writer.begin_mutation(ContentMutation::Replacing));
        assert!(writer.begin_working_generation());
        writer.append_storage_events(&[event(2, &[0x90, 61, 100])], 8, false);
        publisher.pump();
        writer.cancel_mutation();

        assert!(writer.begin_mutation(ContentMutation::Recording));
        assert!(writer.begin_working_generation());
        writer.append_storage_events(&[event(3, &[0x90, 62, 100])], 8, true);
        writer.finish_mutation(false);
        publisher.pump();
        assert_eq!(
            reader
                .latest()
                .snapshot
                .events()
                .map(|event| event.time)
                .collect::<Vec<_>>(),
            vec![1, 3]
        );
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn payload_boundaries_partial_publication_and_clear_are_deterministic() {
        let (mut writer, _control, mut publisher, reader) =
            midi_snapshot_channel(Arc::new(SessionContentEpoch::default()), 2, 6);
        assert!(writer.begin_mutation(ContentMutation::Recording));
        writer
            .append_raw_event(1, &[0xf0, 1, 2, 3], 8, false)
            .expect("maximum payload");
        publisher.pump();
        assert!(reader.latest().snapshot.events().next().is_none());
        assert!(writer
            .append_raw_event(2, &[0xf0, 1, 2, 3, 4], 8, true)
            .is_none());
        let published = writer
            .append_raw_event(3, &[0x90, 60, 100], 8, true)
            .expect("valid payload after rejection");
        writer.finish_mutation(false);
        publisher.pump();
        let retained = reader.latest().snapshot;
        assert_eq!(retained.revision, published);
        assert_eq!(
            retained
                .events()
                .map(|event| event.time)
                .collect::<Vec<_>>(),
            vec![1, 3]
        );

        assert!(writer.begin_mutation(ContentMutation::Clearing));
        assert!(writer.begin_working_generation());
        let cleared = writer.clear(true).expect("clear");
        writer.finish_mutation(false);
        publisher.pump();
        assert!(cleared > published);
        assert_eq!(retained.events().count(), 2);
        let current = reader.try_current().expect("clear settled");
        assert_eq!(current.metadata.length, 0);
        assert_eq!(current.events().count(), 0);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn midi_saturation_is_sticky_until_a_full_clear_recovers() {
        let (mut writer, _control, mut publisher, reader) =
            midi_snapshot_channel(Arc::new(SessionContentEpoch::default()), 1, 1);
        assert!(writer.begin_mutation(ContentMutation::Recording));
        writer
            .append_storage_events(&[event(1, &[0x90, 60, 100])], 4, true)
            .expect("first block");
        assert!(writer
            .append_storage_events(&[event(2, &[0x80, 60, 0])], 4, true)
            .is_none());
        publisher.pump();
        writer.finish_mutation(false);
        assert!(matches!(
            reader.latest().currentness,
            SnapshotCurrentness::Stale(StaleReason::PublicationSaturated)
        ));

        assert!(writer.begin_mutation(ContentMutation::Clearing));
        writer.clear(true).expect("full clear recovery");
        writer.finish_mutation(false);
        publisher.pump();
        assert!(reader
            .try_current()
            .expect("recovered")
            .events()
            .next()
            .is_none());
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn range_replacement_is_sorted_and_hidden_until_committed() {
        let (mut writer, _control, mut publisher, reader) =
            midi_snapshot_channel(Arc::new(SessionContentEpoch::default()), 2, 5);
        assert!(writer.begin_mutation(ContentMutation::Loading));
        writer.append_storage_events(
            &[
                event(1, &[0x90, 60, 100]),
                event(4, &[0x80, 60, 0]),
                event(8, &[0x90, 61, 100]),
            ],
            10,
            true,
        );
        writer.finish_mutation(false);
        publisher.pump();

        assert!(writer.begin_mutation(ContentMutation::Replacing));
        assert!(writer.begin_working_generation());
        writer.remove_range(3, 6, 10);
        writer.append_storage_events(&[event(3, &[0x90, 64, 100])], 10, false);
        publisher.pump();
        assert_eq!(
            reader
                .latest()
                .snapshot
                .events()
                .map(|event| event.time)
                .collect::<Vec<_>>(),
            vec![1, 4, 8]
        );

        writer.finish_mutation(true);
        publisher.pump();
        assert_eq!(
            reader
                .latest()
                .snapshot
                .events()
                .map(|event| event.time)
                .collect::<Vec<_>>(),
            vec![1, 3, 8]
        );
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn hidden_replace_is_published_only_when_committed() {
        let (mut writer, _control, mut publisher, reader) =
            midi_snapshot_channel(Arc::new(SessionContentEpoch::default()), 2, 3);
        assert!(writer.begin_mutation(ContentMutation::Loading));
        writer.append_storage_events(
            &[event(1, &[0x90, 60, 100]), event(8, &[0x80, 60, 0])],
            10,
            true,
        );
        writer.finish_mutation(false);
        publisher.pump();

        assert!(writer.begin_mutation(ContentMutation::Replacing));
        writer.truncate_after(2, 10, false);
        writer.append_storage_events(&[event(3, &[0x90, 64, 100])], 10, false);
        publisher.pump();
        assert_eq!(reader.latest().snapshot.events().count(), 2);
        writer.finish_mutation(true);
        publisher.pump();
        assert_eq!(
            reader
                .latest()
                .snapshot
                .events()
                .map(|event| event.time)
                .collect::<Vec<_>>(),
            vec![1, 3]
        );
    }
}
