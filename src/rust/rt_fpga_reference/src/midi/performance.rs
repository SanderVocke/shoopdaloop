//! Fixed-index musical expression state and incremental difference words.
use super::{Error, Message};
use alloc::{boxed::Box, vec};

pub const CHANNELS: usize = 16;
pub const STRIDE: usize = 512;
pub const FIELDS: usize = CHANNELS * STRIDE;
pub const PITCH: usize = 256;
pub const PRESSURE: usize = 257;
pub const KNOWN: u32 = 1 << 31;
pub const KEY: u32 = 1 << 7;
pub const SOSTENUTO: u32 = 1 << 8;
pub const SUSTAIN: u32 = 1 << 9;
pub const POLY_KNOWN: u32 = 1 << 23;
pub const NOTE_CORE: u32 = 127 | KEY | SOSTENUTO | SUSTAIN;
pub const DEFAULT_CONTROLLERS: u128 =
    (1 << 1) | (1 << 2) | (1 << 4) | (1 << 11) | (1 << 64) | (1 << 66) | (1 << 67) | (1 << 74);

pub fn ordinary_controller(cc: u8) -> bool {
    matches!(cc, 1..=5 | 7..=31 | 64..=68 | 70..=95 | 102..=119)
}
pub fn matches(target: u32, current: u32, field: usize) -> bool {
    if target & KNOWN == 0 {
        return true;
    }
    if current & KNOWN == 0 {
        return false;
    }
    if field % STRIDE < 128 {
        target & NOTE_CORE == current & NOTE_CORE
            && (target & 127 == 0
                || target & POLY_KNOWN == 0
                || current & POLY_KNOWN != 0 && target & (127 << 16) == current & (127 << 16))
    } else {
        target == current
    }
}

