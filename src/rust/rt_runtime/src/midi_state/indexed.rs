//! State reconstruction from ordered packet categories and interval summaries.

use super::{interval::*, ChannelState, MidiState, ParameterFamily};
use crate::{midi::Message, Frame};

pub(crate) mod store;
pub(crate) mod tree;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct Cut {
    point: Point,
    inclusive: bool,
}
impl Cut {
    pub(crate) fn before(point: Point) -> Self {
        Self {
            point,
            inclusive: false,
        }
    }
    pub(crate) fn at(point: Point) -> Self {
        Self {
            point,
            inclusive: true,
        }
    }
    pub(crate) fn frame(frame: Frame) -> Self {
        Self::before(Point { frame, order: 0 })
    }
    pub(crate) fn includes(self, point: Point) -> bool {
        Self::at(point) <= self
    }
    pub(crate) fn shifted(self, frames: u64) -> Option<Self> {
        Some(Self {
            point: Point {
                frame: Frame(self.point.frame.0.checked_add(frames)?),
                order: self.point.order,
            },
            inclusive: self.inclusive,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Key {
    NoteOn(u8, u8),
    NoteOff(u8, u8),
    KeyPressure(u8, u8),
    Controller(u8, u8),
    Program(u8),
    Pressure(u8),
    Pitch(u8),
    ChannelOff(u8),
    SoundOff(u8),
    ResetControllers(u8),
    Hold(u8),
    Sostenuto(u8),
    Reset,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Event {
    pub(crate) point: Point,
    data: [u8; 3],
}
#[derive(Clone, Copy)]
pub(crate) struct StatePacket([u8; 3]);
impl StatePacket {
    pub(crate) fn apply(self, state: &mut MidiState) {
        let len = match self.0[0] {
            0xff => 1,
            0xc0..=0xdf => 2,
            _ => 3,
        };
        state
            .apply_packet(&self.0[..len], 3)
            .expect("validated state packet");
    }
    pub(crate) fn at(self, point: Point) -> Event {
        Event {
            point,
            data: self.0,
        }
    }
}
impl Event {
    pub(crate) fn packet(self) -> StatePacket {
        StatePacket(self.data)
    }
    pub(crate) fn from_message(point: Point, message: Message<'_>) -> Option<Self> {
        let packet = message.data();
        if packet[0] != 0xff && !(0x80..0xf0).contains(&packet[0]) {
            return None;
        }
        let mut data = [0; 3];
        data[..packet.len()].copy_from_slice(packet);
        Some(Self { point, data })
    }
    pub(crate) fn keys(self) -> [Option<Key>; 3] {
        let [status, a, b] = self.data;
        let ch = status & 15;
        let one = |key| [Some(key), None, None];
        if status == 0xff {
            return one(Key::Reset);
        }
        match status & 0xf0 {
            0x80 => one(Key::NoteOff(ch, a)),
            0x90 if b == 0 => one(Key::NoteOff(ch, a)),
            0x90 => one(Key::NoteOn(ch, a)),
            0xa0 => one(Key::KeyPressure(ch, a)),
            0xb0 => match a {
                64 | 69 => [Some(Key::Controller(ch, a)), Some(Key::Hold(ch)), None],
                66 => [Some(Key::Controller(ch, a)), Some(Key::Sostenuto(ch)), None],
                120 => [
                    Some(Key::SoundOff(ch)),
                    Some(Key::ChannelOff(ch)),
                    Some(Key::Sostenuto(ch)),
                ],
                121 => [
                    Some(Key::ResetControllers(ch)),
                    Some(Key::Hold(ch)),
                    Some(Key::Sostenuto(ch)),
                ],
                123 => one(Key::ChannelOff(ch)),
                124..=127 => [
                    Some(Key::Controller(ch, a)),
                    Some(Key::ChannelOff(ch)),
                    None,
                ],
                _ => one(Key::Controller(ch, a)),
            },
            0xc0 => one(Key::Program(ch)),
            0xd0 => one(Key::Pressure(ch)),
            0xe0 => one(Key::Pitch(ch)),
            _ => unreachable!("state packet"),
        }
    }
    pub(crate) fn hold(self) -> Option<HoldUpdate> {
        if self.data[0] & 0xf0 != 0xb0 {
            return None;
        }
        match self.data[1] {
            64 => Some(HoldUpdate::Sustain(self.data[2] >= 64)),
            69 => Some(HoldUpdate::Hold2(self.data[2] >= 64)),
            121 => Some(HoldUpdate::Reset),
            _ => None,
        }
    }
    pub(crate) fn sostenuto(self) -> Option<SostenutoUpdate> {
        if self.data[0] & 0xf0 != 0xb0 {
            return None;
        }
        match self.data[1] {
            66 => Some(SostenutoUpdate::Pedal(self.data[2] >= 64)),
            120 => Some(SostenutoUpdate::Clear),
            121 => Some(SostenutoUpdate::Reset),
            _ => None,
        }
    }
}

pub(crate) trait Timeline {
    fn last(&self, key: Key, cut: Cut) -> Option<Event>;
    fn first(&self, key: Key, after: Cut, cut: Cut) -> Option<Event>;
    fn holds(&self, channel: u8, after: Cut, cut: Cut) -> HoldSummary;
    fn sostenuto(&self, channel: u8, after: Cut, cut: Cut) -> SostenutoSummary;
}

fn latest<const N: usize>(events: [Option<Event>; N]) -> Option<Event> {
    events.into_iter().flatten().max_by_key(|event| event.point)
}
fn earliest<const N: usize>(events: [Option<Event>; N]) -> Option<Event> {
    events.into_iter().flatten().min_by_key(|event| event.point)
}
fn reset_value(cc: u8) -> Option<u8> {
    match cc {
        1 | 64 | 65 | 66 | 67 | 69 => Some(0),
        11 | 98..=101 => Some(127),
        _ => None,
    }
}

struct Reader<'a, T> {
    timeline: &'a T,
    base: &'a MidiState,
    start: Cut,
    end: Cut,
}
impl<T: Timeline> Reader<'_, T> {
    fn at(&self, key: Key, cut: Cut) -> Option<Event> {
        self.timeline
            .last(key, cut)
            .filter(|event| !self.start.includes(event.point))
    }
    fn last(&self, key: Key) -> Option<Event> {
        self.at(key, self.end)
    }
    fn controller(&self, ch: u8, cc: u8, reset: Option<Event>) -> Option<u8> {
        if matches!(cc, 96 | 97 | 120 | 121 | 123) {
            return None;
        }
        let own = self.last(Key::Controller(ch, cc));
        let other = match cc {
            124 => self.last(Key::Controller(ch, 125)),
            125 => self.last(Key::Controller(ch, 124)),
            126 => self.last(Key::Controller(ch, 127)),
            127 => self.last(Key::Controller(ch, 126)),
            _ => None,
        };
        let reset = (!super::reset_preserves(cc)).then_some(reset).flatten();
        match latest([own, other, reset]) {
            Some(event) if Some(event) == other => None,
            Some(event) if Some(event) == reset => reset_value(cc),
            Some(event) => Some(event.data[2]),
            None => self.base.channels[ch as usize].controller(cc),
        }
    }
    fn held(&self, ch: u8, key: u8, cut: Cut) -> bool {
        let on = self.at(Key::NoteOn(ch, key), cut);
        let off = latest([
            self.at(Key::NoteOff(ch, key), cut),
            self.at(Key::ChannelOff(ch), cut),
        ]);
        match latest([on, off]) {
            Some(event) => Some(event) == on,
            None => self.base.channels[ch as usize].notes[key as usize].held(),
        }
    }
    fn latch(&self, ch: u8, key: u8, cut: Cut) -> (bool, SostenutoProjection) {
        let base = &self.base.channels[ch as usize];
        let summary = self
            .timeline
            .sostenuto(ch, self.start, cut)
            .apply(base.down(66));
        let latched = match (summary.last_rising, summary.last_clear) {
            (Some(rising), clear) if clear.is_none_or(|clear| rising > clear) => {
                self.held(ch, key, Cut::before(rising))
            }
            (_, Some(_)) => false,
            _ => base.notes[key as usize].sostenuto(),
        };
        (latched, summary)
    }
    fn velocity(&self, ch: u8, key: u8) -> u8 {
        let base = &self.base.channels[ch as usize];
        let initial = base.notes[key as usize];
        let on = self.last(Key::NoteOn(ch, key));
        let velocity = on.map_or(initial.velocity, |event| event.data[2]);
        if velocity == 0
            || self
                .last(Key::SoundOff(ch))
                .is_some_and(|off| on.is_none_or(|on| off.point > on.point))
        {
            return 0;
        }
        let after = on.map_or(self.start, |on| Cut::at(on.point));
        let off = earliest([
            self.timeline.first(Key::NoteOff(ch, key), after, self.end),
            self.timeline.first(Key::ChannelOff(ch), after, self.end),
        ]);
        let mut released = if on.is_none() && !initial.held() {
            self.start
        } else if let Some(off) = off {
            Cut::at(off.point)
        } else {
            return velocity;
        };
        let (latched, pedal) = self.latch(ch, key, released);
        if latched {
            match self
                .timeline
                .sostenuto(ch, released, self.end)
                .apply(pedal.down)
                .first_clear
            {
                Some(clear) => released = Cut::at(clear),
                None => return velocity,
            }
        }
        let initial_hold = HoldState::new(base.down(64), base.down(69));
        let holds = self
            .timeline
            .holds(ch, self.start, released)
            .apply(initial_hold)
            .state;
        if !holds.active()
            || self
                .timeline
                .holds(ch, released, self.end)
                .apply(holds)
                .clear_seen
        {
            0
        } else {
            velocity
        }
    }
    fn channel(&self, ch: u8) -> ChannelState {
        let mut result = self.base.channels[ch as usize];
        let reset = self.last(Key::ResetControllers(ch));
        for cc in 0..128 {
            result.set_controller(cc, self.controller(ch, cc, reset));
        }
        if let Some(program) = self.last(Key::Program(ch)) {
            result.program = Some(program.data[1]);
            result.program_bank = [0, 32].map(|cc| {
                self.at(Key::Controller(ch, cc), Cut::before(program.point))
                    .map_or(self.base.channels[ch as usize].controller(cc), |event| {
                        Some(event.data[2])
                    })
            });
        }
        if let Some(event) = latest([self.last(Key::Pressure(ch)), reset]) {
            result.pressure = Some(if Some(event) == reset {
                0
            } else {
                event.data[1]
            });
        }
        if let Some(event) = latest([self.last(Key::Pitch(ch)), reset]) {
            result.pitch_bend = if Some(event) == reset {
                8192
            } else {
                u16::from(event.data[1]) | (u16::from(event.data[2]) << 7)
            };
        }
        result.reset_controllers |= reset.is_some();
        if let Some(event) = latest([
            reset,
            self.last(Key::Controller(ch, 98)),
            self.last(Key::Controller(ch, 99)),
            self.last(Key::Controller(ch, 100)),
            self.last(Key::Controller(ch, 101)),
        ]) {
            result.parameter_family = Some(if matches!(event.data[1], 98 | 99) {
                ParameterFamily::Nrpn
            } else {
                ParameterFamily::Rpn
            });
        }
        result.parameter_history_untracked |= [6, 38, 96, 97]
            .into_iter()
            .any(|cc| self.last(Key::Controller(ch, cc)).is_some());
        for key in 0..128 {
            let pressure = match latest([self.last(Key::KeyPressure(ch, key)), reset]) {
                Some(event) if Some(event) == reset => 0,
                Some(event) => event.data[2],
                None => self.base.channels[ch as usize].notes[key as usize].pressure,
            };
            let held = self.held(ch, key, self.end);
            let latched = self.latch(ch, key, self.end).0;
            let note = &mut result.notes[key as usize];
            note.velocity = self.velocity(ch, key);
            note.set_held(held);
            note.set_sostenuto(latched);
            note.pressure = pressure;
            result.refresh_note(key);
        }
        result
    }
}

pub(crate) fn state<T: Timeline>(timeline: &T, initial: &MidiState, cut: Cut) -> MidiState {
    let reset = timeline.last(Key::Reset, cut);
    let mut result = if reset.is_some() {
        MidiState::default()
    } else {
        *initial
    };
    result.system_reset |= reset.is_some();
    let base = result;
    let reader = Reader {
        timeline,
        base: &base,
        start: reset.map_or(Cut::frame(Frame(0)), |reset| Cut::at(reset.point)),
        end: cut,
    };
    for channel in 0..16 {
        result.channels[channel] = reader.channel(channel as u8);
    }
    result
}

#[cfg(test)]
mod tests;
