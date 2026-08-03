use super::manifest::{manifest_pair, ManifestPublisher, ManifestReader};
use super::transport::{bounded_transport, ProcessSender, PublisherReceiver};
use super::{
    ContentMutation, ContentRevision, ContentStatus, MidiContentSnapshot, MidiSnapshotMetadata,
    SessionContentEpoch,
};
use crate::midi_event::MidiEvent;
use crate::midi_storage::{MidiStorageElem, MAX_MSG_BYTES};
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
    Append,
    Clear,
    TruncateAfter(i32),
    Publish,
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

pub struct MidiProcessSnapshotWriter {
    updates: ProcessSender<MidiUpdateBlock>,
    returned: PublisherReceiver<MidiUpdateBlock>,
    free: Vec<MidiUpdateBlock>,
    status: Arc<ContentStatus>,
    latest_revision: ContentRevision,
    latest_length: u32,
    block_events: usize,
}

pub struct MidiSnapshotPublisher {
    updates: PublisherReceiver<MidiUpdateBlock>,
    returned: ProcessSender<MidiUpdateBlock>,
    manifest: ManifestPublisher<MidiContentSnapshot>,
    events: Vec<MidiEvent>,
    snapshot_chunk_events: usize,
}

pub type MidiSnapshotReader = ManifestReader<MidiContentSnapshot>;

pub fn midi_snapshot_channel(
    epoch: Arc<SessionContentEpoch>,
    block_events: usize,
    transport_blocks: usize,
) -> (
    MidiProcessSnapshotWriter,
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
            status,
            latest_revision: ContentRevision(0),
            latest_length: 0,
            block_events,
        },
        MidiSnapshotPublisher {
            updates: updates_rx,
            returned: returned_tx,
            manifest,
            events: Vec::new(),
            snapshot_chunk_events: block_events,
        },
        reader,
    )
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
                    destination.set(event.time as i32, event.data());
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

impl MidiSnapshotPublisher {
    pub fn pump(&mut self) -> usize {
        let mut processed = 0;
        while let Some(block) = self.updates.try_recv() {
            match block.kind {
                MidiUpdateKind::Append => {
                    self.events
                        .extend(block.events[..block.used].iter().map(WireMidiEvent::event));
                }
                MidiUpdateKind::Clear => self.events.clear(),
                MidiUpdateKind::TruncateAfter(time) => {
                    self.events
                        .retain(|event| event.time < 0 || event.time <= time);
                }
                MidiUpdateKind::Publish => {}
            }
            let final_block = block.final_block;
            let publish = block.publish;
            let revision = block.revision;
            let length = block.total_length;
            self.returned
                .try_send(block)
                .expect("return queue capacity matches the transport pool");
            if final_block && publish {
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

    #[test]
    fn recording_publishes_ordered_complete_event_blocks() {
        let (mut writer, mut publisher, reader) =
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

    #[test]
    fn exact_midi_requires_final_publication() {
        let (mut writer, mut publisher, reader) =
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

    #[test]
    fn hidden_replace_is_published_only_when_committed() {
        let (mut writer, mut publisher, reader) =
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
