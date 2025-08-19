use backend_bindings::{
    AudioChannel, AudioChannelState, AudioPort, ChannelMode, MidiChannel, MidiChannelState,
    MidiEvent, MidiPort,
};
use common::logging::macros::*;
use std::{any::Any, collections::HashMap};
shoop_log_unit!("Frontend.AnyChannel");
pub enum AnyBackendChannel {
    Audio(AudioChannel),
    Midi(MidiChannel),
}

impl AnyBackendChannel {
    pub fn audio_connect_input(&self, port: &AudioPort) {
        match self {
            AnyBackendChannel::Audio(audio_channel) => {
                audio_channel.connect_input(port);
            }
            AnyBackendChannel::Midi(_) => {
                error!("Attempted to connect MIDI channel to audio port, ignored.");
            }
        }
    }

    pub fn audio_connect_output(&self, port: &AudioPort) {
        match self {
            AnyBackendChannel::Audio(audio_channel) => {
                audio_channel.connect_output(port);
            }
            AnyBackendChannel::Midi(_) => {
                error!("Attempted to connect MIDI channel to audio port, ignored.");
            }
        }
    }

    pub fn audio_disconnect(&self, port: &AudioPort) {
        match self {
            AnyBackendChannel::Audio(audio_channel) => {
                audio_channel.disconnect(port);
            }
            AnyBackendChannel::Midi(_) => {
                error!("Attempted to disconnect MIDI channel from audio port, ignored.");
            }
        }
    }

    pub fn audio_load_data(&self, data: &[f32]) {
        match self {
            AnyBackendChannel::Audio(audio_channel) => {
                audio_channel.load_data(data);
            }
            AnyBackendChannel::Midi(_) => {
                error!("Attempted to load audio data to MIDI channel, ignored.");
            }
        }
    }

    pub fn audio_get_data(&self) -> Vec<f32> {
        match self {
            AnyBackendChannel::Audio(audio_channel) => {
                return audio_channel.get_data();
            }
            AnyBackendChannel::Midi(_) => {
                error!("Attempted to get audio data from MIDI channel, ignored.");
                return Vec::default();
            }
        }
    }

    pub fn audio_set_gain(&self, gain: f32) {
        match self {
            AnyBackendChannel::Audio(audio_channel) => {
                audio_channel.set_gain(gain);
            }
            AnyBackendChannel::Midi(_) => {
                error!("Attempted to set gain on MIDI channel, ignored.");
            }
        }
    }

    pub fn midi_get_data(&self) -> Vec<MidiEvent> {
        match self {
            AnyBackendChannel::Audio(_) => {
                error!("Attempted to get MIDI data from audio channel, ignored.");
                return Vec::default();
            }
            AnyBackendChannel::Midi(midi_channel) => {
                return midi_channel.get_all_midi_data();
            }
        }
    }

    pub fn midi_load_data(&self, msgs: &[MidiEvent]) {
        match self {
            AnyBackendChannel::Audio(_) => {
                error!("Attempted to load MIDI data to audio channel, ignored.");
            }
            AnyBackendChannel::Midi(midi_channel) => {
                midi_channel.load_all_midi_data(msgs);
            }
        }
    }

    pub fn midi_connect_input(&self, port: &MidiPort) {
        match self {
            AnyBackendChannel::Midi(midi_channel) => {
                midi_channel.connect_input(port);
            }
            AnyBackendChannel::Audio(_) => {
                error!("Attempted to connect audio channel to MIDI port, ignored.");
            }
        }
    }

    pub fn midi_connect_output(&self, port: &MidiPort) {
        match self {
            AnyBackendChannel::Midi(midi_channel) => {
                midi_channel.connect_output(port);
            }
            AnyBackendChannel::Audio(_) => {
                error!("Attempted to connect audio channel to MIDI port, ignored.");
            }
        }
    }

    pub fn midi_disconnect(&self, port: &MidiPort) {
        match self {
            AnyBackendChannel::Midi(midi_channel) => {
                midi_channel.disconnect(port);
            }
            AnyBackendChannel::Audio(_) => {
                error!("Attempted to disconnect audio channel from MIDI port, ignored.");
            }
        }
    }

    pub fn midi_reset_state_tracking(&self) {
        match self {
            AnyBackendChannel::Midi(midi_channel) => {
                midi_channel.reset_state_tracking();
            }
            AnyBackendChannel::Audio(_) => {
                error!("Attempted to reset MIDI state tracking on audio channel, ignored.");
            }
        }
    }