#[derive(Clone)]
pub struct State {
    pub channels: u16,
    words: Box<[u32]>,
}
impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}
impl State {
    pub fn new() -> Self {
        Self {
            channels: 0,
            words: vec![0; FIELDS].into_boxed_slice(),
        }
    }
    pub fn word(&self, field: usize) -> u32 {
        if self.channels & (1 << (field / STRIDE)) == 0 {
            0
        } else {
            self.words[field]
        }
    }
    pub fn set_word(&mut self, field: usize, value: u32) -> Result<(), Error> {
        if field >= FIELDS || field % STRIDE > PRESSURE {
            return Err(Error::InvalidState);
        }
        self.channels |= 1 << (field / STRIDE);
        self.words[field] = value;
        Ok(())
    }
    pub fn copy_from(&mut self, source: &Self, channels: u16) {
        self.words.copy_from_slice(&source.words);
        self.channels = source.channels & channels;
    }
    pub fn clear(&mut self) {
        self.channels = 0;
        self.words.fill(0);
    }
    pub fn known_reset(&mut self, channels: u16) {
        self.clear();
        for ch in 0..CHANNELS {
            if channels & (1 << ch) == 0 {
                continue;
            }
            let base = ch * STRIDE;
            for key in 0..128 {
                self.words[base + key] = KNOWN;
            }
            for cc in [1, 64, 65, 66, 67] {
                self.words[base + 128 + cc] = KNOWN;
            }
            self.words[base + 128 + 11] = KNOWN | 127;
            self.words[base + PITCH] = KNOWN | 8192;
            self.words[base + PRESSURE] = KNOWN;
        }
        self.channels = channels;
    }
    fn write(&mut self, field: usize, value: u32, changed: &mut Bits) {
        if self.word(field) != value {
            self.channels |= 1 << (field / STRIDE);
            self.words[field] = value;
            changed.set(field, true);
        }
    }
    fn release(&mut self, base: usize, key: usize, changed: &mut Bits) {
        let old = self.word(base + key);
        let mut value = old & !KEY;
        if old & 127 != 0 && self.word(base + 192) & 127 >= 64 {
            value |= SUSTAIN;
        }
        if value & (SUSTAIN | SOSTENUTO) == 0 {
            value = 0;
        }
        self.write(base + key, KNOWN | value, changed);
    }
    fn pedal(&mut self, base: usize, cc: usize, old: u32, value: u32, changed: &mut Bits) {
        if (old & 127 >= 64) == (value >= 64) {
            return;
        }
        for key in 0..128 {
            let mut note = self.word(base + key);
            if note & KNOWN == 0 || note & 127 == 0 {
                continue;
            }
            if cc == 64 {
                if value >= 64 && note & KEY == 0 {
                    note |= SUSTAIN;
                }
                if value < 64 {
                    note &= !SUSTAIN;
                }
            } else if value >= 64 {
                if note & KEY != 0 {
                    note |= SOSTENUTO;
                }
            } else {
                note &= !SOSTENUTO;
            }
            if note & (KEY | SUSTAIN | SOSTENUTO) == 0 {
                note = KNOWN;
            }
            self.write(base + key, note, changed);
        }
    }
    pub fn apply(&mut self, message: Message) -> Bits {
        let mut changed = Bits::new();
        let bytes = message.as_bytes();
        let status = bytes[0];
        if status == 0xff {
            for ch in 0..16 {
                self.reset_channel(ch * STRIDE, true, &mut changed);
            }
            return changed;
        }
        if status >= 0xf0 {
            return changed;
        }
        let base = (status as usize & 15) * STRIDE;
        let a = usize::from(bytes[1]);
        match status & 0xf0 {
            0x80 => self.release(base, a, &mut changed),
            0x90 if bytes[2] == 0 => self.release(base, a, &mut changed),
            0x90 => {
                let retained = self.word(base + a) & SOSTENUTO;
                self.write(
                    base + a,
                    KNOWN | KEY | retained | POLY_KNOWN | u32::from(bytes[2]),
                    &mut changed,
                );
            }
            0xa0 => {
                let note = self.word(base + a);
                if note & 127 != 0 {
                    self.write(
                        base + a,
                        (note & !(127 << 16)) | POLY_KNOWN | (u32::from(bytes[2]) << 16),
                        &mut changed,
                    );
                }
            }
            0xb0 => {
                let value = u32::from(bytes[2]);
                match a {
                    120 => {
                        for key in 0..128 {
                            self.write(base + key, KNOWN, &mut changed);
                        }
                    }
                    121 => self.reset_channel(base, false, &mut changed),
                    123..=127 => {
                        for key in 0..128 {
                            if self.word(base + key) & KNOWN != 0 {
                                self.release(base, key, &mut changed);
                            }
                        }
                    }
                    _ if ordinary_controller(a as u8) => {
                        let old = self.word(base + 128 + a);
                        self.write(base + 128 + a, KNOWN | value, &mut changed);
                        if a == 64 || a == 66 {
                            self.pedal(base, a, old, value, &mut changed);
                        }
                    }
                    _ => {}
                }
            }
            0xd0 => self.write(base + PRESSURE, KNOWN | a as u32, &mut changed),
            0xe0 => self.write(
                base + PITCH,
                KNOWN | a as u32 | (u32::from(bytes[2]) << 7),
                &mut changed,
            ),
            _ => {}
        }
        changed
    }
    fn reset_channel(&mut self, base: usize, sound_off: bool, changed: &mut Bits) {
        for cc in 0..128 {
            if !ordinary_controller(cc as u8) {
                continue;
            }
            if !sound_off && matches!(cc, 7 | 10 | 91..=95) {
                continue;
            }
            let value = match cc {
                1 | 64..=67 => KNOWN,
                11 => KNOWN | 127,
                _ => 0,
            };
            self.write(base + 128 + cc, value, changed);
        }
        self.write(base + PITCH, KNOWN | 8192, changed);
        self.write(base + PRESSURE, KNOWN, changed);
        for key in 0..128 {
            let note = self.word(base + key);
            let next = if !sound_off && note & KEY != 0 {
                note & !(SUSTAIN | SOSTENUTO | (127 << 16)) | POLY_KNOWN
            } else if sound_off || note & KNOWN != 0 {
                KNOWN
            } else {
                0
            };
            self.write(base + key, next, changed);
        }
    }
}

