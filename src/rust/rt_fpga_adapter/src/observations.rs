//! Historical device cuts, read without changing their receipt ownership.
use crate::{
    audio_actions::{self as action, Identity, Record},
    audio_image::Transfer,
    protocol::{self, get, put},
    resident,
};
use rt_runtime::{
    Epoch, Error, Frame, ResourceId, Revision,
    resource::{CaptureInfo, Description, ExternalReadScope, Format, Snapshot, SnapshotId},
};
use shoop_loops::{
    LoopId,
    audio::Smoothing,
    media,
    runtime::{Cut, Reply, Selection},
    windows::Windows,
};
pub const WORDS: usize = 30;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Reference {
    pub identity: Identity,
    pub epoch: u32,
    pub exported: bool,
    pub frame: Frame,
    pub graph_epoch: Epoch,
    pub topology_epoch: Epoch,
}
impl Reference {
    pub fn decode(record: &Record, epoch: u32) -> Result<Self, Error> {
        let p = &record.payload;
        if record.identity.generation == 0
            || record.terminal_frame < record.effective_frame
            || !record.terminal
            || !record.state
            || record.error != 0
            || !matches!(record.opcode, action::QUERY | action::EXPORT)
            || resident::tag(p) != resident::SNAPSHOT
            || get(p, 0, 16) != u64::from(record.identity.slot)
            || get(p, 16, 32) != u64::from(record.identity.generation)
            || get(p, 80, 8) != WORDS as u64
            || get(p, 89, 7) != 0
            || (288..504).any(|bit| get(p, bit, 1) != 0)
            || get(p, 88, 1) != u64::from(record.opcode == action::EXPORT)
            || get(p, 96, 64) != record.terminal_frame
        {
            return Err(Error::Disconnected);
        }
        if get(p, 48, 32) != u64::from(epoch) {
            return Err(Error::StaleEpoch);
        }
        Ok(Self {
            identity: record.identity,
            epoch,
            exported: record.opcode == action::EXPORT,
            frame: Frame(get(p, 96, 64)),
            graph_epoch: Epoch(get(p, 160, 64)),
            topology_epoch: Epoch(get(p, 224, 64)),
        })
    }
    pub fn word(&self, index: usize) -> Result<Transfer, Error> {
        if index >= WORDS {
            return Err(Error::InvalidOffset);
        }
        let mut t = Transfer {
            opcode: 48,
            state: false,
            address: self.identity.slot,
            words: [0; 20],
        };
        put(&mut t.words, 0, 32, u64::from(self.identity.generation));
        put(&mut t.words, 32, 32, u64::from(self.epoch));
        put(&mut t.words, 64, 8, index as u64);
        Ok(t)
    }
}
pub struct Reader {
    reference: Reference,
    words: [u64; WORDS],
    next: usize,
}
impl Reader {
    pub fn new(reference: Reference) -> Self {
        Self {
            reference,
            words: [0; WORDS],
            next: 0,
        }
    }
    pub fn request(&self) -> Option<Transfer> {
        self.reference.word(self.next).ok()
    }
    pub fn accept(&mut self, error: u8, words: [u32; 26]) -> Result<bool, Error> {
        if error != 0 {
            return Err(match error {
                3 => Error::InvalidId,
                5 => Error::InvalidState,
                _ => Error::Disconnected,
            });
        }
        if self.next == WORDS
            || words[5..].iter().any(|&w| w != 0)
            || get(&words, 64, 16) != self.next as u64
            || get(&words, 80, 16) != u64::from(self.reference.identity.slot)
            || get(&words, 96, 32) != u64::from(self.reference.identity.generation)
            || get(&words, 128, 32) != u64::from(self.reference.epoch)
        {
            return Err(Error::Disconnected);
        }
        self.words[self.next] = get(&words, 0, 64);
        self.next += 1;
        Ok(self.next == WORDS)
    }
    pub fn finish(self) -> Result<[u64; WORDS], Error> {
        if self.next != WORDS {
            Err(Error::Incomplete)
        } else {
            Ok(self.words)
        }
    }
}
pub struct Scope {
    ids: media::IdentityScope,
    reads: ExternalReadScope,
    channels: u32,
    origin: Frame,
}
impl Scope {
    pub fn new(channels: u32, origin: Frame) -> Result<Self, Error> {
        if !(1..=2).contains(&channels) {
            return Err(Error::InvalidConfig);
        }
        Ok(Self {
            ids: media::IdentityScope::new()?,
            reads: ExternalReadScope::new()?,
            channels,
            origin,
        })
    }
    fn frame(&self, frame: u64) -> Result<Frame, Error> {
        self.origin
            .0
            .checked_add(frame)
            .map(Frame)
            .ok_or(Error::Exhausted)
    }
    fn media(&self, slot: u32, generation: u32) -> Result<media::MediaId, Error> {
        if slot != u32::MAX && slot >= 8 {
            return Err(Error::Disconnected);
        }
        self.ids.media(slot, generation)
    }
    pub fn export_parts(&self, id: media::ExportId) -> Result<[u32; 2], Error> {
        let [_, slot, generation] = id.parts();
        if self.ids.export(slot, generation)? != id {
            return Err(Error::InvalidId);
        }
        Ok([slot, generation])
    }
    pub fn media_parts(&self, id: media::MediaId) -> Result<[u32; 2], Error> {
        let [_, slot, generation] = id.parts();
        if self.ids.media(slot, generation)? != id {
            return Err(Error::InvalidId);
        }
        Ok([slot, generation])
    }

