use common::logging::macros::*;
use shoop_engine::app_backend::{AudioDriver, AudioPort, BackendSession, MidiPort};
use shoop_engine::{
    AudioPortState, CommandSequence, MidiEvent, MidiPortState, PortConnectability, PortDataType,
    PortDirection, SendError,
};
use std::collections::HashMap;
shoop_log_unit!("Frontend.AnyPort");
pub enum AnyBackendPort {
    Audio(AudioPort),
    Midi(MidiPort),
}

fn report_queue_result(result: Result<CommandSequence, SendError>) {
    if let Err(error) = result {
        error!("Could not queue engine command: {error}");
    }
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

    /// State as of the last published cycle, for the frame-rate poll.
    ///
    /// The update thread fans one signal out to every port, channel and loop, so this is
    /// mirror-only. Pending ports report that no state is available and retain their frontend
    /// defaults; they never fall back to a synchronous engine read.
    pub fn poll_state(&self) -> Result<AnyBackendPortState, anyhow::Error> {
        let polled = match self {
            AnyBackendPort::Audio(port) => port.poll_state().map(AnyBackendPortState::from),
            AnyBackendPort::Midi(port) => port.poll_state().map(AnyBackendPortState::from),
        };
        polled.ok_or_else(|| anyhow::anyhow!("port state is pending"))
    }

    pub fn push_state(&self, state: &AnyBackendPortState) -> Result<(), anyhow::Error> {
        match self {
            AnyBackendPort::Audio(port) => {
                port.set_gain(state.gain)?;
                port.set_muted(state.muted != 0)?;
                port.set_passthrough_muted(state.passthrough_muted != 0)?;
            }
            AnyBackendPort::Midi(port) => {
                port.set_muted(state.muted != 0)?;
                port.set_passthrough_muted(state.passthrough_muted != 0)?;
            }
        }
        Ok(())
    }

    pub fn set_gain(&self, gain: f32) {
        if let AnyBackendPort::Audio(port) = self {
            report_queue_result(port.set_gain(gain));
        } else {
            error!("Attempted to set gain on a Midi port, ignored.");
        }
    }

    pub fn set_muted(&self, muted: bool) {
        let result = match self {
            AnyBackendPort::Audio(port) => port.set_muted(muted),
            AnyBackendPort::Midi(port) => port.set_muted(muted),
        };
        report_queue_result(result);
    }

    pub fn set_passthrough_muted(&self, passthrough_muted: bool) {
        let result = match self {
            AnyBackendPort::Audio(port) => port.set_passthrough_muted(passthrough_muted),
            AnyBackendPort::Midi(port) => port.set_passthrough_muted(passthrough_muted),
        };
        report_queue_result(result);
    }

    pub fn connect_internal(&self, other: &AnyBackendPort) {
        match self {
            AnyBackendPort::Audio(port) => {
                if let AnyBackendPort::Audio(other_audio) = other {
                    report_queue_result(port.connect_internal(other_audio));
                } else {
                    error!("Attempted to internally connect audio port to a midi port");
                }
            }
            AnyBackendPort::Midi(port) => {
                if let AnyBackendPort::Midi(other_midi) = other {
                    report_queue_result(port.connect_internal(other_midi));
                } else {
                    error!("Attempted to internally connect midi port to an audio port");
                }
            }
        }
    }

    pub fn dummy_queue_audio_data(&self, data: &[f32]) {
        if let AnyBackendPort::Audio(port) = self {
            report_queue_result(port.dummy_queue_data(data));
        } else {
            error!("Attempted to queue audio data on a midi port");
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
        let result = match self {
            AnyBackendPort::Audio(port) => port.dummy_request_data(n_frames),
            AnyBackendPort::Midi(port) => port.dummy_request_data(n_frames),
        };
        report_queue_result(result);
    }

    pub fn dummy_clear_queues(&self) {
        if let AnyBackendPort::Midi(port) = self {
            report_queue_result(port.dummy_clear_queues());
        } else {
            error!("Attempted to clear queues on a audio port");
        }
    }

    pub fn dummy_queue_midi_msg(&self, msg: &MidiEvent) {
        if let AnyBackendPort::Midi(port) = self {
            report_queue_result(port.dummy_queue_msg(msg));
        } else {
            error!("Attempted to queue midi message on a audio port");
        }
    }

    pub fn dummy_queue_midi_msgs(&self, msgs: &[MidiEvent]) {
        if let AnyBackendPort::Midi(port) = self {
            report_queue_result(port.dummy_queue_msgs(msgs.to_vec()));
        } else {
            error!("Attempted to queue midi messages on a audio port");
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
        let result = match self {
            AnyBackendPort::Midi(port) => port.set_ringbuffer_n_samples(n_samples),
            AnyBackendPort::Audio(port) => port.set_ringbuffer_n_samples(n_samples),
        };
        report_queue_result(result);
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
            muted: state.muted as u32,
            passthrough_muted: state.passthrough_muted as u32,
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
            muted: state.muted as u32,
            passthrough_muted: state.passthrough_muted as u32,
            ringbuffer_n_samples: state.ringbuffer_n_samples,
            name: state.name.clone(),
            n_input_events: state.n_input_events,
            n_input_notes_active: state.n_input_notes_active,
            n_output_events: state.n_output_events,
            n_output_notes_active: state.n_output_notes_active,
        }
    }
}
