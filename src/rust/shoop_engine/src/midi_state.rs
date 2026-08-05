//! Running MIDI state, and the messages needed to reproduce it.
//!
//! A channel needs to know what state its output is in so that, when playback
//! jumps into the middle of a recording, it can first emit whatever is required
//! to make the receiver match. Held notes, controller values, pitch wheel,
//! channel pressure and program are each tracked optionally, because tracking
//! costs memory per channel and not every use needs all of it.
//!
//! Here the diff is computed by comparing two trackers instead, which needs no
//! raw pointers and no subscription lifetime to manage.

use crate::midi::{self, N_CHANNELS, N_NOTES};
use crate::midi_storage::MidiStorageElem;

/// Which categories of state to keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackWhat {
    pub notes: bool,
    pub controls: bool,
    pub programs: bool,
}

impl TrackWhat {
    pub const ALL: Self = Self {
        notes: true,
        controls: true,
        programs: true,
    };
    pub const NOTHING: Self = Self {
        notes: false,
        controls: false,
        programs: false,
    };
    pub fn anything(self) -> bool {
        self.notes || self.controls || self.programs
    }
}

/// Pitch wheel centre. Known from the start rather than unknown: a receiver that
/// has never been sent a pitch wheel message is at centre, so the value can be
/// restored without having observed it.
pub const PITCH_WHEEL_CENTRE: u16 = 0x2000;

/// Upper bound on the messages [`MidiStateTracker::diff_to_into`] can emit.
///
/// Every channel can differ in each controller, its pitch wheel, channel pressure
/// and program, and in each note. Audio-thread callers preallocate this so a
/// restore never has to grow its buffer.
pub const MAX_DIFF_MESSAGES: usize = N_CHANNELS * (N_NOTES + 3 + N_NOTES);

/// Controllers whose neutral value is known without having seen one: the two hold
/// pedals, which are released until pressed. Every other controller starts unknown,
/// because there is no safe value to assume.
const KNOWN_NEUTRAL_CCS: [(u8, u8); 2] = [(64, 0), (69, 0)];

/// Per-channel controller state.
///
/// `None` means never observed and no neutral value to assume, so it is left out
/// of a restore rather than invented.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Controls {
    cc: [Option<u8>; N_NOTES],
    pitch_wheel: Option<u16>,
    channel_pressure: Option<u8>,
}