    fn media_action(&self, opcode: u8, parts: [u32; 3], valid: bool, error: u8) -> Transfer {
        let mut t = Transfer {
            opcode,
            state: false,
            address: 0,
            words: [0; 20],
        };
        put(&mut t.words, 0, 32, u64::from(parts[1]));
        put(&mut t.words, 32, 32, u64::from(parts[2]));
        if !valid {
            put(&mut t.words, resident::DEFERRED_ERROR, 8, u64::from(error));
        }
        t
    }
    pub fn release(&self, id: media::ExportId) -> Transfer {
        self.media_action(
            action::RELEASE,
            id.parts(),
            self.export_parts(id).is_ok(),
            1,
        )
    }
    pub fn request_cursor(&self, id: media::ExportId, midi: bool) -> Transfer {
        self.media_action(
            action::CURSOR,
            id.parts(),
            !midi && self.export_parts(id).is_ok(),
            if midi { 2 } else { 1 },
        )
    }
    pub fn retire(&self, id: media::MediaId) -> Transfer {
        self.media_action(action::RETIRE, id.parts(), self.media_parts(id).is_ok(), 1)
    }
    pub fn cursor(&self, payload: &[u32; 16]) -> Result<rt_runtime::resource::Cursor, Error> {
        if resident::tag(payload) != resident::CURSOR
            || (64..504).any(|bit| get(payload, bit, 1) != 0)
            || payload[0] >= 8
            || payload[1] == 0
        {
            return Err(Error::Disconnected);
        }
        Ok(self.reads.cursor(SnapshotId {
            slot: payload[0],
            generation: payload[1],
        }))
    }
    pub fn cursor_snapshot(
        &self,
        cursor: rt_runtime::resource::Cursor,
    ) -> Result<SnapshotId, Error> {
        self.reads.snapshot(cursor)
    }
    pub fn decode(
        &self,
        reference: Reference,
        raw: &[u64; WORDS],
        target: LoopId,
        selection: Option<Selection>,
    ) -> Result<Reply, Error> {
        let words: [u32; 60] = std::array::from_fn(|i| {
            if i % 2 == 0 {
                raw[i / 2] as u32
            } else {
                (raw[i / 2] >> 32) as u32
            }
        });
        if reference.exported != selection.is_some()
            || get(&words, 832, 64) != reference.frame.0
            || get(&words, 896, 64) != reference.graph_epoch.0
            || get(&words, 1825, 64) != reference.topology_epoch.0
            || words[59] >> 1 != 0
        {
            return Err(Error::Disconnected);
        }
        let mut state = protocol::state(&std::array::from_fn(|i| words[i]), 0)?;
        if state.id != target {
            return Err(Error::Disconnected);
        }
        if let Some(pulse) = state.last_pulse {
            state.last_pulse = Some(self.frame(pulse.0)?);
        }
        let latest_slot = get(&words, 960, 32) as u32;
        let latest = if latest_slot == u32::MAX {
            None
        } else {
            Some(self.media(latest_slot, get(&words, 992, 32) as u32)?)
        };
        let playback = self.media(get(&words, 1024, 32) as u32, get(&words, 1056, 32) as u32)?;
        let gain = f32::from_bits(words[54]);
        let balance = f32::from_bits(words[55]);
        if !gain.is_finite() || !balance.is_finite() || (words[57] & 1 == 0 && words[56] != 0) {
            return Err(Error::Disconnected);
        }
        let frame = self.frame(reference.frame.0)?;
        let cut = Cut {
            raw_audio_frames: state.content_length,
            raw_midi_frames: 0,
            windows: Windows::default(),
            timing_revision: state.timing_revision,
            frame,
            graph_epoch: reference.graph_epoch,
            topology_epoch: reference.topology_epoch,
            state,
            playback: Some(playback),
            latest,
            gain,
            balance,
            smoothing: if words[57] & 1 == 0 {
                Smoothing::None
            } else {
                Smoothing::LinearRestart { frames: words[56] }
            },
            capture_failure: None,
            midi_overflows: 0,
        };
        let Some(selection) = selection else {
            if words[34] != u32::MAX || words[35] != 0 || words[36..54].iter().any(|&w| w != 0) {
                return Err(Error::Disconnected);
            }
            return Ok(Reply::State(cut));
        };
        let selected = self.media(words[50], words[51])?;
        if match selection {
            Selection::Playback => Some(playback),
            Selection::Latest => latest,
            Selection::Recording => None,
        } != Some(selected)
            || words[34] >= 4
            || words[35] == 0
        {
            return Err(Error::Disconnected);
        }
        let id = self.ids.export(words[34], words[35])?;
        let revision = Revision(get(&words, 1152, 64));
        let frames = get(&words, 1216, 64);
        let description = media::Description {
            owner: state.id,
            revision,
            frames,
            channels: self.channels,
            auxiliary_channels: 0,
            midi: false,
        };
        let view = if words[50] == u32::MAX {
            if frames != 0 {
                return Err(Error::Disconnected);
            }
            media::View {
                windows: None,
                description,
                audio: None,
                auxiliary: None,
                midi: None,
            }
        } else {
            let bank = get(&words, 1536, 64);
            let start = get(&words, 1344, 64);
            let end = get(&words, 1408, 64);
            if bank > 1
                || start > end
                || end > reference.frame.0
                || get(&words, 1664, 64)
                    != (u64::from(state.id.slot()) | (u64::from(state.id.generation()) << 32))
            {
                return Err(Error::Disconnected);
            }
            let snapshot = Snapshot {
                id: SnapshotId {
                    slot: words[50],
                    generation: words[51],
                },
                resource: ResourceId::from_parts(bank as u32, 1)?,
                revision: Revision(get(&words, 1472, 64)),
                description: Description {
                    format: Format::Audio {
                        channels: self.channels,
                    },
                    frames,
                    bytes: usize::try_from(frames)
                        .ok()
                        .and_then(|n| n.checked_mul(4 * self.channels as usize))
                        .ok_or(Error::Capacity)?,
                },
                cut_frame: self.frame(end)?,
                capture: Some(CaptureInfo {
                    start_frame: self.frame(start)?,
                    next_frame: self.frame(end)?,
                    active: false,
                    incomplete: None,
                }),
            };
            media::View {
                windows: Some(Windows::default()),
                description,
                audio: Some(media::Source {
                    snapshot,
                    index: self.reads.index(snapshot.id),
                }),
                auxiliary: None,
                midi: None,
            }
        };
        Ok(Reply::Export {
            id,
            cut,
            media: media::Export {
                media: selected,
                requested_cut: frame,
                view,
            },
        })
    }
}
