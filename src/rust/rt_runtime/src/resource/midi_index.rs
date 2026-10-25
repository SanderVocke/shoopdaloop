//! Opt-in fixed MIDI checkpoint rows, owned by resource versions.
use super::*;
use crate::midi_state::MidiState;
use format::RecordSummary;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MidiIndexConfig {
    pub versions: usize,
    pub checkpoints_per_version: usize,
    pub replay_events: usize,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MidiIndexUsage {
    pub free_versions: usize,
    pub versions: usize,
    pub checkpoints_per_version: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MidiIndexInfo {
    pub events: u64,
    pub checkpoints: usize,
    pub replay_events: usize,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MidiIndexWork {
    pub background_records: usize,
    pub render_records: usize,
    pub replayed_records: usize,
    pub state_copies: usize,
    pub counter_overflow: bool,
}
impl MidiIndexWork {
    fn add(&mut self, records: usize, copies: usize, replay: usize, class: WorkClass) {
        let record_counter = match class {
            WorkClass::Background => &mut self.background_records,
            WorkClass::Render => &mut self.render_records,
        };
        for (counter, value) in [
            (record_counter, records),
            (&mut self.state_copies, copies),
            (&mut self.replayed_records, replay),
        ] {
            match counter.checked_add(value) {
                Some(n) => *counter = n,
                None => {
                    *counter = usize::MAX;
                    self.counter_overflow = true;
                }
            }
        }
    }
}
#[derive(Clone, Copy)]
pub(super) struct MidiOwner(usize);
#[derive(Clone, Copy, Default)]
struct MidiVersion {
    source: Option<UploadId>,
    state: MidiState,
    events: u64,
    used: usize,
}
#[derive(Clone, Copy, Default)]
struct Checkpoint {
    frame: Option<u64>,
    end: usize,
    state: MidiState,
}
#[derive(Default)]
pub(super) struct MidiPool {
    config: Option<MidiIndexConfig>,
    versions: Vec<MidiVersion>,
    checkpoints: Vec<Checkpoint>,
    free: Vec<usize>,
    pub work: MidiIndexWork,
}
impl MidiPool {
    pub fn new(config: MidiIndexConfig) -> Result<Self, Error> {
        if config.versions == 0 || config.checkpoints_per_version == 0 || config.replay_events == 0
        {
            return Err(Error::InvalidConfig);
        }
        if config.replay_events > i32::MAX as usize {
            return Err(Error::Capacity);
        }
        let count = config
            .versions
            .checked_mul(config.checkpoints_per_version)
            .ok_or(Error::Capacity)?;
        let bytes = count
            .checked_mul(size_of::<Checkpoint>())
            .and_then(|n| {
                config
                    .versions
                    .checked_mul(size_of::<MidiVersion>())
                    .and_then(|m| n.checked_add(m))
            })
            .ok_or(Error::Capacity)?;
        if bytes > i32::MAX as usize {
            return Err(Error::Capacity);
        }
        let mut versions = vec![MidiVersion::default(); config.versions];
        let mut checkpoints = vec![Checkpoint::default(); count];
        for row in &mut versions {
            *std::hint::black_box(row) = MidiVersion::default();
        }
        for checkpoint in &mut checkpoints {
            *std::hint::black_box(checkpoint) = Checkpoint::default();
        }
        Ok(Self {
            config: Some(config),
            versions,
            checkpoints,
            free: (0..config.versions).rev().collect(),
            work: MidiIndexWork::default(),
        })
    }
    pub fn usage(&self) -> MidiIndexUsage {
        MidiIndexUsage {
            free_versions: self.free.len(),
            versions: self.versions.len(),
            checkpoints_per_version: self.config.map_or(0, |c| c.checkpoints_per_version),
        }
    }
    pub fn prepare(&self) -> Result<(), Error> {
        if self.config.is_none() {
            return Err(Error::InvalidState);
        }
        if self.free.is_empty() {
            return Err(Error::Capacity);
        }
        Ok(())
    }
    pub fn begin(&mut self, source: UploadId, initial: &MidiState) -> MidiOwner {
        let slot = self.free.pop().expect("prepared MIDI version credit");
        let config = self.config.expect("enabled MIDI indexing");
        self.versions[slot] = MidiVersion {
            source: Some(source),
            state: *initial,
            events: 0,
            used: 1,
        };
        self.checkpoints[slot * config.checkpoints_per_version] = Checkpoint {
            frame: None,
            end: 0,
            state: *initial,
        };
        self.work.add(0, 2, 0, WorkClass::Background);
        MidiOwner(slot)
    }
    pub fn preflight(&self, owner: MidiOwner, source: UploadId, records: u64) -> Result<(), Error> {
        let row = self.row(owner, source)?;
        let config = self.config.ok_or(Error::InvalidState)?;
        let future = row.events.checked_add(records).ok_or(Error::Exhausted)?;
        let additional = usize::try_from(
            future / config.replay_events as u64 - row.events / config.replay_events as u64,
        )
        .map_err(|_| Error::Capacity)?;
        if row
            .used
            .checked_add(additional)
            .is_none_or(|n| n > config.checkpoints_per_version)
        {
            return Err(Error::Capacity);
        }
        Ok(())
    }
    fn row(&self, owner: MidiOwner, source: UploadId) -> Result<&MidiVersion, Error> {
        self.versions
            .get(owner.0)
            .filter(|row| row.source == Some(source))
            .ok_or(Error::InvalidId)
    }
    pub fn info(&self, owner: MidiOwner, source: UploadId) -> Result<MidiIndexInfo, Error> {
        let row = self.row(owner, source)?;
        Ok(MidiIndexInfo {
            events: row.events,
            checkpoints: row.used,
            replay_events: self.config.ok_or(Error::InvalidState)?.replay_events,
        })
    }
    pub fn append(&mut self, owner: MidiOwner, record: RecordSummary, class: WorkClass) {
        let config = self.config.expect("prepared MIDI index");
        let row = &mut self.versions[owner.0];
        apply_short(&mut row.state, record.short);
        row.events += 1;
        self.work.add(1, 0, 0, class);
        if row.events % config.replay_events as u64 == 0 {
            assert!(
                row.used < config.checkpoints_per_version,
                "reserved checkpoint capacity"
            );
            self.checkpoints[owner.0 * config.checkpoints_per_version + row.used] = Checkpoint {
                frame: Some(record.frame),
                end: record.end,
                state: row.state,
            };
            row.used += 1;
            self.work.add(0, 1, 0, class);
        }
    }
    pub fn before(
        &mut self,
        owner: MidiOwner,
        source: UploadId,
        frame: u64,
        prefix: usize,
    ) -> Result<(MidiState, usize, usize), Error> {
        let row = self.row(owner, source)?;
        let config = self.config.ok_or(Error::InvalidState)?;
        let base = owner.0 * config.checkpoints_per_version;
        let (mut low, mut high) = (0, row.used);
        while low < high {
            let middle = low + (high - low) / 2;
            let checkpoint = &self.checkpoints[base + middle];
            if checkpoint.frame.is_none_or(|f| f < frame) && checkpoint.end <= prefix {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        let checkpoint = self.checkpoints[base + low.checked_sub(1).ok_or(Error::InvalidState)?];
        self.work.add(0, 1, 0, WorkClass::Render);
        Ok((checkpoint.state, checkpoint.end, config.replay_events))
    }
    pub fn replay(&mut self, state: &mut MidiState, short: [u8; 3]) {
        apply_short(state, short);
        self.work.add(0, 0, 1, WorkClass::Render);
    }
    pub fn release(&mut self, owner: MidiOwner) {
        let row = &mut self.versions[owner.0];
        assert!(row.source.is_some());
        row.source = None;
        row.used = 0;
        row.events = 0;
        self.free.push(owner.0);
    }
}
fn apply_short(state: &mut MidiState, short: [u8; 3]) {
    let count = match short[0] {
        0x80..=0xbf | 0xe0..=0xef => 3,
        0xc0..=0xdf => 2,
        0xff => 1,
        _ => return,
    };
    state
        .apply_packet(&short[..count], 3)
        .expect("validated complete MIDI record");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[shoop_wasm_test_support::shoop_test]
    fn saturated_work_counters_do_not_replace_semantic_record_exhaustion() {
        let mut pool = MidiPool::new(MidiIndexConfig {
            versions: 1,
            checkpoints_per_version: 2,
            replay_events: 1,
        })
        .unwrap();
        let source = UploadId {
            slot: 0,
            generation: 1,
        };
        let owner = pool.begin(source, &MidiState::default());
        pool.work = MidiIndexWork {
            background_records: usize::MAX,
            render_records: usize::MAX,
            replayed_records: usize::MAX,
            state_copies: usize::MAX,
            counter_overflow: false,
        };
        pool.preflight(owner, source, 1).unwrap();
        pool.append(
            owner,
            RecordSummary {
                frame: 0,
                end: 15,
                short: [0x90, 60, 99],
            },
            WorkClass::Render,
        );
        let (state, offset, _) = pool.before(owner, source, 1, 15).unwrap();
        assert_eq!(offset, 15);
        assert_eq!(state.channel(0).unwrap().note(60).unwrap().velocity(), 99);
        assert!(pool.work.counter_overflow);
        assert_eq!(
            pool.preflight(owner, source, u64::MAX),
            Err(Error::Exhausted)
        );
        pool.release(owner);
        let next = UploadId {
            slot: 0,
            generation: 2,
        };
        pool.begin(next, &MidiState::default());
        assert_eq!(pool.info(owner, source), Err(Error::InvalidId));
    }
}

impl ResourceStore {
    pub fn with_midi_indexing(
        config: PreparedConfig,
        indexing: MidiIndexConfig,
    ) -> Result<Self, Error> {
        let pool = MidiPool::new(indexing)?;
        let mut store = Self::new(config)?;
        store.midi_indexes = pool;
        Ok(store)
    }
    pub fn midi_index_usage(&self) -> MidiIndexUsage {
        self.midi_indexes.usage()
    }
    pub fn midi_index_work(&self) -> MidiIndexWork {
        self.midi_indexes.work
    }
    pub fn begin_indexed_midi_upload(
        &mut self,
        resource: ResourceId,
        expected: Revision,
        description: Description,
        initial: &MidiState,
    ) -> Result<UploadId, Error> {
        if description.format != Format::Midi {
            return Err(Error::InvalidShape);
        }
        self.midi_indexes.prepare()?;
        let upload = self.begin_upload(resource, expected, description)?;
        let owner = self.midi_indexes.begin(upload, initial);
        self.versions[upload.slot as usize]
            .value
            .as_mut()
            .expect("reserved version")
            .midi = Some(owner);
        Ok(upload)
    }
    pub fn midi_index_info(&self, resource: ResourceId) -> Result<MidiIndexInfo, Error> {
        let source = get(&self.resources, resource.slot(), resource.generation())?
            .active
            .ok_or(Error::InvalidState)?;
        let version = get(&self.versions, source.slot, source.generation)?;
        self.midi_indexes
            .info(version.midi.ok_or(Error::InvalidState)?, source)
    }
}