impl Default for Controls {
    fn default() -> Self {
        let mut cc = [None; N_NOTES];
        for (controller, value) in KNOWN_NEUTRAL_CCS {
            cc[controller as usize] = Some(value);
        }
        Self {
            cc,
            pitch_wheel: Some(PITCH_WHEEL_CENTRE),
            channel_pressure: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MidiStateTracker {
    track: TrackWhat,
    /// Velocity per (channel, note); `None` means not sounding.
    notes: Vec<[Option<u8>; N_NOTES]>,
    n_notes_active: u32,
    controls: Vec<Controls>,
    programs: Vec<Option<u8>>,
}

impl MidiStateTracker {
    pub fn new(track: TrackWhat) -> Self {
        Self {
            track,
            notes: if track.notes {
                vec![[None; N_NOTES]; N_CHANNELS]
            } else {
                Vec::new()
            },
            n_notes_active: 0,
            controls: if track.controls {
                vec![Controls::default(); N_CHANNELS]
            } else {
                Vec::new()
            },
            programs: if track.programs {
                vec![None; N_CHANNELS]
            } else {
                Vec::new()
            },
        }
    }

    pub fn tracking(&self) -> TrackWhat {
        self.track
    }
    pub fn n_notes_active(&self) -> u32 {
        self.n_notes_active
    }

    pub fn note_velocity(&self, ch: u8, note: u8) -> Option<u8> {
        self.notes
            .get(ch as usize)
            .and_then(|n| n.get(note as usize).copied().flatten())
    }
    pub fn cc_value(&self, ch: u8, controller: u8) -> Option<u8> {
        self.controls
            .get(ch as usize)
            .and_then(|c| c.cc.get(controller as usize).copied().flatten())
    }
    pub fn pitch_wheel(&self, ch: u8) -> Option<u16> {
        self.controls.get(ch as usize).and_then(|c| c.pitch_wheel)
    }
    pub fn channel_pressure(&self, ch: u8) -> Option<u8> {
        self.controls
            .get(ch as usize)
            .and_then(|c| c.channel_pressure)
    }
    pub fn program(&self, ch: u8) -> Option<u8> {
        self.programs.get(ch as usize).copied().flatten()
    }

    pub fn clear(&mut self) {
        for ch in self.notes.iter_mut() {
            *ch = [None; N_NOTES];
        }
        self.n_notes_active = 0;
        for c in self.controls.iter_mut() {
            *c = Controls::default();
        }
        for p in self.programs.iter_mut() {
            *p = None;
        }
    }

    /// Adopts whatever categories both trackers have in common.
    pub fn copy_relevant_state(&mut self, other: &Self) {
        if self.track.notes && other.track.notes {
            self.notes.clone_from(&other.notes);
            self.n_notes_active = other.n_notes_active;
        }
        if self.track.controls && other.track.controls {
            self.controls.clone_from(&other.controls);
        }
        if self.track.programs && other.track.programs {
            self.programs.clone_from(&other.programs);
        }
    }

    /// Folds one message into the state.
    pub fn process(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        let ch = midi::channel(data) as usize;

        if self.track.notes {
            if let Some(off_ch) =
                midi::all_notes_off_channel(data).or_else(|| midi::all_sound_off_channel(data))
            {
                if let Some(slots) = self.notes.get_mut(off_ch as usize) {
                    for slot in slots.iter_mut() {
                        if slot.take().is_some() {
                            self.n_notes_active -= 1;
                        }
                    }
                }
                return;
            }
            if midi::is_note_on(data) {
                if let Some(slot) = self
                    .notes
                    .get_mut(ch)
                    .and_then(|n| n.get_mut(midi::note(data) as usize))
                {
                    if slot.is_none() {
                        self.n_notes_active += 1;
                    }
                    *slot = Some(midi::velocity(data));
                }
                return;
            }
            if midi::is_note_off(data) {
                if let Some(slot) = self
                    .notes
                    .get_mut(ch)
                    .and_then(|n| n.get_mut(midi::note(data) as usize))
                {
                    if slot.take().is_some() {
                        self.n_notes_active -= 1;
                    }
                }
                return;
            }
        }

        if self.track.controls {
            if midi::is_cc(data) {
                if let Some(c) = self.controls.get_mut(ch) {
                    if let Some(slot) = c.cc.get_mut(data[1] as usize) {
                        *slot = Some(data[2]);
                    }
                }
                return;
            }
            if midi::is_pitch_wheel(data) {
                if let Some(c) = self.controls.get_mut(ch) {
                    c.pitch_wheel = Some((data[1] as u16) | ((data[2] as u16) << 7));
                }
                return;
            }
            if midi::is_channel_pressure(data) {
                if let Some(c) = self.controls.get_mut(ch) {
                    c.channel_pressure = Some(data[1]);
                }
                return;
            }
        }

        if self.track.programs && midi::is_program(data) {
            if let Some(p) = self.programs.get_mut(ch) {
                *p = Some(data[1]);
            }
        }
    }

    /// Messages that would bring a fresh receiver to this state.
    ///
    /// Controllers come before notes so a note sounds with the intended
    /// controller values already applied.
    pub fn state_as_messages(&self) -> Vec<Vec<u8>> {
        let mut storage = Vec::with_capacity(MAX_DIFF_MESSAGES);
        self.state_as_messages_into(&mut storage);
        storage
            .iter()
            .map(|message| message.data().to_vec())
            .collect()
    }

    /// Allocation-free variant for process-side publication.
    pub fn state_as_messages_into(&self, out: &mut Vec<MidiStorageElem>) {
        out.clear();
        let mut push = |data: &[u8]| {
            if let Some(message) = MidiStorageElem::new(0, data) {
                out.push(message);
            }
        };
        for ch in 0..N_CHANNELS as u8 {
            if let Some(c) = self.controls.get(ch as usize) {
                for (controller, value) in c.cc.iter().enumerate() {
                    if let Some(value) = value {
                        push(&midi::cc(ch, controller as u8, *value));
                    }
                }
                if let Some(value) = c.pitch_wheel {
                    push(&midi::pitch_wheel(ch, value));
                }
                if let Some(value) = c.channel_pressure {
                    push(&midi::channel_pressure(ch, value));
                }
            }
            if let Some(program) = self.program(ch) {
                push(&midi::program_change(ch, program));
            }
        }
        for ch in 0..N_CHANNELS as u8 {
            if let Some(slots) = self.notes.get(ch as usize) {
                for (note, velocity) in slots.iter().enumerate() {
                    if let Some(velocity) = velocity {
                        push(&midi::note_on(ch, note as u8, *velocity));
                    }
                }
            }
        }
    }

    /// [`Self::diff_to`] without allocating, appending into `out`.
    ///
    /// Playback state restoration happens on the audio thread, so the caller keeps
    /// a reusable buffer rather than receiving a fresh `Vec` of `Vec`s.
    ///
    /// One message per differing key, matching `MidiStateDiffTracker::resolve_to`:
    ///
    /// - a note that should be sounding is retriggered at the target velocity; one
    ///   that should not gets a note-off at velocity 64
    /// - a controller is always sent, falling back to 0 when the target never
    ///   observed one, because leaving a controller where it drifted to is worse
    ///   than guessing zero
    /// - a program, channel pressure or pitch wheel is sent only when the target
    ///   has a value, which for pitch wheel is always, since centre is known
    pub fn diff_to_into(&self, target: &Self, out: &mut Vec<MidiStorageElem>) {
        let mut push = |data: &[u8]| {
            if let Some(e) = MidiStorageElem::new(0, data) {
                out.push(e);
            }
        };

        for ch in 0..N_CHANNELS as u8 {
            if let (Some(a), Some(b)) = (
                self.controls.get(ch as usize),
                target.controls.get(ch as usize),
            ) {
                for (controller, (va, vb)) in a.cc.iter().zip(b.cc.iter()).enumerate() {
                    if va != vb {
                        push(&midi::cc(ch, controller as u8, vb.unwrap_or(0)));
                    }
                }
                if a.pitch_wheel != b.pitch_wheel {
                    if let Some(v) = b.pitch_wheel {
                        push(&midi::pitch_wheel(ch, v));
                    }
                }
                if a.channel_pressure != b.channel_pressure {
                    if let Some(v) = b.channel_pressure {
                        push(&midi::channel_pressure(ch, v));
                    }
                }
            }
            if self.program(ch) != target.program(ch) {
                if let Some(p) = target.program(ch) {
                    push(&midi::program_change(ch, p));
                }
            }
        }

        for ch in 0..N_CHANNELS as u8 {
            if let (Some(a), Some(b)) = (self.notes.get(ch as usize), target.notes.get(ch as usize))
            {
                for (note, (va, vb)) in a.iter().zip(b.iter()).enumerate() {
                    if va == vb {
                        continue;
                    }
                    match vb {
                        Some(v) => push(&midi::note_on(ch, note as u8, *v)),
                        None => push(&midi::note_off(ch, note as u8, 64)),
                    }
                }
            }
        }
    }

    /// Messages that would move a receiver in state `self` to state `target`.
    ///
    /// Allocating convenience wrapper over [`Self::diff_to_into`] for control-path
    /// callers and tests.
    pub fn diff_to(&self, target: &Self) -> Vec<Vec<u8>> {
        let mut buf = Vec::new();
        self.diff_to_into(target, &mut buf);
        buf.iter().map(|m| m.data().to_vec()).collect()
    }

    /// Note-offs for everything currently sounding.
    pub fn all_notes_off_messages(&self) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        for ch in 0..N_CHANNELS as u8 {
            if let Some(slots) = self.notes.get(ch as usize) {
                for (note, v) in slots.iter().enumerate() {
                    if v.is_some() {
                        out.push(midi::note_off(ch, note as u8, 0).to_vec());
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert2::check;

    fn tracker() -> MidiStateTracker {
        MidiStateTracker::new(TrackWhat::ALL)
    }

    #[test]
    fn tracks_notes_on_and_off() {
        let mut t = tracker();
        check!(t.n_notes_active() == 0);
        t.process(&midi::note_on(0, 60, 100));
        check!(t.n_notes_active() == 1);
        check!(t.note_velocity(0, 60) == Some(100));

        t.process(&midi::note_off(0, 60, 0));
        check!(t.n_notes_active() == 0);
        check!(t.note_velocity(0, 60) == None);
    }

    #[test]
    fn zero_velocity_note_on_releases() {
        let mut t = tracker();
        t.process(&midi::note_on(0, 60, 100));
        t.process(&midi::note_on(0, 60, 0));
        check!(t.n_notes_active() == 0);
    }

    #[test]
    fn retriggering_a_sounding_note_does_not_double_count() {
        let mut t = tracker();
        t.process(&midi::note_on(0, 60, 10));
        t.process(&midi::note_on(0, 60, 20));
        check!(t.n_notes_active() == 1);
        check!(t.note_velocity(0, 60) == Some(20));
    }

    #[test]
    fn note_off_for_silent_note_does_not_underflow() {
        let mut t = tracker();
        t.process(&midi::note_off(0, 60, 0));
        check!(t.n_notes_active() == 0);
    }

    #[test]
    fn all_notes_off_clears_only_its_channel() {
        let mut t = tracker();
        t.process(&midi::note_on(0, 60, 1));
        t.process(&midi::note_on(1, 61, 1));
        check!(t.n_notes_active() == 2);
        t.process(&midi::cc(0, 123, 0));
        check!(t.n_notes_active() == 1);
        check!(t.note_velocity(0, 60) == None);
        check!(t.note_velocity(1, 61) == Some(1));
    }

    #[test]
    fn all_sound_off_also_clears_notes() {
        let mut t = tracker();
        t.process(&midi::note_on(2, 40, 1));
        t.process(&midi::all_sound_off(2));
        check!(t.n_notes_active() == 0);
    }

    #[test]
    fn tracks_controls_and_programs() {
        let mut t = tracker();
        check!(t.cc_value(0, 7) == None);
        t.process(&midi::cc(0, 7, 99));
        check!(t.cc_value(0, 7) == Some(99));

        t.process(&midi::pitch_wheel(3, 1000));
        check!(t.pitch_wheel(3) == Some(1000));

        t.process(&midi::channel_pressure(4, 55));
        check!(t.channel_pressure(4) == Some(55));

        t.process(&midi::program_change(5, 12));
        check!(t.program(5) == Some(12));
    }

    #[test]
    fn untracked_categories_are_not_stored() {
        let mut t = MidiStateTracker::new(TrackWhat {
            notes: true,
            controls: false,
            programs: false,
        });
        t.process(&midi::cc(0, 7, 99));
        t.process(&midi::program_change(0, 3));
        check!(t.cc_value(0, 7) == None);
        check!(t.program(0) == None);
        // Notes still work.
        t.process(&midi::note_on(0, 60, 1));
        check!(t.n_notes_active() == 1);
    }

    #[test]
    fn cc_value_of_zero_is_distinct_from_unset() {
        let mut t = tracker();
        check!(t.cc_value(0, 7) == None);
        t.process(&midi::cc(0, 7, 0));
        check!(t.cc_value(0, 7) == Some(0));
        // An explicit zero must be reproduced, unlike a never-sent controller.
        let msgs = t.state_as_messages();
        check!(msgs.contains(&midi::cc(0, 7, 0).to_vec()));
    }

    #[test]
    fn clear_resets_everything() {
        let mut t = tracker();
        t.process(&midi::note_on(0, 60, 1));
        t.process(&midi::cc(0, 7, 9));
        t.process(&midi::program_change(0, 4));
        t.clear();
        check!(t.n_notes_active() == 0);
        check!(t.cc_value(0, 7) == None);
        check!(t.program(0) == None);
        // Back to the neutral defaults, not blank: pitch centre and released pedals
        // are known states, so a cleared tracker equals a fresh one.
        check!(t == tracker());
        check!(t.pitch_wheel(0) == Some(PITCH_WHEEL_CENTRE));
        check!(t.cc_value(0, 64) == Some(0));
    }

    #[test]
    fn state_as_messages_reproduces_state() {
        let mut t = tracker();
        t.process(&midi::cc(0, 7, 90));
        t.process(&midi::program_change(0, 3));
        t.process(&midi::note_on(0, 60, 100));

        let mut replayed = tracker();
        for m in t.state_as_messages() {
            replayed.process(&m);
        }
        check!(replayed == t);
    }

    #[test]
    fn state_as_messages_puts_controls_before_notes() {
        let mut t = tracker();
        t.process(&midi::note_on(0, 60, 100));
        t.process(&midi::cc(0, 7, 90));
        let msgs = t.state_as_messages();
        let cc_at = msgs.iter().position(|m| midi::is_cc(m));
        let note_at = msgs.iter().position(|m| midi::is_note_on(m));
        check!(cc_at < note_at);
    }

    #[test]
    fn diff_to_produces_the_missing_messages() {
        let mut from = tracker();
        from.process(&midi::cc(0, 7, 10));

        let mut to = tracker();
        to.process(&midi::cc(0, 7, 20));
        to.process(&midi::note_on(0, 60, 5));

        let mut applied = from.clone();
        for m in from.diff_to(&to) {
            applied.process(&m);
        }
        check!(applied == to);
    }

    #[test]
    /// A controller the target never observed is sent as 0 rather than skipped:
    /// leaving it where it drifted to is worse than assuming the neutral value.
    /// The advertised bound really does bound it, so audio-thread callers can size
    /// a buffer from it.
    fn diff_to_never_exceeds_its_advertised_bound() {
        let mut from = tracker();
        for ch in 0..N_CHANNELS as u8 {
            for n in 0..N_NOTES as u8 {
                from.process(&midi::note_on(ch, n, 100));
                from.process(&midi::cc(ch, n, 100));
            }
            from.process(&midi::pitch_wheel(ch, 1));
            from.process(&midi::channel_pressure(ch, 1));
            from.process(&midi::program_change(ch, 1));
        }
        let to = tracker();

        let mut out = Vec::new();
        from.diff_to_into(&to, &mut out);
        check!(out.len() <= MAX_DIFF_MESSAGES);
        // Nearly everything differs, so the bound is close rather than vacuous.
        check!(out.len() > MAX_DIFF_MESSAGES / 2);
    }

    #[test]
    fn diff_to_zeroes_a_controller_the_target_never_observed() {
        let mut from = tracker();
        from.process(&midi::cc(0, 7, 90));
        // Controller 7 has no known default, so the fresh tracker has never seen it.
        let to = tracker();
        check!(to.cc_value(0, 7) == None);

        check!(from.diff_to(&to) == vec![midi::cc(0, 7, 0).to_vec()]);
    }

    #[test]
    /// Unlike controllers, a program or channel pressure the target never observed
    /// is left alone: there is no neutral program to fall back to.
    fn diff_to_skips_an_unobserved_program_or_pressure() {
        let mut from = tracker();
        from.process(&midi::program_change(0, 4));
        from.process(&midi::channel_pressure(0, 20));
        let to = tracker();

        check!(from.diff_to(&to).is_empty());
    }

    #[test]
    fn diff_to_releases_notes_no_longer_sounding() {
        let mut from = tracker();
        from.process(&midi::note_on(0, 60, 5));
        let to = tracker();

        // Velocity 64 is the conventional "no particular release" value.
        let diff = from.diff_to(&to);
        check!(diff == vec![midi::note_off(0, 60, 64).to_vec()]);

        let mut applied = from.clone();
        for m in &diff {
            applied.process(m);
        }
        check!(applied.n_notes_active() == 0);
    }

    #[test]
    fn diff_to_retriggers_notes_whose_velocity_changed() {
        let mut from = tracker();
        from.process(&midi::note_on(0, 60, 5));
        let mut to = tracker();
        to.process(&midi::note_on(0, 60, 90));

        // One message, not an off/on pair: a note-on at the target velocity is
        // enough to restore the state, and re-attacking twice is worse.
        let diff = from.diff_to(&to);
        check!(diff == vec![midi::note_on(0, 60, 90).to_vec()]);

        let mut applied = from.clone();
        for m in &diff {
            applied.process(m);
        }
        check!(applied == to);
    }

    #[test]
    fn diff_to_identical_state_is_empty() {
        let mut a = tracker();
        a.process(&midi::note_on(0, 60, 5));
        a.process(&midi::cc(0, 7, 1));
        let b = a.clone();
        check!(a.diff_to(&b).is_empty());
    }

    #[test]
    fn copy_relevant_state_copies_shared_categories_only() {
        let mut full = tracker();
        full.process(&midi::note_on(0, 60, 7));
        full.process(&midi::cc(0, 7, 3));

        let mut notes_only = MidiStateTracker::new(TrackWhat {
            notes: true,
            controls: false,
            programs: false,
        });
        notes_only.copy_relevant_state(&full);
        check!(notes_only.n_notes_active() == 1);
        check!(notes_only.note_velocity(0, 60) == Some(7));
        // Controls are not tracked here, so nothing was taken.
        check!(notes_only.cc_value(0, 7) == None);
    }

    #[test]
    fn copy_relevant_state_does_not_add_untracked_categories() {
        let mut full = tracker();
        full.process(&midi::note_on(0, 60, 7));
        full.process(&midi::cc(0, 7, 3));

        let mut controls_only = MidiStateTracker::new(TrackWhat {
            notes: false,
            controls: true,
            programs: false,
        });
        controls_only.copy_relevant_state(&full);
        // Controls come across; note state must not, or this tracker would start
        // reporting notes it was explicitly built not to track.
        check!(controls_only.cc_value(0, 7) == Some(3));
        check!(controls_only.note_velocity(0, 60) == None);
        check!(controls_only.n_notes_active() == 0);
        check!(controls_only.all_notes_off_messages().is_empty());
    }

    #[test]
    fn all_notes_off_messages_covers_sounding_notes() {
        let mut t = tracker();
        t.process(&midi::note_on(0, 60, 1));
        t.process(&midi::note_on(5, 70, 1));
        let msgs = t.all_notes_off_messages();
        check!(msgs.len() == 2);
        for m in &msgs {
            check!(midi::is_note_off(m));
        }
    }

    #[test]
    fn empty_message_is_ignored() {
        let mut t = tracker();
        t.process(&[]);
        check!(t.n_notes_active() == 0);
    }
}
