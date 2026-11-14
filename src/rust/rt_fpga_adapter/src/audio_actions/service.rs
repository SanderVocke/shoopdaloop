use super::{Identity, Record, begin, collect, commit, install, node, read};
use crate::{audio_image::Transfer, protocol::get};
use rt_runtime::Error;

struct Entry {
    sequence: u64,
    requested: u64,
    effective: u64,
    original: Transfer,
    published: bool,
    terminal: Option<Record>,
}
struct Slot {
    generation: u32,
    entry: Option<Entry>,
}
enum Kind {
    Schedule,
    Publish,
    Read,
    Collect,
    Cancel,
    Abort(u8),
}
enum Stage {
    Install,
    Publication(usize),
    Record,
}
struct Transaction {
    kind: Kind,
    stage: Stage,
    slot: usize,
}
pub enum Progress {
    Transfer(Transfer),
    Admitted {
        identity: Identity,
        effective_frame: u64,
    },
    Published {
        frame: u64,
    },
    Record {
        record: Record,
        original: Transfer,
        requested_frame: u64,
    },
    Rejected(u8),
}
/// ARM-side storage ownership. Publication ACK is the action admission point.
pub struct Service {
    slots: Vec<Slot>,
    transaction: Option<Transaction>,
    program: Vec<Transfer>,
    version: u64,
    quarantined: bool,
    quantum: u32,
}
impl Service {
    pub fn new(capacity: u32, quantum: u32) -> Result<Self, Error> {
        if capacity == 0 || capacity > 65536 || !matches!(quantum, 1 | 16 | 32 | 64 | 128) {
            return Err(Error::InvalidConfig);
        }
        Ok(Self {
            slots: (0..capacity)
                .map(|_| Slot {
                    generation: 1,
                    entry: None,
                })
                .collect(),
            transaction: None,
            program: Vec::with_capacity(capacity as usize + 2),
            version: 0,
            quarantined: false,
            quantum,
        })
    }
    pub fn uncertain(&mut self) {
        self.quarantined = true;
    }
    fn available(&self) -> Result<(), u8> {
        if self.quarantined || self.transaction.is_some() {
            Err(5)
        } else {
            Ok(())
        }
    }
    fn identity(&self, slot: usize) -> Identity {
        Identity {
            slot: slot as u16,
            generation: self.slots[slot].generation,
        }
    }
    fn publication(&mut self, frame: u64) {
        let mut members: Vec<_> = self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| {
                let e = slot.entry.as_ref()?;
                e.terminal
                    .is_none()
                    .then_some((e.effective.max(frame), e.sequence, index))
            })
            .collect();
        members.sort_unstable();
        self.program.clear();
        self.program.push(begin(self.version, members.len() as u32));
        for (index, (_, _, slot)) in members.into_iter().enumerate() {
            self.program
                .push(node(index as u16, self.identity(slot), 0));
        }
        self.program.push(commit(0));
    }
    pub fn schedule(
        &mut self,
        sequence: u64,
        requested: u64,
        action: Transfer,
    ) -> Result<Transfer, u8> {
        self.available()?;
        if self
            .slots
            .iter()
            .filter_map(|s| s.entry.as_ref())
            .any(|e| e.sequence == sequence)
        {
            return Err(4);
        }
        let slot = self
            .slots
            .iter()
            .position(|s| s.generation != 0 && s.entry.is_none())
            .ok_or(2)?;
        let effective = requested
            .checked_add(u64::from(self.quantum - 1))
            .ok_or(8)?
            & !u64::from(self.quantum - 1);
        let transfer =
            install(self.identity(slot), sequence, requested, &action).map_err(|_| 10u8)?;
        self.slots[slot].entry = Some(Entry {
            sequence,
            requested,
            effective,
            original: action,
            published: false,
            terminal: None,
        });
        self.transaction = Some(Transaction {
            kind: Kind::Schedule,
            stage: Stage::Install,
            slot,
        });
        Ok(transfer)
    }
    pub fn publish(&mut self, frame: u64) -> Result<Transfer, u8> {
        self.available()?;
        self.publication(frame);
        self.transaction = Some(Transaction {
            kind: Kind::Publish,
            stage: Stage::Publication(0),
            slot: 0,
        });
        Ok(self.program[0].clone())
    }
    pub fn inspect(&mut self, id: Identity, release: bool) -> Result<Transfer, u8> {
        self.record_request(id, if release { Kind::Collect } else { Kind::Read })
    }
    pub fn cancel(&mut self, id: Identity) -> Result<Transfer, u8> {
        self.record_request(id, Kind::Cancel)
    }
    fn record_request(&mut self, id: Identity, kind: Kind) -> Result<Transfer, u8> {
        self.available()?;
        let slot = self.slots.get(id.slot as usize).ok_or(3)?;
        if slot.generation != id.generation || slot.entry.is_none() {
            return Err(3);
        }
        let transfer = match kind {
            Kind::Collect => collect(id),
            Kind::Cancel => super::cancel(id),
            _ => read(id),
        };
        self.transaction = Some(Transaction {
            kind,
            stage: Stage::Record,
            slot: id.slot as usize,
        });
        Ok(transfer)
    }
    fn checked_record(&self, slot: usize, words: &[u32; 26]) -> Result<Record, Error> {
        let r = Record::decode(slot as u16, words)?;
        let e = self.slots[slot].entry.as_ref().ok_or(Error::Disconnected)?;
        if r.identity != self.identity(slot)
            || r.sequence != e.sequence
            || r.opcode != e.original.opcode
            || r.state != e.original.state
            || r.address != e.original.address
            || (!r.terminal && r.payload != e.original.words[..7])
            || r.published != e.published
            || r.effective_frame != e.effective
            || e.terminal.as_ref().is_some_and(|old| old != &r)
        {
            return Err(Error::Disconnected);
        }
        Ok(r)
    }
    fn release(&mut self, slot: usize) {
        self.slots[slot].entry = None;
        self.slots[slot].generation = self.slots[slot].generation.checked_add(1).unwrap_or(0);
    }
    pub fn finish(
        &mut self,
        error: u8,
        words: [u32; 26],
        device_frame: u64,
    ) -> Result<Progress, Error> {
        if self.quarantined {
            return Err(Error::Disconnected);
        }
        let result = self.finish_inner(error, words, device_frame);
        if result.is_err() {
            self.quarantined = true;
        }
        result
    }
    fn finish_inner(
        &mut self,
        error: u8,
        words: [u32; 26],
        device_frame: u64,
    ) -> Result<Progress, Error> {
        let mut t = self.transaction.take().ok_or(Error::Disconnected)?;
        if error > 10 || (error != 0 && words.iter().any(|&w| w != 0)) {
            return Err(Error::Disconnected);
        }
        if error != 0 {
            if matches!(t.stage,Stage::Publication(index) if index+1==self.program.len())
                && error == 5
            {
                self.publication(device_frame);
                t.stage = Stage::Publication(0);
                self.transaction = Some(t);
                return Ok(Progress::Transfer(self.program[0].clone()));
            }
            if matches!(t.kind, Kind::Schedule) && !matches!(t.stage, Stage::Install) {
                t.kind = Kind::Abort(error);
                t.stage = Stage::Record;
                let transfer = super::abort(self.identity(t.slot));
                self.transaction = Some(t);
                return Ok(Progress::Transfer(transfer));
            }
            if matches!(t.kind, Kind::Abort(_)) {
                return Err(Error::Disconnected);
            }
            if matches!(t.stage, Stage::Install) {
                self.slots[t.slot].entry = None;
            }
            return Ok(Progress::Rejected(error));
        }
        match t.stage {
            Stage::Install => {
                let record = self.checked_record(t.slot, &words)?;
                if record.published || record.terminal {
                    return Err(Error::Disconnected);
                }
                self.slots[t.slot].entry.as_mut().unwrap().effective = record.effective_frame;
                self.publication(device_frame);
                t.stage = Stage::Publication(0);
                self.transaction = Some(t);
                Ok(Progress::Transfer(self.program[0].clone()))
            }
            Stage::Publication(index) => {
                if index == 0 {
                    let nonce = get(&words, 0, 64);
                    if nonce == 0 || words[2..].iter().any(|&w| w != 0) {
                        return Err(Error::Disconnected);
                    }
                    for operation in &mut self.program[1..] {
                        crate::protocol::put(
                            &mut operation.words,
                            if operation.opcode == 17 { 64 } else { 0 },
                            64,
                            nonce,
                        );
                    }
                } else if index + 1 == self.program.len() {
                    let version = get(&words, 0, 64);
                    let frame = get(&words, 64, 64);
                    if Some(version) != self.version.checked_add(1)
                        || frame != device_frame
                        || words[4..].iter().any(|&w| w != 0)
                    {
                        return Err(Error::Disconnected);
                    }
                    self.version = version;
                    for entry in self.slots.iter_mut().filter_map(|s| s.entry.as_mut()) {
                        if !entry.published && entry.terminal.is_none() {
                            entry.published = true;
                            entry.effective = entry.effective.max(frame);
                        }
                    }
                    return Ok(match t.kind {
                        Kind::Schedule => Progress::Admitted {
                            identity: self.identity(t.slot),
                            effective_frame: self.slots[t.slot].entry.as_ref().unwrap().effective,
                        },
                        Kind::Publish => Progress::Published { frame },
                        _ => return Err(Error::Disconnected),
                    });
                } else if words.iter().any(|&w| w != 0) {
                    return Err(Error::Disconnected);
                }
                t.stage = Stage::Publication(index + 1);
                self.transaction = Some(t);
                Ok(Progress::Transfer(self.program[index + 1].clone()))
            }
            Stage::Record => {
                let record = self.checked_record(t.slot, &words)?;
                if let Kind::Abort(error) = t.kind {
                    if record.published {
                        return Err(Error::Disconnected);
                    }
                    self.release(t.slot);
                    return Ok(Progress::Rejected(error));
                }
                if matches!(t.kind, Kind::Collect | Kind::Cancel) && !record.terminal {
                    return Err(Error::Disconnected);
                }
                let entry = self.slots[t.slot].entry.as_mut().unwrap();
                let original = entry.original.clone();
                let requested_frame = entry.requested;
                if record.terminal {
                    entry.terminal = Some(record.clone());
                }
                if matches!(t.kind, Kind::Collect) {
                    self.release(t.slot);
                }
                Ok(Progress::Record {
                    record,
                    original,
                    requested_frame,
                })
            }
        }
    }
}
