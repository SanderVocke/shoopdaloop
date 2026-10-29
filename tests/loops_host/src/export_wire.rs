//! Prepared immutable-export protocol. Handles are receiver-local, never pointers.
use crate::wire::Error;
#[cfg(test)]
mod tests;
use shoop_loops::LoopId;

pub const REQUEST_BYTES: usize = 96;
pub const RESPONSE_BYTES: usize = 1024;
pub const DATA: usize = 256;
pub const INITIAL_BYTES: usize = 656;
pub const BANKS: usize = 2;
pub const CREDITS: usize = 1;
pub const EXPORT: u8 = 1;
pub const AUDIO_CURSOR: u8 = 2;
pub const MIDI_CURSOR: u8 = 3;
pub const AUDIO_READ: u8 = 4;
pub const MIDI_READ: u8 = 5;
pub const INITIAL: u8 = 6;
pub const RELEASE: u8 = 7;
pub const ADMITTED: u32 = 1;
pub const APPLIED: u32 = 2;
pub const CANCELLED: u32 = 3;
pub const REJECTED: u32 = 4;
pub const FAILED: u32 = 5;

pub(crate) fn get32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}
pub(crate) fn get64(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}
pub(crate) fn put32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
pub(crate) fn put64(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
fn header(bytes: &[u8], size: usize, magic: &[u8; 4]) -> Result<(), Error> {
    if bytes.len() != size {
        return Err(Error::Size);
    }
    if &bytes[..4] != magic {
        return Err(Error::Format);
    }
    if bytes[4..6] != 1u16.to_le_bytes() {
        return Err(Error::Version);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Request {
    pub session: u64,
    pub sequence: u64,
    pub kind: u8,
    pub at: u64,
    pub bank: u32,
    pub generation: u32,
    pub target: Option<LoopId>,
    pub position: u64,
    pub channel: u8,
    pub selection: u8,
    pub cancelled: bool,
}
impl Request {
    pub fn encode(self) -> Result<[u8; REQUEST_BYTES], Error> {
        if self.session == 0 || self.sequence == 0 || self.generation == 0 {
            return Err(Error::Identity);
        }
        if self.bank as usize >= BANKS {
            return Err(Error::Slot);
        }
        if !(EXPORT..=RELEASE).contains(&self.kind) {
            return Err(Error::Operation);
        }
        if (self.kind == EXPORT) != self.target.is_some()
            || (self.kind != EXPORT && self.selection != 0)
            || self.selection > 2
            || (self.kind != INITIAL && self.channel != 0)
            || self.channel >= 16
            || (!matches!(self.kind, AUDIO_READ | MIDI_READ) && self.position != 0)
            || (matches!(self.kind, AUDIO_READ | MIDI_READ) && self.at != 0)
        {
            return Err(Error::Operation);
        }
        let mut bytes = [0; REQUEST_BYTES];
        bytes[..4].copy_from_slice(b"LLE1");
        bytes[4..6].copy_from_slice(&1u16.to_le_bytes());
        bytes[6] = self.kind;
        bytes[7] = u8::from(self.cancelled);
        put64(&mut bytes, 8, self.session);
        put64(&mut bytes, 16, self.sequence);
        put64(&mut bytes, 24, self.at);
        put32(&mut bytes, 36, self.bank);
        put32(&mut bytes, 40, self.generation);
        if let Some(target) = self.target {
            put32(&mut bytes, 44, target.slot());
            put32(&mut bytes, 48, target.generation());
        }
        bytes[52] = self.channel;
        bytes[53] = self.selection;
        put64(&mut bytes, 56, self.position);
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        header(bytes, REQUEST_BYTES, b"LLE1")?;
        let result = Self {
            session: get64(bytes, 8),
            sequence: get64(bytes, 16),
            kind: bytes[6],
            cancelled: bytes[7] != 0,
            at: get64(bytes, 24),
            bank: get32(bytes, 36),
            generation: get32(bytes, 40),
            target: if bytes[6] == EXPORT {
                Some(
                    LoopId::from_parts(get32(bytes, 44), get32(bytes, 48))
                        .map_err(|_| Error::Identity)?,
                )
            } else {
                None
            },
            channel: bytes[52],
            selection: bytes[53],
            position: get64(bytes, 56),
        };
        if result.encode()?.as_slice() != bytes {
            return Err(Error::Reserved);
        }
        Ok(result)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Response(pub [u8; RESPONSE_BYTES]);
impl Response {
    pub(crate) fn new(
        request: Request,
        phase: u8,
        status: u32,
        runtime: u64,
        observed: u64,
    ) -> Self {
        let mut bytes = [0; RESPONSE_BYTES];
        bytes[..4].copy_from_slice(b"LLF1");
        bytes[4..6].copy_from_slice(&1u16.to_le_bytes());
        bytes[6] = request.kind;
        bytes[7] = phase;
        put32(&mut bytes, 12, status);
        put64(&mut bytes, 16, request.session);
        put64(&mut bytes, 24, request.sequence);
        put64(&mut bytes, 32, runtime);
        put64(&mut bytes, 40, request.at);
        put64(&mut bytes, 48, request.at);
        put64(&mut bytes, 64, observed);
        put32(&mut bytes, 76, request.bank);
        put32(&mut bytes, 80, request.generation);
        put64(&mut bytes, 88, request.position);
        Self(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        header(bytes, RESPONSE_BYTES, b"LLF1")?;
        let kind = bytes[6];
        let phase = bytes[7];
        let flags = get32(bytes, 8);
        let status = get32(bytes, 12);
        let length = get32(bytes, 100) as usize;
        let applied = status == APPLIED;
        if !(EXPORT..=RELEASE).contains(&kind)
            || !(1..=3).contains(&phase)
            || !(ADMITTED..=FAILED).contains(&status)
            || get64(bytes, 16) == 0
            || get64(bytes, 24) == 0
            || get32(bytes, 76) as usize >= BANKS
            || get32(bytes, 80) == 0
            || flags & !7 != 0
            || length > RESPONSE_BYTES - DATA
            || bytes[DATA + length..].iter().any(|&v| v != 0)
            || (flags & 1 == 0 && get64(bytes, 56) != 0)
            || (phase == 1 && status == APPLIED)
            || (phase != 1 && status == ADMITTED)
            || ((phase == 3) != matches!(kind, AUDIO_READ | MIDI_READ))
            || (flags & 2 != 0) != (applied && kind == EXPORT)
            || (flags & 4 != 0 && (!applied || phase != 3))
            || (kind != RELEASE || !applied) && get32(bytes, 84) != 0
            || (kind != EXPORT || !applied) && bytes[104..DATA].iter().any(|&v| v != 0)
            || (kind != INITIAL && phase != 3 && length != 0)
            || (kind == INITIAL && length != if applied { INITIAL_BYTES } else { 0 })
            || (phase == 3 && (length > 64 || get32(bytes, 96) as usize != length))
            || (phase != 3 && get32(bytes, 96) != 0)
            || (!matches!(kind, AUDIO_READ | MIDI_READ) && get64(bytes, 88) != 0)
            || (matches!(status, ADMITTED | APPLIED | CANCELLED) && get32(bytes, 72) != 0)
            || (status == REJECTED && !(1..=20).contains(&get32(bytes, 72)))
            || (status == FAILED
                && !(1..=20).contains(&get32(bytes, 72))
                && !(257..=268).contains(&get32(bytes, 72))
                && !(513..=532).contains(&get32(bytes, 72)))
            || (status == FAILED && phase != 2)
            || (status == REJECTED && phase == 2)
            || (status == CANCELLED && phase == 2)
            || (phase == 1 && flags != 0)
            || (phase == 3 && flags & 3 != 0)
            || (get64(bytes, 32) == 0 && status != REJECTED)
            || (phase == 2
                && applied
                && (flags & 1 == 0
                    || get64(bytes, 56) != get64(bytes, 48)
                    || get64(bytes, 64) != get64(bytes, 56)))
            || (get64(bytes, 48) < get64(bytes, 40))
        {
            return Err(Error::Format);
        }
        if applied && kind == EXPORT {
            if bytes[156] & !7 != 0
                || bytes[157] & !15 != 0
                || bytes[158] > 2
                || bytes[159] != 0
                || get32(bytes, 132) == 0
                || get64(bytes, 104) != get64(bytes, 64)
                || (bytes[156] & 2 != 0
                    && get64(bytes, 144)
                        .checked_mul(u64::from(get32(bytes, 152)))
                        .and_then(|n| n.checked_mul(4))
                        != Some(get64(bytes, 160)))
            {
                return Err(Error::Format);
            }
        }
        if applied && kind == INITIAL {
            let state = &bytes[DATA..DATA + INITIAL_BYTES];
            let option = |value: u8| value == 255 || value < 128;
            if state[0] >= 16
                || state[1] > 1
                || !state[2..6].iter().all(|v| option(*v))
                || state[6] > 1
                || state[7] > 2
                || state[8] > 1
                || state[9] != 0
                || u16::from_le_bytes(state[10..12].try_into().unwrap()) > 16383
                || state[12..16].iter().any(|v| *v != 0)
                || state[16..528]
                    .chunks_exact(4)
                    .any(|n| n[0] > 127 || n[1] > 3 || !option(n[2]) || n[3] != 0)
                || !state[528..].iter().all(|v| option(*v))
            {
                return Err(Error::Format);
            }
        }
        Ok(Self(bytes.try_into().unwrap()))
    }
}

#[derive(Clone, Copy, Default)]
struct Bank {
    generation: u32,
    active: bool,
    positions: [Option<u64>; 2],
    bytes: [u64; 2],
}
struct Pending {
    request: Request,
    runtime: Option<u64>,
    effective: u64,
}
pub struct Client {
    session: u64,
    last: u64,
    pending: Option<Pending>,
    banks: [Bank; BANKS],
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
            banks: [Bank {
                generation: 1,
                ..Bank::default()
            }; BANKS],
        })
    }
    pub fn outstanding(&self) -> usize {
        usize::from(self.pending.is_some())
    }
    pub fn retained(&self) -> usize {
        self.banks.iter().filter(|b| b.active).count()
    }
    pub fn generation(&self, bank: usize) -> Option<u32> {
        self.banks.get(bank).map(|b| b.generation)
    }
    pub fn prepare(&mut self, mut request: Request) -> Result<[u8; REQUEST_BYTES], Error> {
        if self.pending.is_some() {
            return Err(Error::Busy);
        }
        let bank = self.banks.get(request.bank as usize).ok_or(Error::Slot)?;
        if request.session != self.session
            || request.generation != bank.generation
            || bank.generation == 0
        {
            return Err(Error::Identity);
        }
        if (request.kind == EXPORT) == bank.active {
            return Err(Error::State);
        }
        if matches!(request.kind, AUDIO_READ | MIDI_READ)
            && bank.positions[usize::from(request.kind == MIDI_READ)] != Some(request.position)
        {
            return Err(Error::State);
        }
        request.sequence = self.last.checked_add(1).ok_or(Error::Exhausted)?;
        let bytes = request.encode()?;
        self.last = request.sequence;
        self.pending = Some(Pending {
            request,
            runtime: None,
            effective: request.at,
        });
        Ok(bytes)
    }
    pub fn receive(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let response = Response::decode(bytes)?;
        let b = &response.0;
        let pending = self.pending.as_ref().ok_or(Error::State)?;
        let request = pending.request;
        if get64(b, 16) != self.session {
            return Err(Error::Session);
        }
        if get64(b, 24) != request.sequence {
            return Err(Error::Sequence);
        }
        if b[6] != request.kind
            || get32(b, 76) != request.bank
            || get32(b, 80) != request.generation
            || get64(b, 40) != request.at
            || get64(b, 88) != request.position
        {
            return Err(Error::Identity);
        }
        let phase = b[7];
        let status = get32(b, 12);
        let runtime = get64(b, 32);
        if phase == 1 {
            if pending.runtime.is_some() {
                return Err(Error::State);
            }
            if status == ADMITTED {
                if runtime == 0 || get64(b, 48) < request.at {
                    return Err(Error::State);
                }
                let pending = self.pending.as_mut().unwrap();
                pending.runtime = Some(runtime);
                pending.effective = get64(b, 48);
                return Ok(());
            }
        } else if phase == 2 {
            if pending.runtime != Some(runtime) || get64(b, 48) != pending.effective {
                return Err(Error::State);
            }
        } else if pending.runtime.is_some() || !matches!(request.kind, AUDIO_READ | MIDI_READ) {
            return Err(Error::State);
        }
        if status == APPLIED && request.kind == INITIAL && b[DATA] != request.channel {
            return Err(Error::Identity);
        }
        let bank = &mut self.banks[request.bank as usize];
        if status == APPLIED {
            match request.kind {
                EXPORT => {
                    bank.active = true;
                    bank.bytes = [get64(b, 160), get64(b, 168)];
                    bank.positions = [None; 2];
                }
                AUDIO_CURSOR | MIDI_CURSOR => {
                    bank.positions[usize::from(request.kind == MIDI_CURSOR)] = Some(0)
                }
                AUDIO_READ | MIDI_READ => {
                    let stream = usize::from(request.kind == MIDI_READ);
                    let end = request
                        .position
                        .checked_add(u64::from(get32(b, 96)))
                        .ok_or(Error::State)?;
                    if end > bank.bytes[stream]
                        || (get32(b, 8) & 4 != 0) != (end == bank.bytes[stream])
                    {
                        return Err(Error::State);
                    }
                    bank.positions[stream] = Some(end);
                }
                RELEASE => {
                    let next = bank.generation.checked_add(1).unwrap_or(0);
                    if get32(b, 84) != next {
                        return Err(Error::Identity);
                    }
                    *bank = Bank {
                        generation: next,
                        ..Bank::default()
                    };
                }
                INITIAL => {}
                _ => unreachable!(),
            }
        }
        self.pending = None;
        Ok(())
    }
}
