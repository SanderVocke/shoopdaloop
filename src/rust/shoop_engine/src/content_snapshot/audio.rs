use super::manifest::{manifest_pair, ManifestPublisher, ManifestReader};
use super::transport::{bounded_transport, ProcessSender, PublisherReceiver};
use super::{
    AudioContentSnapshot, AudioSnapshotMetadata, ContentMutation, ContentRevision, ContentStatus,
    SessionContentEpoch,
};
use std::collections::HashMap;
use std::fmt;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

#[derive(Debug, Clone, Copy)]
enum AudioUpdateKind {
    BeginWorking,
    Range,
    PreRecordRange,
    ClearPreRecord,
    AdoptPreRecord,
    Silence,
    Install(ContentRevision),
}

#[derive(Debug)]
struct AudioUpdateBlock {
    samples: Box<[f32]>,
    offset: usize,
    used: usize,
    total_length: usize,
    revision: ContentRevision,
    final_block: bool,
    publish: bool,
    kind: AudioUpdateKind,
}

impl AudioUpdateBlock {
    fn new(chunk_size: usize) -> Self {
        Self {
            samples: vec![0.0; chunk_size].into_boxed_slice(),
            offset: 0,
            used: 0,
            total_length: 0,
            revision: ContentRevision(0),
            final_block: false,
            publish: false,
            kind: AudioUpdateKind::Range,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreparedAudioSnapshot {
    revision: ContentRevision,
    length: usize,
}

impl PreparedAudioSnapshot {
    pub fn revision(self) -> ContentRevision {
        self.revision
    }
}

struct PreparedAudioManifest {
    token: PreparedAudioSnapshot,
    chunks: Arc<[Arc<[f32]>]>,
}

#[derive(Clone)]
pub struct AudioSnapshotControl {
    status: Arc<ContentStatus>,
    prepared: Sender<PreparedAudioManifest>,
    chunk_size: usize,
}

pub struct AudioProcessSnapshotWriter {
    updates: ProcessSender<AudioUpdateBlock>,
    returned: PublisherReceiver<AudioUpdateBlock>,
    free: Vec<AudioUpdateBlock>,
    status: Arc<ContentStatus>,
    latest_revision: ContentRevision,
    latest_length: usize,
    block_size: usize,
    retirement: ProcessSender<Vec<AudioUpdateBlock>>,
}

impl fmt::Debug for AudioProcessSnapshotWriter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AudioProcessSnapshotWriter")
            .field("latest_revision", &self.latest_revision)
            .field("latest_length", &self.latest_length)
            .finish_non_exhaustive()
    }
}

pub struct AudioSnapshotPublisher {
    updates: PublisherReceiver<AudioUpdateBlock>,
    returned: ProcessSender<AudioUpdateBlock>,
    prepared: Receiver<PreparedAudioManifest>,
    prepared_by_revision: HashMap<ContentRevision, PreparedAudioManifest>,
    manifest: ManifestPublisher<AudioContentSnapshot>,
    chunks: Vec<Arc<[f32]>>,
    committed_chunks: Vec<Arc<[f32]>>,
    prerecord_chunks: Vec<Arc<[f32]>>,
    chunk_size: usize,
    retirement: PublisherReceiver<Vec<AudioUpdateBlock>>,
    retired: bool,
}

pub type AudioSnapshotReader = ManifestReader<AudioContentSnapshot>;

pub fn audio_snapshot_channel(
    epoch: Arc<SessionContentEpoch>,
    chunk_size: usize,
    transport_blocks: usize,
) -> (
    AudioProcessSnapshotWriter,
    AudioSnapshotControl,
    AudioSnapshotPublisher,
    AudioSnapshotReader,
) {
    assert!(chunk_size > 0, "audio snapshot chunk size must be non-zero");
    assert!(
        transport_blocks > 0,
        "audio snapshot transport must have blocks"
    );
    let status = Arc::new(ContentStatus::new(epoch));
    let (updates_tx, updates_rx) = bounded_transport(transport_blocks, Arc::clone(&status));
    let (returned_tx, returned_rx) = bounded_transport(transport_blocks, Arc::clone(&status));
    let (retirement_tx, retirement_rx) = bounded_transport(1, Arc::clone(&status));
    let (prepared_tx, prepared_rx) = mpsc::channel();
    let initial = AudioContentSnapshot::new(
        ContentRevision(0),
        AudioSnapshotMetadata { length: 0 },
        Arc::from([]),
    );
    let (manifest, reader) = manifest_pair(initial, Arc::clone(&status));
    let mut free = Vec::with_capacity(transport_blocks);
    for _ in 0..transport_blocks {
        free.push(AudioUpdateBlock::new(chunk_size));
    }
    (
        AudioProcessSnapshotWriter {
            updates: updates_tx,
            returned: returned_rx,
            free,
            status: Arc::clone(&status),
            latest_revision: ContentRevision(0),
            latest_length: 0,
            block_size: chunk_size,
            retirement: retirement_tx,
        },
        AudioSnapshotControl {
            status,
            prepared: prepared_tx,
            chunk_size,
        },
        AudioSnapshotPublisher {
            updates: updates_rx,
            returned: returned_tx,
            prepared: prepared_rx,
            prepared_by_revision: HashMap::new(),
            manifest,
            chunks: Vec::new(),
            committed_chunks: Vec::new(),
            prerecord_chunks: Vec::new(),
            chunk_size,
            retirement: retirement_rx,
            retired: false,
        },
        reader,
    )
}

impl Drop for AudioProcessSnapshotWriter {
    fn drop(&mut self) {
        let resources = std::mem::take(&mut self.free);
        if let Err(resources) = self.retirement.try_send(resources) {
            // The single producer retires only once, so this is unreachable unless the
            // publisher has already gone away. Leaking is preferable to realtime destruction.
            std::mem::forget(resources);
        }
    }
}

impl AudioProcessSnapshotWriter {
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
        block.kind = AudioUpdateKind::BeginWorking;
        if let Err(block) = self.updates.try_send(block) {
            self.free.push(block);
            self.status.mark_saturated();
            return false;
        }
        true
    }

