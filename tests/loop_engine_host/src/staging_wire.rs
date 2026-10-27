//! Bounded resource and timing-program staging; all handles remain receiver-local.
use crate::{
    export_wire::{get32, get64, put32, put64},
    wire::Error,
};
pub const REQUEST_BYTES: usize = 128;
pub const RESPONSE_BYTES: usize = 256;
pub const DATA: usize = 144;
pub const BANKS: usize = 2;
pub const CREDITS: usize = 1;
pub const CREATE: u8 = 1;
pub const BEGIN: u8 = 2;
pub const WRITE: u8 = 3;
pub const FINISH: u8 = 4;
pub const COMMIT: u8 = 5;
pub const SNAPSHOT: u8 = 6;
pub const CURSOR: u8 = 7;
pub const READ: u8 = 8;
pub const RELEASE: u8 = 9;
pub const REMOVE: u8 = 10;
pub const CANCEL_UPLOAD: u8 = 11;
pub const SOURCE: u8 = 13;
pub const VALIDATE_SOURCE: u8 = 14;
pub const RELEASE_SOURCE: u8 = 15;
pub const PROGRAM_BEGIN: u8 = 17;
pub const APPEND: u8 = 18;
pub const VALIDATE: u8 = 19;
pub const ACTIVATE: u8 = 20;
pub const RECYCLE: u8 = 21;
pub const CANCEL: u8 = 22;
pub const PLAY: u8 = 23;
pub const STOP: u8 = 24;
pub const STATUS: u8 = 25;
pub const SEEK_ITERATION: u8 = 26;
pub const ADMITTED: u32 = 1;
pub const APPLIED: u32 = 2;
pub const CANCELLED: u32 = 3;
pub const REJECTED: u32 = 4;
pub const FAILED: u32 = 5;
pub fn resource(kind: u8) -> bool {
    (CREATE..=CANCEL_UPLOAD).contains(&kind)
}
pub fn program(kind: u8) -> bool {
    kind >= PROGRAM_BEGIN
}
pub fn owning(kind: u8) -> bool {
    matches!(kind, WRITE | READ | PROGRAM_BEGIN | APPEND)
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Request {
    pub session: u64,
    pub sequence: u64,
    pub kind: u8,
    pub cancelled: bool,
    pub at: u64,
    pub bank: u32,
    pub generation: u32,
    pub args: [u64; 3],
    pub data: [u8; 64],
}
impl Request {
    pub fn encode(self) -> Result<[u8; REQUEST_BYTES], Error> {
        if self.session == 0 || self.sequence == 0 || self.generation == 0 {
            return Err(Error::Identity);
        }
        if self.bank as usize >= BANKS {
            return Err(Error::Slot);
        }
        if !(resource(self.kind)
            || (SOURCE..=RELEASE_SOURCE).contains(&self.kind)
            || (PROGRAM_BEGIN..=SEEK_ITERATION).contains(&self.kind))
        {
            return Err(Error::Operation);
        }
        if resource(self.kind) && self.at != 0 {
            return Err(Error::Operation);
        }
        if self.kind == BEGIN {
            if !matches!(self.data[0], 1 | 2)
                || self.data[1..4].iter().any(|v| *v != 0)
                || self.data[8..].iter().any(|v| *v != 0)
                || (self.data[0] == 2 && get32(&self.data, 4) != 0)
            {
                return Err(Error::Reserved);
            }
        } else if !matches!(self.kind, WRITE | PROGRAM_BEGIN | APPEND)
            && self.data.iter().any(|v| *v != 0)
        {
            return Err(Error::Reserved);
        }
        let used = match self.kind {
            BEGIN => 3,
            WRITE | READ | APPEND | PLAY | STOP => 1,
            ACTIVATE | SEEK_ITERATION => 2,
            _ => 0,
        };
        if self.args[used..].iter().any(|v| *v != 0) {
            return Err(Error::Reserved);
        }
        let mut b = [0; REQUEST_BYTES];
        b[..4].copy_from_slice(b"LPS1");
        b[4..6].copy_from_slice(&1u16.to_le_bytes());
        b[6] = self.kind;
        b[7] = u8::from(self.cancelled);
        put64(&mut b, 8, self.session);
        put64(&mut b, 16, self.sequence);
        put64(&mut b, 24, self.at);
        put32(&mut b, 32, self.bank);
        put32(&mut b, 36, self.generation);
        for (i, arg) in self.args.into_iter().enumerate() {
            put64(&mut b, 40 + i * 8, arg);
        }
        b[64..].copy_from_slice(&self.data);
        Ok(b)
    }
    pub fn decode(b: &[u8]) -> Result<Self, Error> {
        if b.len() != REQUEST_BYTES {
            return Err(Error::Size);
        }
        if &b[..4] != b"LPS1" {
            return Err(Error::Format);
        }
        if b[4..6] != 1u16.to_le_bytes() {
            return Err(Error::Version);
        }
        let r = Self {
            session: get64(b, 8),
            sequence: get64(b, 16),
            kind: b[6],
            cancelled: b[7] != 0,
            at: get64(b, 24),
            bank: get32(b, 32),
            generation: get32(b, 36),
            args: std::array::from_fn(|i| get64(b, 40 + i * 8)),
            data: b[64..].try_into().unwrap(),
        };
        if r.encode()?.as_slice() != b {
            return Err(Error::Reserved);
        }
        Ok(r)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Response(pub [u8; RESPONSE_BYTES]);
impl Response {
    pub(crate) fn new(
        r: Request,
        phase: u8,
        status: u32,
        runtime: u64,
        effective: u64,
        observed: u64,
    ) -> Self {
        let mut b = [0; RESPONSE_BYTES];
        b[..4].copy_from_slice(b"LPR1");
        b[4..6].copy_from_slice(&1u16.to_le_bytes());
        b[6] = r.kind;
        b[7] = phase;
        put64(&mut b, 8, r.session);
        put64(&mut b, 16, r.sequence);
        put64(&mut b, 24, runtime);
        put64(&mut b, 32, r.at);
        put64(&mut b, 40, effective);
        put64(&mut b, 56, observed);
        put32(&mut b, 64, status);
        put32(&mut b, 72, r.bank);
        put32(&mut b, 76, r.generation);
        put32(&mut b, 80, r.generation);
        Self(b)
    }
    pub fn decode(b: &[u8]) -> Result<Self, Error> {
        if b.len() != RESPONSE_BYTES {
            return Err(Error::Size);
        }
        if &b[..4] != b"LPR1" {
            return Err(Error::Format);
        }
        if b[4..6] != 1u16.to_le_bytes() {
            return Err(Error::Version);
        }
        let kind = b[6];
        let phase = b[7];
        let status = get32(b, 64);
        let flags = get32(b, 84);
        let len = get32(b, 128) as usize;
        if !(resource(kind)
            || (SOURCE..=RELEASE_SOURCE).contains(&kind)
            || (PROGRAM_BEGIN..=SEEK_ITERATION).contains(&kind))
            || !(1..=3).contains(&phase)
            || (phase == 3) != resource(kind)
            || !(ADMITTED..=FAILED).contains(&status)
            || get64(b, 8) == 0
            || get64(b, 16) == 0
            || get32(b, 72) as usize >= BANKS
            || get32(b, 76) == 0
            || flags & !3 != 0
            || len > 64
            || b[136..DATA].iter().any(|v| *v != 0)
            || b[DATA + len..].iter().any(|v| *v != 0)
            || get32(b, 132) & !(if program(kind) { 256 } else { 15 }) != 0
            || (phase == 1 && matches!(status, APPLIED | FAILED))
            || (phase != 1 && status == ADMITTED)
            || (phase == 2 && matches!(status, CANCELLED | REJECTED))
            || (matches!(status, ADMITTED | APPLIED | CANCELLED) && get32(b, 68) != 0)
            || (matches!(status, REJECTED | FAILED) && get32(b, 68) == 0)
            || (get64(b, 24) == 0 && status != REJECTED)
            || get64(b, 40) < get64(b, 32)
            || (flags & 1 == 0 && get64(b, 48) != 0)
            || (phase == 2
                && status == APPLIED
                && (flags & 1 == 0 || get64(b, 48) != get64(b, 40) || get64(b, 56) != get64(b, 48)))
            || (flags & 2 != 0 && (kind != READ || status != APPLIED))
            || (len != 0
                && !(matches!(kind, WRITE | READ) && phase == 3
                    || matches!(kind, PROGRAM_BEGIN | APPEND) && phase == 1))
        {
            return Err(Error::Format);
        }
        Ok(Self(b.try_into().unwrap()))
    }
}
#[cfg(test)]
mod tests;
struct Pending {
    request: Request,
    runtime: Option<(u64, u64)>,
}
#[derive(Clone, Copy)]
struct Mirror {
    generation: u32,
    flags: u32,
    position: Option<u64>,
    bytes: u64,
}
pub struct Client {
    session: u64,
    last: u64,
    pending: Option<Pending>,
    banks: [[Mirror; BANKS]; 2],
}
impl Client {
    pub fn new(session: u64) -> Result<Self, Error> {
        if session == 0 {
            return Err(Error::Session);
        }
        Ok(Self {
            session,
            last: 0,
            pending: None,
            banks: [[Mirror {
                generation: 1,
                flags: 0,
                position: None,
                bytes: 0,
            }; BANKS]; 2],
        })
    }
    pub fn outstanding(&self) -> usize {
        usize::from(self.pending.is_some())
    }
    pub fn retained(&self) -> usize {
        self.banks
            .iter()
            .flatten()
            .map(|b| b.flags.count_ones() as usize)
            .sum()
    }
    pub fn generation(&self, bank: usize, program: bool) -> Option<u32> {
        self.banks[usize::from(program)]
            .get(bank)
            .map(|b| b.generation)
    }
    pub fn prepare(&mut self, mut request: Request) -> Result<[u8; REQUEST_BYTES], Error> {
        if self.pending.is_some() {
            return Err(Error::Busy);
        }
        let bank = self.banks[usize::from(program(request.kind))]
            .get(request.bank as usize)
            .ok_or(Error::Slot)?;
        if bank.generation == 0 || request.generation != bank.generation {
            return Err(Error::Identity);
        }
        request.session = self.session;
        request.sequence = self.last.checked_add(1).ok_or(Error::Exhausted)?;
        let bytes = request.encode()?;
        self.last = request.sequence;
        self.pending = Some(Pending {
            request,
            runtime: None,
        });
        Ok(bytes)
    }
    pub fn receive(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let response = Response::decode(bytes)?;
        let b = &response.0;
        if get64(b, 8) != self.session {
            return Err(Error::Session);
        }
        let pending = self.pending.as_mut().ok_or(Error::Sequence)?;
        let r = pending.request;
        if get64(b, 16) != r.sequence {
            return Err(Error::Sequence);
        }
        if b[6] != r.kind
            || get32(b, 72) != r.bank
            || get32(b, 76) != r.generation
            || get64(b, 32) != r.at
        {
            return Err(Error::Identity);
        }
        let status = get32(b, 64);
        if matches!(r.kind, WRITE | PROGRAM_BEGIN | APPEND)
            && (get32(b, 128) != 64 || b[DATA..DATA + 64] != r.data)
            && b[7] != 2
        {
            return Err(Error::Identity);
        }
        if b[7] == 1 && status == ADMITTED {
            let bank = &self.banks[usize::from(program(r.kind))][r.bank as usize];
            if get32(b, 132) != bank.flags
                || get32(b, 80) != bank.generation
                || b[88..128].iter().any(|v| *v != 0)
            {
                return Err(Error::Identity);
            }
            if pending.runtime.is_some() {
                return Err(Error::Sequence);
            }
            pending.runtime = Some((get64(b, 24), get64(b, 40)));
            return Ok(());
        }
        if b[7] == 2 && pending.runtime != Some((get64(b, 24), get64(b, 40))) {
            return Err(Error::Sequence);
        }
        if b[7] != 2 && pending.runtime.is_some() {
            return Err(Error::Sequence);
        }
        let bank = &mut self.banks[usize::from(program(r.kind))][r.bank as usize];
        let mut flags = bank.flags;
        let mut generation = bank.generation;
        if status == APPLIED {
            match r.kind {
                CREATE => flags |= 1,
                BEGIN => flags |= 2,
                COMMIT | CANCEL_UPLOAD => flags &= !2,
                SNAPSHOT => flags |= 4,
                RELEASE => flags &= !4,
                REMOVE => {
                    flags = 0;
                    generation = generation.checked_add(1).unwrap_or(0);
                }
                SOURCE => flags |= 8,
                RELEASE_SOURCE => flags &= !8,
                PROGRAM_BEGIN => flags = 256,
                RECYCLE | CANCEL => {
                    flags = 0;
                    generation = generation.checked_add(1).unwrap_or(0);
                }
                _ => {}
            }
        }
        if get32(b, 132) != flags || get32(b, 80) != generation {
            return Err(Error::Identity);
        }
        if status == APPLIED && r.kind == READ {
            let end = r.args[0]
                .checked_add(u64::from(get32(b, 128)))
                .ok_or(Error::Format)?;
            if bank.position != Some(r.args[0])
                || get64(b, 88) != end
                || (get32(b, 84) & 2 != 0) != (end == bank.bytes)
            {
                return Err(Error::Identity);
            }
        }
        if status == APPLIED {
            match r.kind {
                SNAPSHOT => bank.bytes = get64(b, 96),
                CURSOR => bank.position = Some(get64(b, 88)),
                READ => bank.position = Some(get64(b, 88)),
                RELEASE | REMOVE => bank.position = None,
                _ => {}
            }
        }
        bank.flags = flags;
        bank.generation = generation;
        self.pending = None;
        Ok(())
    }
}
