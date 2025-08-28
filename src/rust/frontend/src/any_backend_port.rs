use backend_bindings::{
    AudioDriver, AudioPort, AudioPortState, BackendSession, MidiEvent, MidiPort, MidiPortState,
    PortConnectability, PortDataType, PortDirection,
};
use common::logging::macros::*;
use std::collections::HashMap;
shoop_log_unit!("Frontend.AnyPort");
pub enum AnyBackendPort {
    Audio(AudioPort),
    Midi(MidiPort),
}

impl AnyBackendPort {
    pub fn new_driver_port(
        port_type: PortDataType,
        backend_session: &BackendSession,
        audio_driver: &AudioDriver,
        name_hint: &str,
        direction: &PortDirection,
        min_n_ringbuffer_samples: u32,
    ) -> Result<Self, anyhow::Error> {
        match port_type {
            PortDataType::Audio => Ok(AnyBackendPort::Audio(AudioPort::new_driver_port(
                backend_session,
                audio_driver,
                name_hint,
                direction,
                min_n_ringbuffer_samples,
            )?)),
            PortDataType::Midi => Ok(AnyBackendPort::Midi(MidiPort::new_driver_port(
                backend_session,
                audio_driver,
                name_hint,
                direction,
                min_n_ringbuffer_samples,
            )?)),
            _ => Err(anyhow::Error::msg("Invalid port type")),
        }
    }

    pub fn input_connectability(&self) -> PortConnectability {
        match self {
            AnyBackendPort::Audio(port) => port.input_connectability(),
            AnyBackendPort::Midi(port) => port.input_connectability(),
        }
    }

    pub fn output_connectability(&self) -> PortConnectability {
        match self {
            AnyBackendPort::Audio(port) => port.output_connectability(),
            AnyBackendPort::Midi(port) => port.output_connectability(),
        }
    }

    pub fn get_state(&self) -> Result<AnyBackendPortState, anyhow::Error> {
        match self {
            AnyBackendPort::Audio(port) => Ok(AnyBackendPortState::from(port.get_state()?)),
            AnyBackendPort::Midi(port) => Ok(AnyBackendPortState::from(port.get_state()?)),
        }
    }

    pub fn push_state(&self, state: &AnyBackendPortState) -> Result<(), anyhow::Error> {
        match self {
            AnyBackendPort::Audio(port) => {
                port.set_gain(state.gain);
                port.set_muted(state.muted != 0);
                port.set_passthrough_muted(state.passthrough_muted != 0);
            }
            AnyBackendPort::Midi(port) => {
                port.set_muted(state.muted != 0);
                port.set_passthrough_muted(state.passthrough_muted != 0);
            }
        }
        Ok(())
    }

    pub fn set_gain(&self, gain: f32) {
        match self {
            AnyBackendPort::Audio(port) => port.set_gain(gain),
            AnyBackendPort::Midi(_) => error!("Attempted to set gain on a Midi port, ignored."),
        }
    }

    pub fn set_muted(&self, muted: bool) {
        match self {
            AnyBackendPort::Audio(port) => port.set_muted(muted),
            AnyBackendPort::Midi(port) => port.set_muted(muted),
        }
    }

    pub fn set_passthrough_muted(&self, passthrough_muted: bool) {
        match self {
            AnyBackendPort::Audio(port) => port.set_passthrough_muted(passthrough_muted),
            AnyBackendPort::Midi(port) => port.set_passthrough_muted(passthrough_muted),
        }
    }

    pub fn connect_internal(&self, other: &AnyBackendPort) {
        match self {
            AnyBackendPort::Audio(port) => {
                if let AnyBackendPort::Audio(other_audio) = other {
                    port.connect_internal(other_audio);
                } else {
                    error!("Attempted to internally connect audio port to a midi port");
                }
            }
            AnyBackendPort::Midi(port) => {
                if let AnyBackendPort::Midi(other_midi) = other {
                    port.connect_internal(other_midi);
                } else {
                    error!("Attempted to internally connect midi port to an audio port");
                }
            }
        }
    }

    pub fn dummy_queue_audio_data(&self, data: &[f32]) {
        match self {
            AnyBackendPort::Audio(port) => port.dummy_queue_data(data),
            AnyBackendPort::Midi(_) => {
                error!("Attempted to queue audio data on a midi port");
            }
        }
    }