#[derive(Clone)]
pub struct Bits {
    words: [u64; FIELDS / 64],
    summary: u128,
    pub count: u16,
}
impl Default for Bits {
    fn default() -> Self {
        Self::new()
    }
}
impl Bits {
    pub const fn new() -> Self {
        Self {
            words: [0; FIELDS / 64],
            summary: 0,
            count: 0,
        }
    }
    pub fn set(&mut self, field: usize, value: bool) {
        let word = field / 64;
        let mask = 1u64 << (field % 64);
        let before = self.words[word];
        if (before & mask != 0) == value {
            return;
        }
        self.count = if value {
            self.count + 1
        } else {
            self.count - 1
        };
        self.words[word] = if value { before | mask } else { before & !mask };
        if self.words[word] == 0 {
            self.summary &= !(1u128 << word);
        } else {
            self.summary |= 1u128 << word;
        }
    }
    pub fn contains(&self, field: usize) -> bool {
        self.words[field / 64] & (1 << (field % 64)) != 0
    }
    pub fn next(&self, start: usize, end: usize) -> Option<usize> {
        if start >= end {
            return None;
        }
        let first = start / 64;
        let mut words = self.summary & (!0u128 << first);
        while words != 0 {
            let word = words.trailing_zeros() as usize;
            let bits = self.words[word]
                & if word == first {
                    !0u64 << (start % 64)
                } else {
                    !0
                };
            if bits != 0 {
                let field = word * 64 + bits.trailing_zeros() as usize;
                return (field < end).then_some(field);
            }
            words &= words - 1;
        }
        None
    }
    pub fn clear(&mut self) {
        self.words.fill(0);
        self.summary = 0;
        self.count = 0;
    }
}

pub trait RestoreIo {
    fn current(&self, field: usize) -> u32;
    fn next_dirty(&self, start: usize, end: usize) -> Option<usize>;
    fn emit(&mut self, message: Message) -> Result<(), Error>;
}
fn send(io: &mut impl RestoreIo, bytes: &[u8]) -> Result<(), Error> {
    io.emit(Message::parse(bytes)?)
}
fn value_message(channel: u8, field: usize, value: u32) -> Message {
    let value = value & !KNOWN;
    if field == PITCH {
        Message::parse(&[0xe0 | channel, (value & 127) as u8, (value >> 7) as u8]).unwrap()
    } else if field == PRESSURE {
        Message::parse(&[0xd0 | channel, value as u8]).unwrap()
    } else {
        Message::parse(&[0xb0 | channel, (field - 128) as u8, value as u8]).unwrap()
    }
}

