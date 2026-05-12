/// Event types for channel-based communication between trackers and diff trackers
pub const EVENT_NOTE: u8 = 0;
pub const EVENT_CC: u8 = 1;
pub const EVENT_PROGRAM: u8 = 2;
pub const EVENT_PITCH: u8 = 3;
pub const EVENT_PRESSURE: u8 = 4;

/// Event sent through channel from tracker to diff tracker
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Event {
    /// ID of the tracker that generated this event
    pub tracker_id: u64,
    /// Type of event (EVENT_NOTE, EVENT_CC, EVENT_PROGRAM, EVENT_PITCH, EVENT_PRESSURE)
    pub event_type: u8,
    /// MIDI channel (0-15)
    pub channel: u8,
    /// First parameter (note number for note events, CC number for CC events)
    pub param1: u8,
    /// Second parameter (velocity for note events, value for CC events)
    pub param2: u8,
}

impl Event {
    pub fn note(tracker_id: u64, channel: u8, note: u8, velocity: u8) -> Self {
        Self {
            tracker_id,
            event_type: EVENT_NOTE,
            channel,
            param1: note,
            param2: velocity,
        }
    }

    pub fn note_off(tracker_id: u64, channel: u8, note: u8) -> Self {
        Self {
            tracker_id,
            event_type: EVENT_NOTE,
            channel,
            param1: note,
            param2: 0, // velocity 0 means note off
        }
    }

    pub fn cc(tracker_id: u64, channel: u8, cc: u8, value: u8) -> Self {
        Self {
            tracker_id,
            event_type: EVENT_CC,
            channel,
            param1: cc,
            param2: value,
        }
    }

    pub fn program(tracker_id: u64, channel: u8, program: u8) -> Self {
        Self {
            tracker_id,
            event_type: EVENT_PROGRAM,
            channel,
            param1: program,
            param2: 0,
        }
    }

    pub fn pitch(tracker_id: u64, channel: u8, value: u16) -> Self {
        Self {
            tracker_id,
            event_type: EVENT_PITCH,
            channel,
            param1: (value & 0x7F) as u8,
            param2: ((value >> 7) & 0x7F) as u8,
        }
    }

    pub fn pressure(tracker_id: u64, channel: u8, value: u8) -> Self {
        Self {
            tracker_id,
            event_type: EVENT_PRESSURE,
            channel,
            param1: value,
            param2: 0,
        }
    }
}

/// Shared immutable view of tracker state for diff trackers
#[derive(Clone, Debug)]
pub struct TrackerStateData {
    /// 16 * 128 entries for note velocities (0x80 = inactive)
    pub notes_active: Vec<u8>,
    /// 16 * 128 entries for CC values (0x80 = unknown)
    pub controls: Vec<u8>,
    /// 16 entries for program values (0x80 = unknown)
    pub programs: Vec<u8>,
    /// 16 entries for pitch wheel values (0x8000 = unknown)
    pub pitch_wheel: Vec<u16>,
    /// 16 entries for channel pressure values (0x80 = unknown)
    pub channel_pressure: Vec<u8>,
}

impl TrackerStateData {
    pub fn new(track_notes: bool, track_controls: bool, track_programs: bool) -> Self {
        Self {
            notes_active: if track_notes { vec![0x80; 16 * 128] } else { vec![] },
            controls: if track_controls { vec![0x80; 16 * 128] } else { vec![] },
            programs: if track_programs { vec![0x80; 16] } else { vec![] },
            pitch_wheel: if track_controls { vec![0x8000; 16] } else { vec![] },
            channel_pressure: if track_controls { vec![0x80; 16] } else { vec![] },
        }
    }

    pub fn tracking_notes(&self) -> bool {
        !self.notes_active.is_empty()
    }

    pub fn tracking_controls(&self) -> bool {
        !self.controls.is_empty()
    }

    pub fn tracking_programs(&self) -> bool {
        !self.programs.is_empty()
    }

    #[inline]
    pub fn note_index(channel: u8, note: u8) -> usize {
        ((channel & 0x0F) as usize) * 128 + (note as usize)
    }

    #[inline]
    pub fn cc_index(channel: u8, cc: u8) -> usize {
        ((channel & 0x0F) as usize) * 128 + (cc as usize)
    }

    pub fn maybe_current_note_velocity(&self, channel: u8, note: u8) -> u8 {
        self.notes_active[Self::note_index(channel, note)]
    }

    pub fn maybe_cc_value(&self, channel: u8, cc: u8) -> u8 {
        self.controls[Self::cc_index(channel, cc)]
    }

    pub fn maybe_pitch_wheel_value(&self, channel: u8) -> u16 {
        self.pitch_wheel[(channel & 0x0F) as usize]
    }

    pub fn maybe_channel_pressure_value(&self, channel: u8) -> u8 {
        self.channel_pressure[(channel & 0x0F) as usize]
    }

    pub fn maybe_program_value(&self, channel: u8) -> u8 {
        self.programs[(channel & 0x0F) as usize]
    }
}