    pub fn dummy_dequeue_audio_data(&self, n_frames: u32) -> Vec<f32> {
        match self {
            AnyBackendPort::Audio(port) => port.dummy_dequeue_data(n_frames),
            AnyBackendPort::Midi(_) => {
                error!("Attempted tow dequeue audio data from a midi port");
                Vec::default()
            }
        }
    }

    pub fn dummy_request_data(&self, n_frames: u32) {
        match self {
            AnyBackendPort::Audio(port) => port.dummy_request_data(n_frames),
            AnyBackendPort::Midi(port) => port.dummy_request_data(n_frames),
        }
    }

    pub fn dummy_clear_queues(&self) {
        match self {
            AnyBackendPort::Midi(port) => port.dummy_clear_queues(),
            AnyBackendPort::Audio(_) => {
                error!("Attempted to clear queues on a audio port");
            }
        }
    }

    pub fn dummy_queue_midi_msg(&self, msg: &MidiEvent) {
        match self {
            AnyBackendPort::Midi(port) => port.dummy_queue_msg(msg),
            AnyBackendPort::Audio(_) => {
                error!("Attempted to queue midi message on a audio port");
            }
        }
    }

    pub fn dummy_queue_midi_msgs(&self, msgs: &[MidiEvent]) {
        match self {
            AnyBackendPort::Midi(port) => port.dummy_queue_msgs(msgs.to_vec()),
            AnyBackendPort::Audio(_) => {
                error!("Attempted to queue midi messages on a audio port");
            }
        }
    }

    pub fn dummy_dequeue_midi_msgs(&self) -> Vec<MidiEvent> {
        match self {
            AnyBackendPort::Midi(port) => port.dummy_dequeue_data(),
            AnyBackendPort::Audio(_) => {
                error!("Attempted to dequeue midi messages from a audio port");
                vec![]
            }
        }
    }

    pub fn get_connections_state(&self) -> HashMap<String, bool> {
        match self {
            AnyBackendPort::Midi(port) => port.get_connections_state(),
            AnyBackendPort::Audio(port) => port.get_connections_state(),
        }
    }

    pub fn connect_external_port(&self, name: &str) {
        match self {
            AnyBackendPort::Midi(port) => port.connect_external_port(name),
            AnyBackendPort::Audio(port) => port.connect_external_port(name),
        }
    }

    pub fn disconnect_external_port(&self, name: &str) {
        match self {
            AnyBackendPort::Midi(port) => port.disconnect_external_port(name),
            AnyBackendPort::Audio(port) => port.disconnect_external_port(name),
        }
    }

    pub fn set_ringbuffer_n_samples(&self, n_samples: u32) {
        match self {
            AnyBackendPort::Midi(port) => port.set_ringbuffer_n_samples(n_samples),
            AnyBackendPort::Audio(port) => port.set_ringbuffer_n_samples(n_samples),
        }
    }

    pub fn direction(&self) -> PortDirection {
        match self {
            AnyBackendPort::Midi(port) => port.direction(),
            AnyBackendPort::Audio(port) => port.direction(),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct AnyBackendPortState {
    pub input_peak: f32,
    pub output_peak: f32,
    pub gain: f32,
    pub n_input_events: u32,
    pub n_input_notes_active: u32,
    pub n_output_events: u32,
    pub n_output_notes_active: u32,
    pub muted: u32,
    pub passthrough_muted: u32,
    pub ringbuffer_n_samples: u32,
    pub name: String,
}

impl From<AudioPortState> for AnyBackendPortState {
    fn from(state: AudioPortState) -> Self {
        AnyBackendPortState {
            input_peak: state.input_peak,
            output_peak: state.output_peak,
            gain: state.gain,
            muted: state.muted,
            passthrough_muted: state.passthrough_muted,
            ringbuffer_n_samples: state.ringbuffer_n_samples,
            name: state.name.clone(),
            n_input_events: 0,
            n_input_notes_active: 0,
            n_output_events: 0,
            n_output_notes_active: 0,
        }
    }
}

impl From<MidiPortState> for AnyBackendPortState {
    fn from(state: MidiPortState) -> Self {
        AnyBackendPortState {
            input_peak: 0.0,
            output_peak: 0.0,
            gain: 0.0,
            muted: state.muted,
            passthrough_muted: state.passthrough_muted,
            ringbuffer_n_samples: state.ringbuffer_n_samples,
            name: state.name.clone(),
            n_input_events: state.n_input_events,
            n_input_notes_active: state.n_input_notes_active,
            n_output_events: state.n_output_events,
            n_output_notes_active: state.n_output_notes_active,
        }
    }
}