pub fn restore(
    target: &State,
    channels: u16,
    controllers: u128,
    io: &mut impl RestoreIo,
) -> Result<(), Error> {
    let controllers = controllers | (1 << 64) | (1 << 66);
    for channel in 0..16 {
        if channels & (1 << channel) == 0 {
            continue;
        }
        let base = channel * STRIDE;
        if io.next_dirty(base, base + STRIDE).is_none() {
            continue;
        }
        let mut repair_notes = false;
        let mut at = base;
        while let Some(field) = io.next_dirty(at, base + 128) {
            let desired = target.word(field);
            let current = io.current(field);
            if desired & NOTE_CORE != current & NOTE_CORE {
                repair_notes |= (desired | current) & (SUSTAIN | SOSTENUTO) != 0
                    || [64, 66].iter().any(|cc| {
                        (target.word(base + 128 + cc) | io.current(base + 128 + cc)) & 127 >= 64
                    });
            }
            at = field + 1;
        }
        let mut pedal_edge = false;
        for cc in [64, 66] {
            let desired = target.word(base + 128 + cc);
            pedal_edge |= desired & KNOWN != 0
                && (desired & 127 >= 64) != (io.current(base + 128 + cc) & 127 >= 64);
        }
        if pedal_edge {
            repair_notes |=
                (0..128).any(|key| (target.word(base + key) | io.current(base + key)) & 127 != 0);
        }
        let mut desired = [0u32; PRESSURE + 1];
        if repair_notes {
            for (f, word) in desired.iter_mut().enumerate() {
                let t = target.word(base + f);
                *word = if t & KNOWN != 0 {
                    t
                } else {
                    io.current(base + f)
                };
            }
            for cc in [64, 66] {
                if io.current(base + 128 + cc) & 127 != 0 {
                    send(io, &[0xb0 | channel as u8, cc as u8, 0])?;
                }
            }
            for key in 0..128 {
                if io.current(base + key) & 127 != 0 {
                    send(io, &[0x80 | channel as u8, key as u8, 0])?;
                }
            }
        } else {
            at = base;
            while let Some(field) = io.next_dirty(at, base + 128) {
                if (target.word(field) & NOTE_CORE != io.current(field) & NOTE_CORE
                    && io.current(field) & 127 != 0)
                    || target.word(field) & KNOWN != 0
                        && target.word(field) & 127 == 0
                        && io.current(field) & KNOWN == 0
                {
                    send(io, &[0x80 | channel as u8, (field - base) as u8, 0])?;
                }
                at = field + 1;
            }
        }
        let mut field = 128;
        while field <= PRESSURE {
            if !repair_notes {
                let Some(next) = io.next_dirty(base + field, base + PRESSURE + 1) else {
                    break;
                };
                field = next - base;
            }
            if field < 256 && (controllers & (1 << (field - 128)) == 0 || field == 194) {
                field += 1;
                continue;
            }
            let word = if repair_notes {
                desired[field]
            } else {
                target.word(base + field)
            };
            if word & KNOWN != 0 && !matches(word, io.current(base + field), field) {
                io.emit(value_message(channel as u8, field, word))?;
            }
            field += 1;
        }
        if repair_notes {
            for (key, &note) in desired[..128].iter().enumerate() {
                if note & 127 != 0 && note & SOSTENUTO != 0 {
                    send(io, &[0x90 | channel as u8, key as u8, (note & 127) as u8])?;
                }
            }
        }
        let sost = if repair_notes {
            desired[194]
        } else {
            target.word(base + 194)
        };
        if sost & KNOWN != 0 && !matches(sost, io.current(base + 194), 194) {
            io.emit(value_message(channel as u8, 194, sost))?;
        }
        if repair_notes {
            for (key, &note) in desired[..128].iter().enumerate() {
                if note & 127 != 0 && note & SOSTENUTO == 0 {
                    send(io, &[0x90 | channel as u8, key as u8, (note & 127) as u8])?;
                }
            }
            for (key, &note) in desired[..128].iter().enumerate() {
                if note & 127 != 0 && note & KEY == 0 {
                    send(io, &[0x80 | channel as u8, key as u8, 0])?;
                }
            }
        } else {
            at = base;
            while let Some(field) = io.next_dirty(at, base + 128) {
                let note = target.word(field);
                if note & 127 != 0 && note & NOTE_CORE != io.current(field) & NOTE_CORE {
                    send(
                        io,
                        &[
                            0x90 | channel as u8,
                            (field - base) as u8,
                            (note & 127) as u8,
                        ],
                    )?;
                }
                at = field + 1;
            }
        }
        let mut key = 0;
        while key < 128 {
            if !repair_notes {
                let Some(next) = io.next_dirty(base + key, base + 128) else {
                    break;
                };
                key = next - base;
            }
            let note = if repair_notes {
                desired[key]
            } else {
                target.word(base + key)
            };
            let current = io.current(base + key);
            if note & POLY_KNOWN != 0
                && note & 127 != 0
                && (current & POLY_KNOWN == 0 || note & (127 << 16) != current & (127 << 16))
            {
                send(
                    io,
                    &[0xa0 | channel as u8, key as u8, ((note >> 16) & 127) as u8],
                )?;
            }
            key += 1;
        }
    }
    Ok(())
}
