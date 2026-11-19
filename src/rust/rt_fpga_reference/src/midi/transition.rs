use super::{Differences, Error, Message, MidiState};

#[derive(Clone, Copy, Debug)]
pub struct StateTransition {
    mask: u16,
    channel: u8,
    phase: u8,
    cursor: u16,
    rebase: bool,
    stop: bool,
    steps: u64,
}
impl StateTransition {
    pub fn restore(channels: u16) -> Self {
        Self {
            mask: channels,
            channel: 0,
            phase: 0,
            cursor: 0,
            rebase: false,
            stop: false,
            steps: 0,
        }
    }
    pub fn cleanup(channels: u16) -> Self {
        Self {
            stop: true,
            ..Self::restore(channels)
        }
    }
    pub fn complete(&self) -> bool {
        self.phase == 20
    }
    pub fn steps(&self) -> u64 {
        self.steps
    }
    pub fn step(
        &mut self,
        target: &MidiState,
        current: &MidiState,
        differences: &Differences,
        mut sink: impl FnMut(Message) -> Result<(), Error>,
    ) -> Result<bool, Error> {
        if self.complete() {
            return Ok(true);
        }
        let mut next = *self;
        let message = next.advance(target, current, differences);
        if let Some(message) = message {
            sink(message)?;
        }
        next.steps = next.steps.saturating_add(1);
        *self = next;
        Ok(self.complete())
    }
    pub(super) fn propose(
        &self,
        target: &MidiState,
        current: &MidiState,
        differences: &Differences,
    ) -> (Self, Option<Message>) {
        let mut next = *self;
        let message = if self.complete() {
            None
        } else {
            next.advance(target, current, differences)
        };
        next.steps = next.steps.saturating_add(1);
        (next, message)
    }
    fn advance(
        &mut self,
        target: &MidiState,
        current: &MidiState,
        diff: &Differences,
    ) -> Option<Message> {
        if self.phase == 0 {
            while self.channel < 16 {
                self.steps = self.steps.saturating_add(1);
                let base = usize::from(self.channel) * 264;
                let c = current.channel(self.channel).unwrap();
                let has_diff = diff.next(base).is_some_and(|cell| cell < base + 264);
                let active = c.has_activity()
                    || [64, 66, 69]
                        .iter()
                        .any(|&cc| c.controller(cc).is_some_and(|v| v >= 64));
                if self.mask & (1 << self.channel) != 0 && if self.stop { active } else { has_diff }
                {
                    break;
                }
                self.channel += 1;
            }
            if self.channel == 16 {
                self.phase = 20;
                return None;
            }
            let base = usize::from(self.channel) * 264;
            let t = target.channel(self.channel).unwrap();
            let c = current.channel(self.channel).unwrap();
            self.rebase = self.stop
                || [64, 66, 69, 124, 125, 126, 127]
                    .iter()
                    .any(|&cc| diff.contains(base + 128 + cc));
            let mut cursor = base;
            while let Some(cell) = diff.next(cursor).filter(|cell| *cell < base + 128) {
                self.steps = self.steps.saturating_add(1);
                let key = (cell - base) as u8;
                let a = t.note(key).unwrap();
                let b = c.note(key).unwrap();
                self.rebase |= (a.velocity(), a.held(), a.sostenuto())
                    != (b.velocity(), b.held(), b.sostenuto());
                cursor = cell + 1;
            }
            self.phase = if self.stop { 9 } else { 1 };
            self.cursor = 0;
            return None;
        }
        let ch = self.channel;
        let t = target.channel(ch).unwrap();
        let c = current.channel(ch).unwrap();
        let base = usize::from(ch) * 264;
        let packet = |bytes: &[u8]| Some(Message::managed(bytes).unwrap());
        let cc = |number: u8, value: u8| packet(&[0xb0 | ch, number, value]);
        let program_dirty = t.program().is_some_and(|p| {
            c.program() != Some(p)
                || t.program_bank()
                    .iter()
                    .zip(c.program_bank())
                    .any(|(a, b)| a.is_some() && *a != b)
        });
        let phase = self.phase;
        match phase {
            1 | 2 => {
                self.phase += 1;
                let i = usize::from(phase - 1);
                if program_dirty {
                    if let Some(v) = t.program_bank()[i] {
                        let number = [0, 32][i];
                        if c.controller(number) != Some(v) {
                            return cc(number, v);
                        }
                    }
                }
            }
            3 => {
                self.phase = 4;
                if program_dirty {
                    return packet(&[0xc0 | ch, t.program().unwrap()]);
                }
            }
            4 | 5 => {
                self.phase += 1;
                let number = if phase == 4 { 0 } else { 32 };
                if let Some(v) = t.controller(number) {
                    if c.controller(number) != Some(v) {
                        return cc(number, v);
                    }
                }
            }
            6 => {
                if let Some(cell) = diff.next(base + 128 + usize::from(self.cursor)) {
                    if cell < base + 256 {
                        let number = (cell - base - 128) as u8;
                        self.cursor = u16::from(number) + 1;
                        if !matches!(number, 0 | 32 | 64 | 66 | 69) {
                            if let Some(v) = t.controller(number) {
                                return cc(number, v);
                            }
                        }
                        return None;
                    }
                }
                self.phase = 7;
                self.cursor = 0;
            }
            7 => {
                self.phase = 8;
                if t.pitch_bend() != c.pitch_bend() {
                    return packet(&[
                        0xe0 | ch,
                        (t.pitch_bend() & 127) as u8,
                        (t.pitch_bend() >> 7) as u8,
                    ]);
                }
            }
            8 => {
                self.phase = 9;
                if let Some(v) = t.pressure() {
                    if c.pressure() != Some(v) {
                        return packet(&[0xd0 | ch, v]);
                    }
                }
            }
            9 => {
                if self.rebase && self.cursor < 3 {
                    let number = [64, 66, 69][usize::from(self.cursor)];
                    self.cursor += 1;
                    if c.controller(number).is_some_and(|v| v >= 64) {
                        return cc(number, 0);
                    }
                } else {
                    self.phase = 10;
                    self.cursor = 0;
                }
            }
            10 => {
                if self.rebase {
                    if let Some((key, n)) = c.next_projected(self.cursor) {
                        self.cursor = u16::from(key) + 1;
                        if n.held() || n.velocity() != 0 {
                            return packet(&[0x80 | ch, key, 0]);
                        }
                        return None;
                    }
                }
                self.phase = if self.stop { 19 } else { 11 };
                self.cursor = 0;
            }
            11 | 12 => {
                self.phase += 1;
                let number = if phase == 11 { 64 } else { 69 };
                if let Some(v) = t.controller(number) {
                    if c.controller(number) != Some(v) {
                        return cc(number, v);
                    }
                }
            }
            13 => {
                self.phase = 14;
                if self.rebase {
                    let value = t.controller(66).filter(|v| *v < 64).unwrap_or(0);
                    if c.controller(66) != Some(value) {
                        return cc(66, value);
                    }
                }
            }
            14 | 16 | 17 | 18 => {
                if !self.rebase && phase != 18 {
                    self.phase += 1;
                    self.cursor = 0;
                    return None;
                }
                let sostenuto = t.controller(66).is_some_and(|v| v >= 64);
                let next_note = if phase == 18 {
                    diff.next(base + usize::from(self.cursor))
                        .filter(|cell| *cell < base + 128)
                        .map(|cell| {
                            let key = (cell - base) as u8;
                            (key, t.note(key).unwrap())
                        })
                } else {
                    t.next_projected(self.cursor)
                };
                if let Some((key, n)) = next_note {
                    self.cursor = u16::from(key) + 1;
                    let candidate = match phase {
                        14 => self.rebase && sostenuto && n.sostenuto() && n.velocity() != 0,
                        16 => self.rebase && n.velocity() != 0 && !(sostenuto && n.sostenuto()),
                        17 => self.rebase && n.velocity() != 0 && !n.held(),
                        18 => n
                            .pressure()
                            .is_some_and(|p| c.note(key).unwrap().pressure() != Some(p)),
                        _ => false,
                    };
                    if candidate {
                        return match phase {
                            17 => packet(&[0x80 | ch, key, 0]),
                            18 => packet(&[0xa0 | ch, key, n.pressure().unwrap()]),
                            _ => packet(&[0x90 | ch, key, n.velocity()]),
                        };
                    }
                    return None;
                }
                self.phase += 1;
                self.cursor = 0;
            }
            15 => {
                self.phase = 16;
                if let Some(v) = t.controller(66) {
                    if c.controller(66) != Some(v) {
                        return cc(66, v);
                    }
                }
            }
            19 => {
                self.channel += 1;
                self.phase = 0;
                self.cursor = 0;
            }
            _ => unreachable!(),
        }
        None
    }
}