    pub fn publish_range(
        &mut self,
        offset: usize,
        samples: &[f32],
        total_length: usize,
        publish: bool,
    ) -> Option<ContentRevision> {
        self.publish_range_kind(
            AudioUpdateKind::Range,
            offset,
            samples,
            total_length,
            publish,
        )
    }

    pub fn publish_prerecord_range(
        &mut self,
        offset: usize,
        samples: &[f32],
        total_length: usize,
    ) -> Option<ContentRevision> {
        self.publish_range_kind(
            AudioUpdateKind::PreRecordRange,
            offset,
            samples,
            total_length,
            false,
        )
    }

    fn publish_range_kind(
        &mut self,
        kind: AudioUpdateKind,
        offset: usize,
        samples: &[f32],
        total_length: usize,
        publish: bool,
    ) -> Option<ContentRevision> {
        self.reclaim();
        let n_blocks = samples.len().max(1).div_ceil(self.block_size);
        if self.free.len() < n_blocks || self.updates.slots() < n_blocks {
            self.status.mark_saturated();
            return None;
        }
        let revision = self.status.next_revision();
        let block_size = self.block_size;
        if samples.is_empty() {
            let mut block = self.free.pop().expect("capacity checked");
            block.offset = offset;
            block.used = 0;
            block.total_length = total_length;
            block.revision = revision;
            block.final_block = true;
            block.publish = publish;
            block.kind = kind;
            if let Err(block) = self.updates.try_send(block) {
                self.free.push(block);
                return None;
            }
        } else {
            for (index, source) in samples.chunks(block_size).enumerate() {
                let mut block = self.free.pop().expect("capacity checked");
                block.samples[..source.len()].copy_from_slice(source);
                block.offset = offset + index * block_size;
                block.used = source.len();
                block.total_length = total_length;
                block.revision = revision;
                block.final_block = index + 1 == n_blocks;
                block.publish = publish;
                block.kind = kind;
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

    pub fn clear_prerecord(&mut self) -> Option<ContentRevision> {
        self.publish_range_kind(AudioUpdateKind::ClearPreRecord, 0, &[], 0, false)
    }

    pub fn adopt_prerecord(&mut self, length: usize) -> Option<ContentRevision> {
        self.publish_range_kind(AudioUpdateKind::AdoptPreRecord, 0, &[], length, true)
    }

    pub fn publish_silence(&mut self, length: usize) -> Option<ContentRevision> {
        self.reclaim();
        if self.free.is_empty() || self.updates.slots() == 0 {
            self.status.mark_saturated();
            return None;
        }
        let revision = self.status.next_revision();
        let mut block = self.free.pop().expect("capacity checked");
        block.used = 0;
        block.total_length = length;
        block.revision = revision;
        block.final_block = true;
        block.publish = true;
        block.kind = AudioUpdateKind::Silence;
        if let Err(block) = self.updates.try_send(block) {
            self.free.push(block);
            return None;
        }
        self.latest_revision = revision;
        self.latest_length = length;
        Some(revision)
    }

    pub fn install_prepared(&mut self, prepared: PreparedAudioSnapshot) -> bool {
        self.reclaim();
        if self.free.is_empty() || self.updates.slots() == 0 {
            self.status.mark_saturated();
            return false;
        }
        let mut block = self.free.pop().expect("capacity checked");
        block.used = 0;
        block.total_length = prepared.length;
        block.revision = prepared.revision;
        block.final_block = true;
        block.publish = true;
        block.kind = AudioUpdateKind::Install(prepared.revision);
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
        if publish_final {
            let _ = self.publish_range(self.latest_length, &[], self.latest_length, true);
        }
        self.status.finish_mutation(self.latest_revision);
        self.latest_revision
    }

    pub fn cancel_mutation(&self) {
        self.status.cancel_mutation();
    }

    pub fn status(&self) -> &Arc<ContentStatus> {
        &self.status
    }
}

impl AudioSnapshotControl {
    pub fn prepare(
        &self,
        samples: &[f32],
        mutation: ContentMutation,
    ) -> Option<PreparedAudioSnapshot> {
        if !self.status.begin_mutation(mutation) {
            return None;
        }
        let revision = self.status.next_revision();
        let chunks: Vec<Arc<[f32]>> = samples
            .chunks(self.chunk_size)
            .map(|chunk| {
                let mut padded = vec![0.0; self.chunk_size];
                padded[..chunk.len()].copy_from_slice(chunk);
                Arc::from(padded.into_boxed_slice())
            })
            .collect();
        let token = PreparedAudioSnapshot {
            revision,
            length: samples.len(),
        };
        if self
            .prepared
            .send(PreparedAudioManifest {
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

impl AudioSnapshotPublisher {
    pub fn is_retired(&self) -> bool {
        self.retired
    }

    pub fn pump(&mut self) -> usize {
        let retired_resources = self.retirement.try_recv();
        while let Ok(prepared) = self.prepared.try_recv() {
            self.prepared_by_revision
                .insert(prepared.token.revision, prepared);
        }
        let mut processed = 0;
        while let Some(block) = self.updates.try_recv() {
            let final_block = block.final_block;
            let publish = block.publish;
            let revision = block.revision;
            let length = block.total_length;
            match block.kind {
                AudioUpdateKind::BeginWorking => {
                    self.chunks = self.committed_chunks.clone();
                }
                AudioUpdateKind::Range => {
                    Self::apply_to(&mut self.chunks, self.chunk_size, &block);
                    if final_block && publish {
                        self.manifest.publish(AudioContentSnapshot::new(
                            revision,
                            AudioSnapshotMetadata { length },
                            Arc::from(self.chunks.clone()),
                        ));
                        self.committed_chunks = self.chunks.clone();
                    }
                }
                AudioUpdateKind::PreRecordRange => {
                    Self::apply_to(&mut self.prerecord_chunks, self.chunk_size, &block);
                }
                AudioUpdateKind::ClearPreRecord => self.prerecord_chunks.clear(),
                AudioUpdateKind::AdoptPreRecord => {
                    self.chunks = std::mem::take(&mut self.prerecord_chunks);
                    Self::resize_to(&mut self.chunks, self.chunk_size, length);
                    self.manifest.publish(AudioContentSnapshot::new(
                        revision,
                        AudioSnapshotMetadata { length },
                        Arc::from(self.chunks.clone()),
                    ));
                    self.committed_chunks = self.chunks.clone();
                }
                AudioUpdateKind::Silence => {
                    self.chunks = (0..length.div_ceil(self.chunk_size))
                        .map(|_| Arc::from(vec![0.0; self.chunk_size].into_boxed_slice()))
                        .collect();
                    self.manifest.publish(AudioContentSnapshot::new(
                        revision,
                        AudioSnapshotMetadata { length },
                        Arc::from(self.chunks.clone()),
                    ));
                    self.committed_chunks = self.chunks.clone();
                }
                AudioUpdateKind::Install(prepared_revision) => {
                    if let Some(prepared) = self.prepared_by_revision.remove(&prepared_revision) {
                        self.chunks = prepared.chunks.to_vec();
                        self.manifest.publish(AudioContentSnapshot::new(
                            prepared.token.revision,
                            AudioSnapshotMetadata {
                                length: prepared.token.length,
                            },
                            prepared.chunks,
                        ));
                        self.committed_chunks = self.chunks.clone();
                    }
                }
            }
            self.returned
                .try_send(block)
                .expect("return queue capacity matches the transport pool");
            processed += 1;
        }
        if let Some(resources) = retired_resources {
            drop(resources);
            self.retired = true;
        }
        processed
    }

    fn apply_to(chunks: &mut Vec<Arc<[f32]>>, chunk_size: usize, block: &AudioUpdateBlock) {
        if block.used == 0 {
            Self::resize_to(chunks, chunk_size, block.total_length);
            return;
        }
        let end = block.offset + block.used;
        let needed = end.div_ceil(chunk_size);
        while chunks.len() < needed {
            chunks.push(Arc::from(vec![0.0; chunk_size].into_boxed_slice()));
        }
        let mut source_offset = 0;
        let mut destination_offset = block.offset;
        while source_offset < block.used {
            let chunk_index = destination_offset / chunk_size;
            let chunk_offset = destination_offset % chunk_size;
            let count = (chunk_size - chunk_offset).min(block.used - source_offset);
            let mut updated = chunks[chunk_index].to_vec();
            updated[chunk_offset..chunk_offset + count]
                .copy_from_slice(&block.samples[source_offset..source_offset + count]);
            chunks[chunk_index] = Arc::from(updated.into_boxed_slice());
            source_offset += count;
            destination_offset += count;
        }
        Self::resize_to(chunks, chunk_size, block.total_length);
    }

    fn resize_to(chunks: &mut Vec<Arc<[f32]>>, chunk_size: usize, length: usize) {
        let needed = length.div_ceil(chunk_size);
        chunks.truncate(needed);
        while chunks.len() < needed {
            chunks.push(Arc::from(vec![0.0; chunk_size].into_boxed_slice()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content_snapshot::{CurrentDataError, SnapshotCurrentness, StaleReason};

    #[test]
    fn recording_publishes_only_complete_process_updates() {
        let (mut writer, _control, mut publisher, reader) =
            audio_snapshot_channel(Arc::new(SessionContentEpoch::default()), 4, 4);
        assert!(writer.begin_mutation(ContentMutation::Recording));
        let first = writer
            .publish_range(0, &[1.0, 2.0, 3.0], 3, true)
            .expect("first update");
        assert_eq!(reader.latest().snapshot.revision, ContentRevision(0));
        assert_eq!(publisher.pump(), 1);
        let read = reader.latest();
        assert_eq!(read.snapshot.revision, first);
        assert_eq!(read.snapshot.contiguous(), vec![1.0, 2.0, 3.0]);
        assert_eq!(
            read.currentness,
            SnapshotCurrentness::Stale(StaleReason::MutationActive(ContentMutation::Recording))
        );

        let second = writer
            .publish_range(3, &[4.0, 5.0], 5, true)
            .expect("second update");
        assert_eq!(publisher.pump(), 1);
        assert_eq!(reader.latest().snapshot.revision, second);
        assert_eq!(
            reader.latest().snapshot.contiguous(),
            vec![1.0, 2.0, 3.0, 4.0, 5.0]
        );
    }

    #[test]
    fn exact_read_waits_for_final_publication_after_recording() {
        let (mut writer, _control, mut publisher, reader) =
            audio_snapshot_channel(Arc::new(SessionContentEpoch::default()), 4, 4);
        assert!(writer.begin_mutation(ContentMutation::Recording));
        let revision = writer
            .publish_range(0, &[1.0, 2.0], 2, true)
            .expect("record update");
        writer.finish_mutation(false);
        assert_eq!(
            reader.try_current().unwrap_err(),
            CurrentDataError::PublicationPending {
                settled: revision,
                published: ContentRevision(0),
            }
        );
        publisher.pump();
        assert_eq!(
            reader.try_current().expect("settled").contiguous(),
            vec![1.0, 2.0]
        );
    }

    #[test]
    fn hidden_working_generation_replaces_the_visible_snapshot_at_commit() {
        let (mut writer, _control, mut publisher, reader) =
            audio_snapshot_channel(Arc::new(SessionContentEpoch::default()), 4, 4);
        assert!(writer.begin_mutation(ContentMutation::Loading));
        writer.publish_range(0, &[1.0, 2.0, 3.0, 4.0], 4, true);
        writer.finish_mutation(false);
        publisher.pump();
        assert_eq!(
            reader.latest().snapshot.contiguous(),
            vec![1.0, 2.0, 3.0, 4.0]
        );

        assert!(writer.begin_mutation(ContentMutation::Replacing));
        writer.publish_range(1, &[9.0, 8.0], 4, false);
        publisher.pump();
        assert_eq!(
            reader.latest().snapshot.contiguous(),
            vec![1.0, 2.0, 3.0, 4.0]
        );
        writer.finish_mutation(true);
        publisher.pump();
        assert_eq!(
            reader.latest().snapshot.contiguous(),
            vec![1.0, 9.0, 8.0, 4.0]
        );
    }

    #[test]
    fn cancelled_private_work_is_reset_from_the_committed_generation() {
        let (mut writer, _control, mut publisher, reader) =
            audio_snapshot_channel(Arc::new(SessionContentEpoch::default()), 4, 6);
        assert!(writer.begin_mutation(ContentMutation::Loading));
        writer.publish_range(0, &[1.0, 2.0, 3.0, 4.0], 4, true);
        writer.finish_mutation(false);
        publisher.pump();

        assert!(writer.begin_mutation(ContentMutation::Replacing));
        assert!(writer.begin_working_generation());
        writer.publish_range(0, &[9.0], 4, false);
        publisher.pump();
        writer.cancel_mutation();

        assert!(writer.begin_mutation(ContentMutation::Recording));
        assert!(writer.begin_working_generation());
        writer.publish_range(3, &[5.0], 4, true);
        writer.finish_mutation(false);
        publisher.pump();
        assert_eq!(
            reader.latest().snapshot.contiguous(),
            vec![1.0, 2.0, 3.0, 5.0]
        );
    }

    #[test]
    fn prepared_generation_installs_without_process_side_content_copying() {
        let (mut writer, control, mut publisher, reader) =
            audio_snapshot_channel(Arc::new(SessionContentEpoch::default()), 2, 2);
        let prepared = control
            .prepare(&[1.0, 2.0, 3.0], ContentMutation::Loading)
            .expect("prepare generation");
        assert!(matches!(
            reader.try_current(),
            Err(CurrentDataError::MutationActive(ContentMutation::Loading))
        ));
        assert!(writer.install_prepared(prepared));
        assert!(matches!(
            reader.try_current(),
            Err(CurrentDataError::PublicationPending { .. })
        ));
        publisher.pump();
        assert_eq!(
            reader.try_current().expect("installed").contiguous(),
            vec![1.0, 2.0, 3.0]
        );
    }

    #[test]
    fn saturation_keeps_the_last_complete_snapshot() {
        let (mut writer, _control, _publisher, reader) =
            audio_snapshot_channel(Arc::new(SessionContentEpoch::default()), 2, 1);
        assert!(writer.begin_mutation(ContentMutation::Recording));
        assert!(writer.publish_range(0, &[1.0, 2.0], 2, true).is_some());
        assert!(writer.publish_range(2, &[3.0], 3, true).is_none());
        assert!(matches!(
            reader.latest().currentness,
            SnapshotCurrentness::Stale(StaleReason::PublicationSaturated)
        ));
        assert!(reader.latest().snapshot.contiguous().is_empty());
    }
}