    pub fn get_state(&self) -> Result<AnyBackendChannelState, anyhow::Error> {
        match self {
            AnyBackendChannel::Audio(audio_channel) => {
                Ok(AnyBackendChannelState::from(audio_channel.get_state()?))
            }
            AnyBackendChannel::Midi(midi_channel) => {
                Ok(AnyBackendChannelState::from(midi_channel.get_state()?))
            }
        }
    }

    pub fn set_mode(&self, mode: ChannelMode) {
        match self {
            AnyBackendChannel::Audio(audio_channel) => {
                audio_channel.set_mode(mode);
            }
            AnyBackendChannel::Midi(midi_channel) => {
                midi_channel.set_mode(mode);
            }
        }
    }

    pub fn set_start_offset(&self, start_offset: i32) {
        match self {
            AnyBackendChannel::Audio(audio_channel) => {
                audio_channel.set_start_offset(start_offset);
            }
            AnyBackendChannel::Midi(midi_channel) => {
                midi_channel.set_start_offset(start_offset);
            }
        }
    }

    pub fn set_n_preplay_samples(&self, n_preplay_samples: u32) {
        match self {
            AnyBackendChannel::Audio(audio_channel) => {
                audio_channel.set_n_preplay_samples(n_preplay_samples);
            }
            AnyBackendChannel::Midi(midi_channel) => {
                midi_channel.set_n_preplay_samples(n_preplay_samples);
            }
        }
    }

    pub fn clear_data_dirty(&self) {
        match self {
            AnyBackendChannel::Audio(audio_channel) => {
                audio_channel.clear_data_dirty();
            }
            AnyBackendChannel::Midi(midi_channel) => {
                midi_channel.clear_data_dirty();
            }
        }
    }

    pub fn clear(&self, length: u32) {
        match self {
            AnyBackendChannel::Audio(audio_channel) => {
                audio_channel.clear(length);
            }
            AnyBackendChannel::Midi(midi_channel) => {
                midi_channel.clear();
            }
        }
    }

    pub fn push_state(&self, state: &AnyBackendChannelState) -> Result<(), anyhow::Error> {
        match self {
            AnyBackendChannel::Audio(chan) => {
                chan.set_gain(state.audio_gain);
                chan.set_mode(state.mode);
                chan.set_start_offset(state.start_offset);
                chan.set_n_preplay_samples(state.n_preplay_samples);
            }
            AnyBackendChannel::Midi(chan) => {
                chan.set_mode(state.mode);
                chan.set_start_offset(state.start_offset);
                chan.set_n_preplay_samples(state.n_preplay_samples);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct AnyBackendChannelState {
    pub mode: ChannelMode,
    pub length: u32,
    pub start_offset: i32,
    pub played_back_sample: Option<u32>,
    pub n_preplay_samples: u32,
    pub data_dirty: bool,
    pub audio_gain: f32,
    pub audio_output_peak: f32,
    pub n_events_triggered: u32,
    pub n_notes_active: u32,
}

impl Default for AnyBackendChannelState {
    fn default() -> Self {
        AnyBackendChannelState {
            mode: ChannelMode::Disabled,
            length: 0,
            start_offset: 0,
            played_back_sample: None,
            n_preplay_samples: 0,
            data_dirty: false,
            audio_gain: 1.0,
            audio_output_peak: 0.0,
            n_events_triggered: 0,
            n_notes_active: 0,
        }
    }
}

impl From<AudioChannelState> for AnyBackendChannelState {
    fn from(state: AudioChannelState) -> Self {
        AnyBackendChannelState {
            mode: state.mode,
            length: state.length,
            start_offset: state.start_offset,
            played_back_sample: state.played_back_sample,
            n_preplay_samples: state.n_preplay_samples,
            data_dirty: state.data_dirty,
            audio_gain: state.gain,
            audio_output_peak: state.output_peak,
            n_events_triggered: 0,
            n_notes_active: 0,
        }
    }
}

impl From<MidiChannelState> for AnyBackendChannelState {
    fn from(state: MidiChannelState) -> Self {
        AnyBackendChannelState {
            mode: state.mode,
            length: state.length,
            start_offset: state.start_offset,
            played_back_sample: state.played_back_sample,
            n_preplay_samples: state.n_preplay_samples,
            data_dirty: state.data_dirty,
            audio_gain: 1.0,
            audio_output_peak: 0.0,
            n_events_triggered: state.n_events_triggered,
            n_notes_active: state.n_notes_active,
        }
    }
}
