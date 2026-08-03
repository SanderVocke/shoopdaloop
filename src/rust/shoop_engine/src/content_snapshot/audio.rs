use super::manifest::{manifest_pair, ManifestPublisher, ManifestReader};
use super::transport::{bounded_transport, ProcessSender, PublisherReceiver};
use super::{
    AudioContentSnapshot, AudioSnapshotMetadata, ContentMutation, ContentRevision, ContentStatus,
    SessionContentEpoch,
};
use std::sync::Arc;

#[derive(Debug)]
struct AudioUpdateBlock {
    samples: Box<[f32]>,
    offset: usize,
    used: usize,
    total_length: usize,
    revision: ContentRevision,
    final_block: bool,
    publish: bool,
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
        }
    }
}

pub struct AudioProcessSnapshotWriter {
    updates: ProcessSender<AudioUpdateBlock>,
    returned: PublisherReceiver<AudioUpdateBlock>,
    free: Vec<AudioUpdateBlock>,
    status: Arc<ContentStatus>,
    latest_revision: ContentRevision,
    latest_length: usize,
    block_size: usize,
}

pub struct AudioSnapshotPublisher {
    updates: PublisherReceiver<AudioUpdateBlock>,
    returned: ProcessSender<AudioUpdateBlock>,
    manifest: ManifestPublisher<AudioContentSnapshot>,
    chunks: Vec<Arc<[f32]>>,
    chunk_size: usize,
}

pub type AudioSnapshotReader = ManifestReader<AudioContentSnapshot>;

pub fn audio_snapshot_channel(
    epoch: Arc<SessionContentEpoch>,
    chunk_size: usize,
    transport_blocks: usize,
) -> (
    AudioProcessSnapshotWriter,
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
            status,
            latest_revision: ContentRevision(0),
            latest_length: 0,
            block_size: chunk_size,
        },
        AudioSnapshotPublisher {
            updates: updates_rx,
            returned: returned_tx,
            manifest,
            chunks: Vec::new(),
            chunk_size,
        },
        reader,
    )
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

    pub fn publish_range(
        &mut self,
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

impl AudioSnapshotPublisher {
    pub fn pump(&mut self) -> usize {
        let mut processed = 0;
        while let Some(block) = self.updates.try_recv() {
            self.apply(&block);
            let final_block = block.final_block;
            let publish = block.publish;
            let revision = block.revision;
            let length = block.total_length;
            self.returned
                .try_send(block)
                .expect("return queue capacity matches the transport pool");
            if final_block && publish {
                self.manifest.publish(AudioContentSnapshot::new(
                    revision,
                    AudioSnapshotMetadata { length },
                    Arc::from(self.chunks.clone()),
                ));
            }
            processed += 1;
        }
        processed
    }

    fn apply(&mut self, block: &AudioUpdateBlock) {
        if block.used == 0 {
            self.truncate(block.total_length);
            return;
        }
        let end = block.offset + block.used;
        let needed = end.div_ceil(self.chunk_size);
        while self.chunks.len() < needed {
            self.chunks
                .push(Arc::from(vec![0.0; self.chunk_size].into_boxed_slice()));
        }
        let mut source_offset = 0;
        let mut destination_offset = block.offset;
        while source_offset < block.used {
            let chunk_index = destination_offset / self.chunk_size;
            let chunk_offset = destination_offset % self.chunk_size;
            let count = (self.chunk_size - chunk_offset).min(block.used - source_offset);
            let mut updated = self.chunks[chunk_index].to_vec();
            updated[chunk_offset..chunk_offset + count]
                .copy_from_slice(&block.samples[source_offset..source_offset + count]);
            self.chunks[chunk_index] = Arc::from(updated.into_boxed_slice());
            source_offset += count;
            destination_offset += count;
        }
        self.truncate(block.total_length);
    }

    fn truncate(&mut self, length: usize) {
        self.chunks.truncate(length.div_ceil(self.chunk_size));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content_snapshot::{CurrentDataError, SnapshotCurrentness, StaleReason};

    #[test]
    fn recording_publishes_only_complete_process_updates() {
        let (mut writer, mut publisher, reader) =
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
        let (mut writer, mut publisher, reader) =
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
        let (mut writer, mut publisher, reader) =
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
    fn saturation_keeps_the_last_complete_snapshot() {
        let (mut writer, _publisher, reader) =
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
