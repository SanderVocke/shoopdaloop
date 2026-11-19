use super::{Error, Message, MidiState, Work};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EmissionKind {
    Cleanup,
    Restore,
    Recovery,
}

/// Each step examines one field, or retries the same unacknowledged message.
pub struct Emission {
    kind: EmissionKind,
    next: u32,
}
impl Emission {
    pub fn new(kind: EmissionKind) -> Self {
        Self { kind, next: 0 }
    }
    pub fn complete(&self) -> bool {
        self.next
            == match self.kind {
                EmissionKind::Cleanup => 16 * 131,
                EmissionKind::Restore => 1 + 16 * 652,
                EmissionKind::Recovery => 32,
            }
    }
    pub fn step(
        &mut self,
        state: &MidiState,
        work: &mut Work,
        sink: &mut impl FnMut(Message) -> Result<(), Error>,
    ) -> Result<bool, Error> {
        if self.complete() {
            return Ok(true);
        }
        work.emission_steps = work.emission_steps.saturating_add(1);
        let message = match self.kind {
            EmissionKind::Recovery => Some(Message::managed(&[
                0xb0 | (self.next / 2) as u8,
                if self.next % 2 == 0 { 120 } else { 123 },
                0,
            ])?),
            EmissionKind::Cleanup => {
                let ch = (self.next / 131) as u8;
                let cell = self.next % 131;
                let channel = state.channel(ch).unwrap();
                if cell < 3 {
                    let cc = [64, 66, 69][cell as usize];
                    channel
                        .controller(cc)
                        .filter(|&v| v >= 64)
                        .map(|_| Message::managed(&[0xb0 | ch, cc, 0]).unwrap())
                } else {
                    let key = (cell - 3) as u8;
                    let note = channel.note(key).unwrap();
                    (note.held() || note.velocity() != 0)
                        .then(|| Message::managed(&[0x80 | ch, key, 0]).unwrap())
                }
            }
            EmissionKind::Restore => self.restore(state),
        };
        if let Some(message) = message {
            sink(message)?;
            work.accepted = work.accepted.saturating_add(1);
        }
        self.next += 1;
        Ok(self.complete())
    }
    fn restore(&self, state: &MidiState) -> Option<Message> {
        if self.next == 0 {
            return state
                .system_reset_seen()
                .then(|| Message::managed(&[0xff]).unwrap());
        }
        let ch = ((self.next - 1) / 652) as u8;
        let cell = (self.next - 1) % 652;
        let s = state.channel(ch).unwrap();
        let cc = |controller: u8, value: Option<u8>| {
            value.map(|v| Message::managed(&[0xb0 | ch, controller, v]).unwrap())
        };
        let note_on =
            |key: u8| Message::managed(&[0x90 | ch, key, s.note(key).unwrap().velocity()]).unwrap();
        match cell {
            0 => s
                .reset_controllers_seen()
                .then(|| Message::managed(&[0xb0 | ch, 121, 0]).unwrap()),
            1..=2 => s.program().and_then(|_| {
                cc(
                    if cell == 1 { 0 } else { 32 },
                    s.program_bank()[(cell - 1) as usize],
                )
            }),
            3 => s
                .program()
                .map(|p| Message::managed(&[0xc0 | ch, p]).unwrap()),
            4..=5 => {
                let index = (cell - 4) as usize;
                let controller = [0, 32][index];
                if s.program().is_none() || s.controller(controller) != s.program_bank()[index] {
                    cc(controller, s.controller(controller))
                } else {
                    None
                }
            }
            6..=133 => {
                let controller = (cell - 6) as u8;
                if matches!(
                    controller,
                    0 | 6 | 32 | 38 | 64 | 66 | 69 | 96..=101 | 120 | 121 | 123
                ) {
                    None
                } else {
                    cc(controller, s.controller(controller))
                }
            }
            134 => Some(
                Message::managed(&[
                    0xe0 | ch,
                    (s.pitch_bend() & 127) as u8,
                    (s.pitch_bend() >> 7) as u8,
                ])
                .unwrap(),
            ),
            135 => s
                .pressure()
                .map(|p| Message::managed(&[0xd0 | ch, p]).unwrap()),
            136..=137 => {
                let c = [64, 69][(cell - 136) as usize];
                cc(c, s.controller(c))
            }
            138 => cc(66, Some(s.controller(66).filter(|&v| v < 64).unwrap_or(0))),
            139..=266 => {
                let key = (cell - 139) as u8;
                let note = s.note(key).unwrap();
                (s.controller(66).is_some_and(|v| v >= 64)
                    && note.sostenuto()
                    && note.velocity() != 0)
                    .then(|| note_on(key))
            }
            267 => cc(66, s.controller(66).filter(|&v| v >= 64)),
            268..=395 => {
                let key = (cell - 268) as u8;
                let note = s.note(key).unwrap();
                (note.velocity() != 0
                    && !(s.controller(66).is_some_and(|v| v >= 64) && note.sostenuto()))
                .then(|| note_on(key))
            }
            396..=523 => {
                let key = (cell - 396) as u8;
                let note = s.note(key).unwrap();
                (note.velocity() != 0 && !note.held())
                    .then(|| Message::managed(&[0x80 | ch, key, 0]).unwrap())
            }
            524..=651 => {
                let key = (cell - 524) as u8;
                s.note(key)
                    .unwrap()
                    .pressure()
                    .map(|p| Message::managed(&[0xa0 | ch, key, p]).unwrap())
            }
            _ => unreachable!(),
        }
    }
}
