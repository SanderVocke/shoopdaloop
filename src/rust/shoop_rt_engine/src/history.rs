use crate::{runtime_capacities, EngineDescription};
use rt_runtime::{
    resource::{indexed_midi, shared_audio, shared_midi},
    Error, Quantum,
};
use shoop_loops::runtime::{HistorySettings, SharedSettings};

/// Shared storage only; ordinary media/index, graph and carrier metadata are
/// additional. Payload includes each independent reservation exactly once.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HistoryCapacity {
    pub retention_frames: u64,
    pub ordinary_payload_bytes: usize,
    pub history_audio_bytes: usize,
    pub shared_audio_bytes: usize,
    pub midi_payload_bytes: usize,
    pub audio_metadata_bytes: usize,
    pub midi_metadata_bytes: usize,
    pub total_resource_pages: usize,
    pub writer_slots: usize,
    pub midi_events_per_source: usize,
    pub midi_bytes_per_source: usize,
}
pub(super) struct Layout {
    pub capacity: HistoryCapacity,
    audio: Option<shared_audio::Config>,
    midi: Option<indexed_midi::Config>,
    sources: usize,
}
fn add(a: usize, b: usize) -> Result<usize, Error> {
    a.checked_add(b).ok_or(Error::Capacity)
}
fn mul(a: usize, b: usize) -> Result<usize, Error> {
    a.checked_mul(b).ok_or(Error::Capacity)
}
impl EngineDescription {
    /// Validate and size production input history without allocating any pool.
    pub fn history_capacity(&self) -> Result<HistoryCapacity, Error> {
        Layout::new(self).map(|layout| layout.capacity)
    }
}
impl Layout {
    pub fn new(d: &EngineDescription) -> Result<Self, Error> {
        d.validate()?;
        let ordinary_payload_bytes = mul(d.resource_pages, d.page_bytes)?;
        let mut result = Self {
            capacity: HistoryCapacity {
                ordinary_payload_bytes,
                total_resource_pages: d.resource_pages,
                ..HistoryCapacity::default()
            },
            audio: None,
            midi: None,
            sources: d.strips(),
        };
        if d.history.seconds == 0 || d.strips() == 0 {
            return Ok(result);
        }
        let q = Quantum::new(d.quantum)?;
        let frames = q
            .ceil(rt_runtime::Frame(
                u64::from(d.sample_rate)
                    .checked_mul(u64::from(d.history.seconds))
                    .ok_or(Error::Capacity)?,
            ))?
            .0;
        let frame_bytes = mul(d.strip_channels as usize, 4)?;
        let page_frames = d.page_bytes / frame_bytes;
        if page_frames == 0
            || d.history.audio_take_pages == 0
            || d.history.midi_pages == 0
            || d.history.midi_records == 0
            || d.history.writers == 0
        {
            return Err(Error::InvalidConfig);
        }
        let ring_pages = usize::try_from(frames)
            .map_err(|_| Error::Capacity)?
            .div_ceil(page_frames)
            .checked_add(1)
            .and_then(usize::checked_next_power_of_two)
            .ok_or(Error::Capacity)?;
        let transaction_pages = d
            .quantum
            .div_ceil(page_frames)
            .min(ring_pages)
            .checked_add(4)
            .ok_or(Error::Capacity)?;
        let transaction_pages = transaction_pages.max(mul(
            d.strip_channels as usize,
            add(d.quantum.div_ceil(page_frames), 3)?,
        )?);
        let history_pages = mul(ring_pages, d.strips())?;
        let audio_pages = add(
            add(history_pages, transaction_pages)?,
            d.history.audio_take_pages as usize,
        )?;
        let total_pages = add(
            add(d.resource_pages, audio_pages)?,
            d.history.midi_pages as usize,
        )?;
        Some(mul(total_pages, d.page_bytes)?)
            .filter(|&bytes| bytes <= i32::MAX as usize)
            .ok_or(Error::Capacity)?;
        let mut limits = runtime_capacities(d, q.frames());
        limits.resource_pages = total_pages;
        let runtime = limits.prepare()?;
        let writers = (d.history.writers as usize).min(d.loops());
        let views = add(
            add(mul(d.loops(), 4)?, mul(writers, 4)?)?,
            add(d.bulk_credits as usize, 16)?,
        )?;
        let audio = shared_audio::Config {
            channels: u32::from(d.strip_channels),
            history_frames: frames.try_into().map_err(|_| Error::Capacity)?,
            views,
            pages: audio_pages,
        };
        let audio_capacity = if d.audio_render_mode.chunked() {
            audio.chunk_capacity(runtime, d.strips(), d.quantum)?
        } else {
            audio.windowed_capacity(runtime, d.strips())?
        };
        let records = d.history.midi_records as usize;
        let history_events = records / 2 / d.strips();
        let history_bytes = mul(d.history.midi_pages as usize, d.page_bytes)? / 2 / d.strips();
        let tick_bytes = add(mul(d.midi_events, 12)?, d.midi_bytes)?;
        if history_events < d.midi_events || history_bytes < tick_bytes {
            return Err(Error::Capacity);
        }
        let imports = writers.max(1);
        let leases = add(
            add(mul(views, 2)?, mul(writers, 2)?)?,
            add(d.strips(), imports)?,
        )?;
        let index_workspace = indexed_midi::Config::index_workspace(d.midi_events, history_events)?;
        let nodes_with_workspace = add(records, index_workspace)?;
        let midi = indexed_midi::Config {
            packets: indexed_midi::PacketLimits {
                pages: d.history.midi_pages as usize,
                bodies: records,
                records,
                max_packet: d.midi_bytes,
                bases: add(add(d.loops(), writers)?, 8)?,
            },
            indexes: indexed_midi::IndexLimits {
                leases,
                versions: add(add(views, writers)?, add(d.strips(), imports)?)?,
                category_pages: add(
                    add(
                        add(mul(records.div_ceil(128), 4)?, mul(views, 2)?)?,
                        mul(d.strips(), 66)?,
                    )?,
                    66,
                )?,
                packet_nodes: mul(records, 2)?.max(nodes_with_workspace),
                scalar_nodes: mul(nodes_with_workspace, 2)?,
                hold_nodes: mul(records, 2)?.max(nodes_with_workspace),
                sostenuto_nodes: nodes_with_workspace,
            },
            sources: d.strips(),
            history_frames: frames,
            history_events,
            history_bytes,
            views,
            writers,
            imports,
            events_per_tick: d.midi_events,
            bytes_per_tick: d.midi_bytes,
            maintenance_steps_per_tick: 32,
        };
        let midi_capacity = midi.capacity(runtime, q)?;
        result.capacity = HistoryCapacity {
            retention_frames: frames,
            ordinary_payload_bytes,
            history_audio_bytes: mul(history_pages, d.page_bytes)?,
            shared_audio_bytes: mul(audio_pages - history_pages, d.page_bytes)?,
            midi_payload_bytes: midi_capacity.reserved_payload_bytes,
            audio_metadata_bytes: audio_capacity.metadata_bytes,
            midi_metadata_bytes: midi_capacity.metadata_bytes,
            total_resource_pages: total_pages,
            writer_slots: writers,
            midi_events_per_source: history_events,
            midi_bytes_per_source: history_bytes,
        };
        result.audio = Some(audio);
        result.midi = Some(midi);
        Ok(result)
    }
    pub fn settings(self) -> SharedSettings {
        let (Some(audio), Some(midi)) = (self.audio, self.midi) else {
            return SharedSettings::default();
        };
        let shape = shared_midi::Config {
            history_frames: midi.history_frames,
            chunks: 1,
            views: midi.views,
            pages: midi.packets.pages,
            max_events: midi.events_per_tick,
            max_bytes: midi.bytes_per_tick,
            max_packet: midi.packets.max_packet,
            transactions: midi.writers.max(1),
            edit_steps: 64,
        };
        SharedSettings {
            histories: (0..self.sources)
                .map(|source| HistorySettings {
                    source: source as u32,
                    audio: Some(audio),
                    midi: Some(shape),
                })
                .collect(),
            audio_pool: Some(audio),
            midi_pool: Some(midi),
            candidates: self.capacity.writer_slots,
            edit_steps_per_callback: 64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Segment;
    use alloc::vec;
    use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

    #[shoop_test]
    fn production_history_layout_keeps_loop_memory_separate_and_refuses_oversized_shapes_without_allocation(
    ) {
        let default = EngineDescription {
            segments: vec![Segment {
                strips: 16,
                loops: 128,
            }],
            resource_pages: 65536,
            ..EngineDescription::default()
        };
        let capacity = assert_no_heap(|| default.history_capacity()).unwrap();
        assert_eq!(capacity.retention_frames, 1_440_000);
        assert_eq!(capacity.ordinary_payload_bytes, 256 << 20);
        assert_eq!(capacity.history_audio_bytes, 256 << 20);
        assert_eq!(capacity.shared_audio_bytes, (128 << 20) + 8 * 4096);
        assert_eq!(capacity.midi_payload_bytes, 16 << 20);
        assert_eq!(capacity.midi_events_per_source, 2048);
        assert_eq!(capacity.midi_bytes_per_source, 512 << 10);
        assert!(capacity.audio_metadata_bytes < 40 << 20);
        assert!(capacity.midi_metadata_bytes < 128 << 20);
        assert_eq!(capacity.writer_slots, 8);
        assert_eq!(
            capacity.total_resource_pages * default.page_bytes,
            capacity.ordinary_payload_bytes
                + capacity.history_audio_bytes
                + capacity.shared_audio_bytes
                + capacity.midi_payload_bytes
        );
        let largest = EngineDescription {
            segments: vec![Segment {
                strips: 64,
                loops: 256,
            }],
            strip_channels: 8,
            resource_pages: 262144,
            ..EngineDescription::default()
        };
        assert_eq!(
            assert_no_heap(|| largest.history_capacity()),
            Err(Error::Capacity)
        );
        let mut invalid = default.clone();
        invalid.history.midi_records = 1;
        assert_eq!(
            assert_no_heap(|| invalid.history_capacity()),
            Err(Error::Capacity)
        );
        invalid.history.seconds = 0;
        let disabled = assert_no_heap(|| invalid.history_capacity()).unwrap();
        assert_eq!(disabled.total_resource_pages, invalid.resource_pages);
        assert_eq!(disabled.retention_frames, 0);
    }
}
