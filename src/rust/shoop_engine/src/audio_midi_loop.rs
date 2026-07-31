//! A loop with channels attached.
//!
//! `BasicLoop` owns loop mechanics; this type owns the channels and keeps the
//! loop's point of interest in sync with theirs. Every method that can move the
//! `PROC_update_poi` override achieved through virtual dispatch. `update_poi`
//! recomputes loop-end and channel POIs from scratch, so re-running it is
//! idempotent rather than cumulative.

use crate::audio_channel::{AudioChannel, ChannelError};
use crate::basic_loop::{BasicLoop, SyncSourceState};
use crate::channel_mode::ChannelMode;
use crate::loop_mode::LoopMode;
use crate::midi_channel::{MidiChannel, MidiChannelError};
use crate::midi_storage::MidiStorageElem;
use crate::state_mirror::LoopStateMirror;

use std::sync::Arc;
use thiserror::Error;

/// A channel failed while the loop itself advanced.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum LoopError {
    #[error("audio channel: {0}")]
    Audio(#[from] ChannelError),
    #[error("midi channel: {0}")]
    Midi(#[from] MidiChannelError),
}

#[derive(Debug, Default)]
pub struct AudioMidiLoop {
    loop_: BasicLoop,
    audio_channels: Vec<AudioChannel>,
    midi_channels: Vec<MidiChannel>,
}

impl AudioMidiLoop {
    pub fn with_state_mirror(state: Arc<LoopStateMirror>) -> Self {
        Self {
            loop_: BasicLoop::with_state_mirror(state),
            ..Default::default()
        }
    }

    // --- channels ---

    /// Adds an audio channel and returns its index.
    pub fn add_audio_channel(&mut self, chunk_size: usize, mode: ChannelMode) -> usize {
        self.audio_channels
            .push(AudioChannel::with_chunk_size(chunk_size, mode));
        self.resync_poi();
        self.audio_channels.len() - 1
    }

    pub fn n_audio_channels(&self) -> usize {
        self.audio_channels.len()
    }
    pub fn audio_channel(&self, idx: usize) -> Option<&AudioChannel> {
        self.audio_channels.get(idx)
    }
    pub fn audio_channel_mut(&mut self, idx: usize) -> Option<&mut AudioChannel> {
        self.audio_channels.get_mut(idx)
    }

    pub fn delete_audio_channel(&mut self, idx: usize) -> bool {
        if idx >= self.audio_channels.len() {
            return false;
        }
        self.audio_channels.remove(idx);
        self.resync_poi();
        true
    }

    /// Adds a MIDI channel and returns its index.
    pub fn add_midi_channel(&mut self, capacity_elems: usize, mode: ChannelMode) -> usize {
        self.midi_channels
            .push(MidiChannel::with_capacity_elems(capacity_elems, mode));
        self.resync_poi();
        self.midi_channels.len() - 1
    }

    pub fn n_midi_channels(&self) -> usize {
        self.midi_channels.len()
    }
    pub fn midi_channel(&self, idx: usize) -> Option<&MidiChannel> {
        self.midi_channels.get(idx)
    }
    pub fn midi_channel_mut(&mut self, idx: usize) -> Option<&mut MidiChannel> {
        self.midi_channels.get_mut(idx)
    }

    pub fn delete_midi_channel(&mut self, idx: usize) -> bool {
        if idx >= self.midi_channels.len() {
            return false;
        }
        self.midi_channels.remove(idx);
        self.resync_poi();
        true
    }

    // --- loop state, mirrored so callers never touch the inner loop directly ---

    pub fn mode(&self) -> LoopMode {
        self.loop_.mode()
    }
    pub fn length(&self) -> u32 {
        self.loop_.length()
    }
    pub fn position(&self) -> u32 {
        self.loop_.position()
    }
    pub fn next_poi(&self) -> Option<u32> {
        self.loop_.next_poi()
    }
    pub fn predicted_next_trigger_eta(&self) -> Option<u32> {
        self.loop_.predicted_next_trigger_eta()
    }
    pub fn sync_source(&self) -> Option<SyncSourceState> {
        self.loop_.sync_source()
    }
    pub fn as_sync_source_state(&self) -> SyncSourceState {
        self.loop_.as_sync_source_state()
    }
    pub fn first_planned_transition(&self) -> Option<(LoopMode, u32)> {
        self.loop_.first_planned_transition()
    }
    pub fn n_planned_transitions(&self) -> usize {
        self.loop_.n_planned_transitions()
    }

    pub fn set_sync_source(&mut self, src: Option<SyncSourceState>) {
        self.loop_.set_sync_source(src);
        self.resync_poi();
    }
    pub fn set_mode(&mut self, mode: LoopMode) {
        self.loop_.set_mode(mode);
        self.resync_poi();
    }
    pub fn set_length(&mut self, length: u32) {
        self.loop_.set_length(length);
        self.resync_poi();
    }
    pub fn set_position(&mut self, position: u32) {
        self.loop_.set_position(position);
        self.resync_poi();
    }
    pub fn plan_transition(
        &mut self,
        mode: LoopMode,
        n_cycles_delay: Option<u32>,
        to_sync_cycle: Option<u32>,
    ) {
        self.loop_
            .plan_transition(mode, n_cycles_delay, to_sync_cycle);
        self.resync_poi();
    }
    /// Empties every channel and resets the loop to `length`, stopped.
    ///
    /// is planned *before* the channels are emptied, so a loop that was mid-playback
    /// does not briefly play whatever the clear leaves behind.
    pub fn clear(&mut self, length: u32) {
        self.clear_planned_transitions();
        self.plan_transition(LoopMode::Stopped, Some(0), None);
        for c in self.audio_channels.iter_mut() {
            c.clear(length as usize);
        }
        for c in self.midi_channels.iter_mut() {
            c.clear();
        }
        self.set_length(length);
    }

    pub fn clear_planned_transitions(&mut self) {
        self.loop_.clear_planned_transitions();
        self.resync_poi();
    }
    pub fn trigger(&mut self, propagate: bool) {
        self.loop_.trigger(propagate);
        self.resync_poi();
    }
    pub fn handle_sync(&mut self) {
        self.loop_.handle_sync();
        self.resync_poi();
    }
    pub fn is_triggering_now(&mut self) -> bool {
        let r = self.loop_.is_triggering_now();
        self.resync_poi();
        r
    }

    /// Earliest channel point of interest under the current loop state.
    fn channel_poi(&self) -> Option<u32> {
        let (next_mode, next_delay) = match self.loop_.first_planned_transition() {
            Some((m, d)) => (m, Some(d)),
            None => (LoopMode::Unknown, None),
        };
        let eta = self.loop_.predicted_next_trigger_eta();
        let mode = self.loop_.mode();
        let pos = self.loop_.position() as i32;
        let audio = self
            .audio_channels
            .iter()
            .filter_map(|c| c.next_poi(mode, next_mode, next_delay, eta, pos))
            .min()
            .map(|v| v as u32);
        let midi = self
            .midi_channels
            .iter()
            .filter_map(|c| c.next_poi(mode, next_mode, next_delay, eta, pos))
            .min();
        match (audio, midi) {
            (Some(a), Some(m)) => Some(a.min(m)),
            (a, m) => a.or(m),
        }
    }

    /// Recomputes the loop POI, then folds in the channels'.
    pub fn resync_poi(&mut self) {
        self.loop_.update_poi();
        if let Some(p) = self.channel_poi() {
            self.loop_.merge_channel_poi(p);
        }
    }

    pub fn handle_poi(&mut self) {
        self.loop_.handle_poi();
        self.resync_poi();
    }

    // --- processing ---

    /// Advances the loop and its channels by `n_samples`.
    ///
    /// Audio channels only queue their copies here; call
    /// [`AudioMidiLoop::finalize_process`] to move samples. MIDI is emitted
    /// immediately, so `midi_in` and `midi_out` supply one entry per MIDI channel,
    /// in channel order, up front.
    ///
    /// Two parallel slices rather than a slice of pairs: a caller can then hold
    /// its input and output buffers in reusable collections instead of building a
    /// pair vector every cycle, which would allocate on the audio thread.
    ///
    /// Port buffer sizes bound the point of interest, so callers must assign them
    /// and then [`AudioMidiLoop::resync_poi`] before processing a cycle.
    pub fn process<I: AsRef<[MidiStorageElem]>>(
        &mut self,
        n_samples: u32,
        midi_in: &[I],
        midi_out: &mut [Vec<MidiStorageElem>],
    ) -> Result<(), LoopError> {
        let audio = &mut self.audio_channels;
        let midi = &mut self.midi_channels;
        let mut err: Option<LoopError> = None;
        self.loop_.process_with(n_samples, |params| {
            for c in audio.iter_mut() {
                let r = c.process(
                    params.mode,
                    params.next_planned_mode,
                    params.next_planned_delay_cycles,
                    params.next_planned_eta,
                    params.n_samples as usize,
                    params.pos_before as i32,
                    params.length_before as usize,
                );
                // Keep processing the remaining channels, but report the first
                // failure: one channel's buffer trouble must not silently drop
                // the others' work.
                if let (Err(e), None) = (r, &err) {
                    err = Some(e.into());
                }
            }
            for ((c, input), out) in midi.iter_mut().zip(midi_in.iter()).zip(midi_out.iter_mut()) {
                let r = c.process(
                    params.mode,
                    params.next_planned_mode,
                    params.next_planned_delay_cycles,
                    params.next_planned_eta,
                    params.n_samples,
                    params.pos_before as i32,
                    params.pos_after,
                    params.length_before,
                    input.as_ref(),
                    out,
                );
                if let (Err(e), None) = (r, &err) {
                    err = Some(e.into());
                }
            }
        });
        self.resync_poi();
        match err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Applies every channel's queued copies for this cycle.
    ///
    /// `buffers` supplies one (input, output) pair per audio channel, in channel
    /// order. Channels beyond the supplied pairs are left queued.
    pub fn finalize_process(&mut self, buffers: &mut [(&[f32], &mut [f32])]) {
        for (c, (input, output)) in self.audio_channels.iter_mut().zip(buffers.iter_mut()) {
            c.finalize_process(input, output);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert2::{check, let_assert};

    use ChannelMode as C;
    use LoopMode as L;

    fn loop_with_channel() -> AudioMidiLoop {
        let mut l = AudioMidiLoop::default();
        l.add_audio_channel(4, C::Direct);
        l
    }

    /// One cycle: size the channel's port buffers, process, finalize.
    fn cycle(l: &mut AudioMidiLoop, n: usize, input: &[f32]) -> Vec<f32> {
        let ch = l.audio_channel_mut(0).unwrap();
        ch.set_recording_buffer_size(n);
        ch.set_playback_buffer_size(n);
        // Buffer sizes bound the point of interest, so resync before processing.
        // An unassigned buffer reports a point of interest of 0, so skipping this
        // would ask the loop to cross it.
        l.resync_poi();
        let mut src = input.to_vec();
        src.resize(n, 0.0);
        let mut out = vec![0.0; n];
        let_assert!(Ok(()) = l.process::<Vec<MidiStorageElem>>(n as u32, &[], &mut []));
        l.finalize_process(&mut [(&src, &mut out)]);
        out
    }

    #[test]
    fn no_channels_behaves_like_a_bare_loop() {
        let mut l = AudioMidiLoop::default();
        check!(l.mode() == L::Stopped);
        check!(l.next_poi() == None);
        let_assert!(Ok(()) = l.process::<Vec<MidiStorageElem>>(1000, &[], &mut []));
        check!(l.length() == 0);
        check!(l.position() == 0);
    }

    #[test]
    fn adding_and_removing_channels() {
        let mut l = AudioMidiLoop::default();
        check!(l.n_audio_channels() == 0);
        let i = l.add_audio_channel(4, C::Direct);
        check!(i == 0);
        l.add_audio_channel(4, C::Wet);
        check!(l.n_audio_channels() == 2);
        check!(l.audio_channel(1).map(|c| c.mode()) == Some(C::Wet));
        check!(l.delete_audio_channel(0) == true);
        check!(l.n_audio_channels() == 1);
        // The surviving channel shifts down.
        check!(l.audio_channel(0).map(|c| c.mode()) == Some(C::Wet));
        check!(l.delete_audio_channel(5) == false);
    }

    #[test]
    fn recording_drives_the_channel() {
        let mut l = loop_with_channel();
        l.set_mode(L::Recording);
        cycle(&mut l, 4, &[1.0, 2.0, 3.0, 4.0]);
        check!(l.length() == 4);
        let_assert!(Some(ch) = l.audio_channel(0));
        check!(ch.data() == vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn playback_reads_the_channel() {
        let mut l = loop_with_channel();
        l.audio_channel_mut(0)
            .unwrap()
            .load_data(&[1.0, 2.0, 3.0, 4.0]);
        l.set_length(4);
        l.set_mode(L::Playing);
        let out = cycle(&mut l, 4, &[]);
        check!(out == vec![1.0, 2.0, 3.0, 4.0]);
        // Reaching exactly the end triggers loop-end, which wraps to 0.
        check!(l.position() == 0);
    }

    #[test]
    fn playback_short_of_the_end_does_not_wrap() {
        let mut l = loop_with_channel();
        l.audio_channel_mut(0)
            .unwrap()
            .load_data(&[1.0, 2.0, 3.0, 4.0]);
        l.set_length(4);
        l.set_mode(L::Playing);
        let out = cycle(&mut l, 3, &[]);
        check!(out == vec![1.0, 2.0, 3.0]);
        check!(l.position() == 3);
    }

    #[test]
    fn channel_poi_constrains_the_loop() {
        let mut l = loop_with_channel();
        l.set_length(100);
        l.set_mode(L::Playing);
        // A channel with no buffer assigned reports 0, not "no constraint": the
        // loop must not advance into a channel that cannot accept the samples.
        l.audio_channel_mut(0).unwrap().clear_buffers();
        l.resync_poi();
        check!(l.next_poi() == Some(0));

        // A playback buffer of 8 samples means the loop cannot run past 8, even
        // though the loop end is 100 away.
        l.audio_channel_mut(0).unwrap().set_playback_buffer_size(8);
        l.resync_poi();
        check!(l.next_poi() == Some(8));
    }

    #[test]
    fn loop_end_wins_when_it_is_earlier_than_the_channel() {
        let mut l = loop_with_channel();
        l.set_length(3);
        l.set_mode(L::Playing);
        l.audio_channel_mut(0).unwrap().set_playback_buffer_size(64);
        l.resync_poi();
        check!(l.next_poi() == Some(3));
    }

    #[test]
    fn channel_poi_survives_loop_state_changes() {
        let mut l = loop_with_channel();
        l.audio_channel_mut(0).unwrap().set_playback_buffer_size(5);
        l.set_length(100);
        l.set_mode(L::Playing);
        // set_length and set_mode both recompute internally; the channel POI
        // must still be folded in afterwards.
        check!(l.next_poi() == Some(5));
        l.set_position(10);
        check!(l.next_poi() == Some(5));
    }

    /// channel is the one case where the loop can outrun a channel's input buffer.
    /// The channel reports it rather than overrunning, and the loop still advances.
    #[test]
    fn process_reports_channel_errors_without_stopping_the_loop() {
        let mut l = loop_with_channel();
        l.audio_channel_mut(0)
            .unwrap()
            .load_data(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
        l.set_length(8);
        l.set_mode(L::Replacing);
        let ch = l.audio_channel_mut(0).unwrap();
        ch.set_recording_buffer_size(2);
        ch.set_playback_buffer_size(8);
        l.resync_poi();
        // Replace is not part of the point of interest, so the loop is free to
        // ask for more than the 2 frames the input holds.
        check!(l.next_poi() == Some(8));

        let r = l.process::<Vec<MidiStorageElem>>(8, &[], &mut []);
        let_assert!(
            Err(LoopError::Audio(
                ChannelError::ReplaceInputOutOfBounds { .. }
            )) = r
        );
        // Reaching exactly the end wraps the position back to 0.
        check!(l.position() == 0);
    }

    #[test]
    fn multiple_channels_each_get_their_buffers() {
        let mut l = AudioMidiLoop::default();
        l.add_audio_channel(4, C::Direct);
        l.add_audio_channel(4, C::Direct);
        l.set_mode(L::Recording);
        for i in 0..2 {
            let ch = l.audio_channel_mut(i).unwrap();
            ch.set_recording_buffer_size(2);
            ch.set_playback_buffer_size(2);
        }
        l.resync_poi();
        let_assert!(Ok(()) = l.process::<Vec<MidiStorageElem>>(2, &[], &mut []));
        let (a, b) = ([1.0f32, 2.0], [3.0f32, 4.0]);
        let (mut oa, mut ob) = (vec![0.0; 2], vec![0.0; 2]);
        l.finalize_process(&mut [(&a, &mut oa), (&b, &mut ob)]);
        check!(l.audio_channel(0).unwrap().data() == vec![1.0, 2.0]);
        check!(l.audio_channel(1).unwrap().data() == vec![3.0, 4.0]);
    }

    #[test]
    fn adding_and_removing_midi_channels() {
        let mut l = AudioMidiLoop::default();
        check!(l.n_midi_channels() == 0);
        check!(l.add_midi_channel(64, C::Direct) == 0);
        l.add_midi_channel(64, C::Wet);
        check!(l.n_midi_channels() == 2);
        check!(l.midi_channel(1).map(|c| c.mode()) == Some(C::Wet));
        check!(l.delete_midi_channel(0));
        check!(l.midi_channel(0).map(|c| c.mode()) == Some(C::Wet));
        check!(!l.delete_midi_channel(9));
    }

    #[test]
    fn midi_channel_records_and_plays_through_the_loop() {
        let mut l = AudioMidiLoop::default();
        l.add_midi_channel(64, C::Direct);
        l.set_mode(L::Recording);

        // A balanced pair: an unbalanced note-on would leave the input state
        // drifted, and playback would prepend a note-off to revert it.
        let msg = crate::midi::note_on(0, 60, 100);
        let off = crate::midi::note_off(0, 60, 64);
        let input = [
            crate::midi_storage::MidiStorageElem::new(1, &msg).unwrap(),
            crate::midi_storage::MidiStorageElem::new(2, &off).unwrap(),
        ];
        l.midi_channel_mut(0).unwrap().set_recording_buffer(4);
        l.midi_channel_mut(0).unwrap().set_playback_buffer(4);
        // Buffer sizes bound the POI, so the loop must be resynced after
        // assigning them and before processing.
        l.resync_poi();
        let mut out = Vec::new();
        let_assert!(
            Ok(()) = l.process(
                4,
                std::slice::from_ref(&input.to_vec()),
                std::slice::from_mut(&mut out)
            )
        );
        check!(l.midi_channel(0).unwrap().n_events() == 2);
        check!(out.is_empty());

        // Now play it back.
        l.set_mode(L::Playing);
        l.set_length(4);
        l.midi_channel_mut(0).unwrap().set_recording_buffer(4);
        l.midi_channel_mut(0).unwrap().set_playback_buffer(4);
        l.resync_poi();
        let mut out = Vec::new();
        let_assert!(
            Ok(()) =
                l.process::<Vec<MidiStorageElem>>(4, &[Vec::new()], std::slice::from_mut(&mut out))
        );
        check!(out.len() == 2);
        check!(out[0].data() == msg.as_slice());
        check!(out[1].data() == off.as_slice());
    }

    #[test]
    fn midi_channel_poi_constrains_the_loop() {
        let mut l = AudioMidiLoop::default();
        l.add_midi_channel(64, C::Direct);
        l.set_length(100);
        l.set_mode(L::Playing);
        l.midi_channel_mut(0).unwrap().set_playback_buffer(6);
        l.resync_poi();
        check!(l.next_poi() == Some(6));
    }

    #[test]
    fn earliest_poi_across_audio_and_midi_wins() {
        let mut l = AudioMidiLoop::default();
        l.add_audio_channel(4, C::Direct);
        l.add_midi_channel(64, C::Direct);
        l.set_length(100);
        l.set_mode(L::Playing);
        l.audio_channel_mut(0).unwrap().set_playback_buffer_size(9);
        l.midi_channel_mut(0).unwrap().set_playback_buffer(5);
        l.resync_poi();
        check!(l.next_poi() == Some(5));

        // And the other way round.
        l.audio_channel_mut(0).unwrap().set_playback_buffer_size(3);
        l.resync_poi();
        check!(l.next_poi() == Some(3));
    }

    /// A MIDI channel's bounds errors are unreachable through the loop once the
    /// point of interest is respected: unlike audio, MIDI has no `Replace` path, so
    /// every mode it acts on is accounted for in `next_poi`. Its own unit tests
    /// cover those errors by calling the channel directly.
    #[test]
    fn a_midi_channel_is_never_asked_to_exceed_its_buffers() {
        let mut l = AudioMidiLoop::default();
        l.add_midi_channel(64, C::Direct);
        l.set_mode(L::Recording);
        l.midi_channel_mut(0).unwrap().set_recording_buffer(2);
        l.midi_channel_mut(0).unwrap().set_playback_buffer(8);
        l.resync_poi();
        // Bounded by the smaller of the two buffers.
        check!(l.next_poi() == Some(2));

        let mut out = Vec::new();
        let r = l.process::<Vec<MidiStorageElem>>(2, &[Vec::new()], std::slice::from_mut(&mut out));
        let_assert!(Ok(()) = r);
        check!(l.length() == 2);
    }

    #[test]
    fn planned_transition_still_works_with_channels() {
        let mut l = loop_with_channel();
        l.set_sync_source(Some(SyncSourceState::default()));
        l.set_mode(L::Recording);
        l.set_length(10);
        l.plan_transition(L::Playing, Some(0), None);
        check!(l.mode() == L::Recording);
        l.trigger(true);
        check!(l.mode() == L::Playing);
    }
}
