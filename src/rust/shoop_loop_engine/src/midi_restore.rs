//! Fixed-protocol MIDI cleanup/restoration. Packets borrow stack storage only
//! for the sink call. This does not reconstruct synthesizer voices or parameters.
use rt_runtime::{
    midi_state::{ChannelState, MidiState, ParameterFamily},
    Error,
};

pub const CLEANUP_MAX_EVENTS: usize = 16 * (3 + 128);
pub const RESTORE_MAX_EVENTS: usize = 1 + 16 * (144 + 3 * 128);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Emission {
    pub attempted: usize,
    pub accepted: usize,
    pub first_error: Option<Error>,
    pub parameter_history_untracked: bool,
}
impl Emission {
    fn send(&mut self, sink: &mut impl FnMut(&[u8]) -> Result<(), Error>, packet: &[u8]) {
        self.attempted += 1;
        if self.first_error.is_none() {
            match sink(packet) {
                Ok(()) => self.accepted += 1,
                Err(error) => self.first_error = Some(error),
            }
        }
    }
}

/// Release loop-owned pedals/keys, not unrelated channel voices. On output
/// refusal the caller must invoke runtime recovery; a prefix is not success.
pub fn cleanup(state: &MidiState, mut sink: impl FnMut(&[u8]) -> Result<(), Error>) -> Emission {
    let mut result = Emission::default();
    for channel in 0..16 {
        let state = state.channel(channel).expect("MIDI channel");
        for cc in [64, 66, 69] {
            if state.controller(cc).is_some_and(|value| value >= 64) {
                result.send(&mut sink, &[0xb0 | channel, cc, 0]);
            }
        }
        for key in 0..128 {
            let note = state.note(key).expect("MIDI key");
            if note.held() || note.velocity() != 0 {
                result.send(&mut sink, &[0x80 | channel, key, 0]);
            }
        }
    }
    result
}

fn control(
    result: &mut Emission,
    sink: &mut impl FnMut(&[u8]) -> Result<(), Error>,
    state: &ChannelState,
    channel: u8,
    cc: u8,
) {
    if let Some(value) = state.controller(cc) {
        result.send(sink, &[0xb0 | channel, cc, value]);
    }
}
fn selectors(
    result: &mut Emission,
    sink: &mut impl FnMut(&[u8]) -> Result<(), Error>,
    state: &ChannelState,
    channel: u8,
    family: ParameterFamily,
) {
    let pair = match family {
        ParameterFamily::Rpn => [101, 100],
        ParameterFamily::Nrpn => [99, 98],
    };
    for cc in pair {
        control(result, sink, state, channel, cc);
    }
}

/// Establish known projected state before delivering events at the seek offset.
/// Cleanup must precede this operation. Data-entry values are emitted only while
/// selectors are null; relative parameter commands are never replayed as state.
/// The report exposes parameter history that this projection cannot reconstruct.
pub fn restore(state: &MidiState, mut sink: impl FnMut(&[u8]) -> Result<(), Error>) -> Emission {
    let mut result = Emission::default();
    if state.system_reset_seen() {
        result.send(&mut sink, &[0xff]);
    }
    for channel in 0..16 {
        let state = state.channel(channel).expect("MIDI channel");
        result.parameter_history_untracked |= state.parameter_history_untracked();
        if state.reset_controllers_seen() {
            result.send(&mut sink, &[0xb0 | channel, 121, 0]);
        }
        let bank = state.program_bank();
        if let Some(program) = state.program() {
            for (cc, value) in [0, 32].into_iter().zip(bank) {
                if let Some(value) = value {
                    result.send(&mut sink, &[0xb0 | channel, cc, value]);
                }
            }
            result.send(&mut sink, &[0xc0 | channel, program]);
        }
        for (cc, selected) in [0, 32].into_iter().zip(bank) {
            if state.program().is_none() || state.controller(cc) != selected {
                control(&mut result, &mut sink, state, channel, cc);
            }
        }
        for cc in 0..128 {
            if !matches!(
                cc,
                0 | 6 | 32 | 38 | 64 | 66 | 69 | 96..=101 | 120 | 121 | 123
            ) {
                control(&mut result, &mut sink, state, channel, cc);
            }
        }
        if state.controller(6).is_some() || state.controller(38).is_some() {
            for cc in [101, 100, 99, 98] {
                result.send(&mut sink, &[0xb0 | channel, cc, 127]);
            }
            for cc in [6, 38] {
                control(&mut result, &mut sink, state, channel, cc);
            }
        }
        let families = match state.parameter_family() {
            Some(ParameterFamily::Nrpn) => [ParameterFamily::Rpn, ParameterFamily::Nrpn],
            _ => [ParameterFamily::Nrpn, ParameterFamily::Rpn],
        };
        for family in families {
            selectors(&mut result, &mut sink, state, channel, family);
        }
        let pitch = state.pitch_bend();
        result.send(
            &mut sink,
            &[0xe0 | channel, (pitch & 127) as u8, (pitch >> 7) as u8],
        );
        if let Some(pressure) = state.pressure() {
            result.send(&mut sink, &[0xd0 | channel, pressure]);
        }
        for cc in [64, 69] {
            control(&mut result, &mut sink, state, channel, cc);
        }
        let sostenuto = state.controller(66).unwrap_or(0);
        result.send(
            &mut sink,
            &[
                0xb0 | channel,
                66,
                if sostenuto >= 64 { 0 } else { sostenuto },
            ],
        );
        if sostenuto >= 64 {
            for key in 0..128 {
                let note = state.note(key).expect("MIDI key");
                if note.sostenuto() && note.velocity() != 0 {
                    result.send(&mut sink, &[0x90 | channel, key, note.velocity()]);
                }
            }
            result.send(&mut sink, &[0xb0 | channel, 66, sostenuto]);
        }
        for key in 0..128 {
            let note = state.note(key).expect("MIDI key");
            if note.velocity() != 0 && !(sostenuto >= 64 && note.sostenuto()) {
                result.send(&mut sink, &[0x90 | channel, key, note.velocity()]);
            }
        }
        for key in 0..128 {
            let note = state.note(key).expect("MIDI key");
            if note.velocity() != 0 && !note.held() {
                result.send(&mut sink, &[0x80 | channel, key, 0]);
            }
            if let Some(pressure) = note.pressure() {
                result.send(&mut sink, &[0xa0 | channel, key, pressure]);
            }
        }
    }
    result
}
