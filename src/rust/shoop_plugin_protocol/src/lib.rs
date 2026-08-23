#[cfg(all(test, target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::io::{Read, Write};

pub const PROTOCOL_VERSION: u32 = 3;
pub const MAX_AUDIO_CHANNELS: usize = 16;
pub const MAX_BLOCK_FRAMES: usize = 8192;
pub const MAX_MIDI_EVENTS_PER_BLOCK: usize = 1024;
pub const MAX_MIDI_BYTES_PER_BLOCK: usize = 8192;
pub const MAX_CONTROL_PAYLOAD_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_STATE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_ERROR_MESSAGE_BYTES: usize = 4096;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(
            Clone,
            Copy,
            Debug,
            Default,
            Deserialize,
            Eq,
            Hash,
            Ord,
            PartialEq,
            PartialOrd,
            Serialize,
        )]
        pub struct $name(pub u64);

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }
    };
}

id_type!(ChainId);
id_type!(ProcessGeneration);
id_type!(RequestId);
id_type!(BlockSequence);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum CarlaChainType {
    Rack,
    Patchbay,
    Patchbay16x,
}

impl CarlaChainType {
    pub const fn audio_channels(self) -> u16 {
        match self {
            Self::Rack | Self::Patchbay => 2,
            Self::Patchbay16x => 16,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum LifecycleState {
    #[default]
    Stopped,
    Starting,
    Running,
    Crashed,
    Restarting,
    Unavailable,
    Stopping,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum WorkerExitKind {
    #[default]
    None,
    Requested,
    UiClosed,
    StartupFailure,
    ProtocolFailure,
    UnexpectedExit,
    Unresponsive,
    ParentDisconnected,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ProtocolErrorCode {
    IncompatibleVersion,
    InvalidNonce,
    InvalidRequest,
    InvalidLifecycle,
    CapacityExceeded,
    HostUnavailable,
    HostFailure,
    Timeout,
    StaleGeneration,
    Internal,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProtocolError {
    pub code: ProtocolErrorCode,
    pub message: String,
}

impl ProtocolError {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.message.len() > MAX_ERROR_MESSAGE_BYTES {
            return Err(ValidationError::ErrorMessageTooLarge);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkerHello {
    pub protocol_version: u32,
    pub nonce: [u8; 32],
    pub generation: ProcessGeneration,
    pub max_audio_channels: u16,
    pub max_block_frames: u32,
    pub max_midi_events: u32,
    pub max_midi_bytes: u32,
    /// Loopback UDP port used only to wake the shared-memory consumer.
    pub notification_port: u16,
}

impl WorkerHello {
    pub fn current(nonce: [u8; 32], generation: ProcessGeneration) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            nonce,
            generation,
            max_audio_channels: MAX_AUDIO_CHANNELS as u16,
            max_block_frames: MAX_BLOCK_FRAMES as u32,
            max_midi_events: MAX_MIDI_EVENTS_PER_BLOCK as u32,
            max_midi_bytes: MAX_MIDI_BYTES_PER_BLOCK as u32,
            notification_port: 1,
        }
    }

    pub fn validate(
        &self,
        expected_nonce: &[u8; 32],
        expected_generation: ProcessGeneration,
    ) -> Result<(), ValidationError> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(ValidationError::IncompatibleVersion);
        }
        if &self.nonce != expected_nonce {
            return Err(ValidationError::InvalidNonce);
        }
        if self.generation != expected_generation {
            return Err(ValidationError::StaleGeneration);
        }
        if self.notification_port == 0 {
            return Err(ValidationError::InvalidNotificationPort);
        }
        if self.max_audio_channels < MAX_AUDIO_CHANNELS as u16
            || self.max_block_frames < MAX_BLOCK_FRAMES as u32
            || self.max_midi_events < MAX_MIDI_EVENTS_PER_BLOCK as u32
            || self.max_midi_bytes < MAX_MIDI_BYTES_PER_BLOCK as u32
        {
            return Err(ValidationError::IncompatibleCapacity);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ControlRequestKind {
    Handshake(WorkerHello),
    Instantiate {
        chain_type: CarlaChainType,
        sample_rate: u32,
        nominal_buffer_size: u32,
    },
    SetActive(bool),
    SetVisible(bool),
    SaveState,
    RestoreState(String),
    Status,
    Shutdown,
    Ping,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ControlRequest {
    pub request_id: RequestId,
    pub chain_id: ChainId,
    pub generation: ProcessGeneration,
    pub kind: ControlRequestKind,
}

impl ControlRequest {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.chain_id.0 == 0 || self.generation.0 == 0 || self.request_id.0 == 0 {
            return Err(ValidationError::InvalidIdentity);
        }
        if let ControlRequestKind::RestoreState(state) = &self.kind {
            if state.len() > MAX_STATE_BYTES {
                return Err(ValidationError::StateTooLarge);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum WorkerLatencyCertainty {
    Exact,
    Range,
    Estimated,
    ManualOnly,
    #[default]
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum WorkerLatencyDiagnostic {
    CarlaRackAggregate,
    CarlaPatchbayGraphRange,
    Manual,
    VersionMismatch,
    #[default]
    Unsupported,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkerLatencyObservation {
    pub minimum_frames: Option<u32>,
    pub maximum_frames: Option<u32>,
    pub certainty: WorkerLatencyCertainty,
    pub sample_rate: u32,
    pub revision: u64,
    pub diagnostic: WorkerLatencyDiagnostic,
}

impl WorkerLatencyObservation {
    pub fn validate(&self) -> Result<(), ValidationError> {
        match (self.minimum_frames, self.maximum_frames, self.certainty) {
            (Some(minimum), Some(maximum), WorkerLatencyCertainty::Exact)
                if minimum == maximum && self.sample_rate > 0 =>
            {
                Ok(())
            }
            (Some(minimum), Some(maximum), WorkerLatencyCertainty::Range)
                if minimum < maximum && self.sample_rate > 0 =>
            {
                Ok(())
            }
            (Some(minimum), Some(maximum), WorkerLatencyCertainty::Estimated)
                if minimum <= maximum && self.sample_rate > 0 =>
            {
                Ok(())
            }
            (None, None, WorkerLatencyCertainty::ManualOnly | WorkerLatencyCertainty::Unknown) => {
                Ok(())
            }
            _ => Err(ValidationError::InvalidLatencyObservation),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkerStatus {
    pub lifecycle: LifecycleState,
    pub exit_kind: WorkerExitKind,
    pub active: bool,
    pub visible: bool,
    pub ready: bool,
    pub generation: ProcessGeneration,
    pub processed_blocks: u64,
    pub deadline_misses: u64,
    pub midi_input_overflows: u64,
    pub midi_output_overflows: u64,
    pub stale_completions: u64,
    pub latency: WorkerLatencyObservation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ControlResponseKind {
    Handshake(WorkerHello),
    Ack,
    State(String),
    Status(WorkerStatus),
    Error(ProtocolError),
    Pong,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ControlResponse {
    pub request_id: RequestId,
    pub chain_id: ChainId,
    pub generation: ProcessGeneration,
    pub kind: ControlResponseKind,
}

impl ControlResponse {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.chain_id.0 == 0 || self.generation.0 == 0 || self.request_id.0 == 0 {
            return Err(ValidationError::InvalidIdentity);
        }
        match &self.kind {
            ControlResponseKind::State(state) if state.len() > MAX_STATE_BYTES => {
                Err(ValidationError::StateTooLarge)
            }
            ControlResponseKind::Error(error) => error.validate(),
            ControlResponseKind::Status(status) => status.latency.validate(),
            _ => Ok(()),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MidiEvent {
    pub frame_offset: u32,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PrototypeBlock {
    pub sequence: BlockSequence,
    pub generation: ProcessGeneration,
    pub frames: u32,
    pub audio_inputs: Vec<Vec<f32>>,
    pub midi_inputs: Vec<MidiEvent>,
}

impl PrototypeBlock {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.sequence.0 == 0 || self.generation.0 == 0 {
            return Err(ValidationError::InvalidIdentity);
        }
        if self.frames == 0 || self.frames as usize > MAX_BLOCK_FRAMES {
            return Err(ValidationError::InvalidFrameCount);
        }
        if self.audio_inputs.len() > MAX_AUDIO_CHANNELS
            || self
                .audio_inputs
                .iter()
                .any(|channel| channel.len() != self.frames as usize)
        {
            return Err(ValidationError::InvalidAudioLayout);
        }
        validate_midi(&self.midi_inputs, self.frames)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PrototypeBlockResult {
    pub sequence: BlockSequence,
    pub generation: ProcessGeneration,
    pub frames: u32,
    pub audio_outputs: Vec<Vec<f32>>,
    pub midi_outputs: Vec<MidiEvent>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum ParentToWorker {
    Control(ControlRequest),
    Process(PrototypeBlock),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum WorkerToParent {
    Control(ControlResponse),
    Process(PrototypeBlockResult),
}

impl PrototypeBlockResult {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.sequence.0 == 0 || self.generation.0 == 0 {
            return Err(ValidationError::InvalidIdentity);
        }
        if self.frames == 0 || self.frames as usize > MAX_BLOCK_FRAMES {
            return Err(ValidationError::InvalidFrameCount);
        }
        if self.audio_outputs.len() > MAX_AUDIO_CHANNELS
            || self
                .audio_outputs
                .iter()
                .any(|channel| channel.len() != self.frames as usize)
        {
            return Err(ValidationError::InvalidAudioLayout);
        }
        validate_midi(&self.midi_outputs, self.frames)
    }
}

fn validate_midi(events: &[MidiEvent], frames: u32) -> Result<(), ValidationError> {
    if events.len() > MAX_MIDI_EVENTS_PER_BLOCK {
        return Err(ValidationError::TooManyMidiEvents);
    }
    let mut bytes = 0usize;
    for event in events {
        if event.frame_offset >= frames {
            return Err(ValidationError::InvalidMidiOffset);
        }
        bytes = bytes.saturating_add(event.data.len());
        if bytes > MAX_MIDI_BYTES_PER_BLOCK {
            return Err(ValidationError::TooManyMidiBytes);
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValidationError {
    IncompatibleVersion,
    InvalidNonce,
    StaleGeneration,
    IncompatibleCapacity,
    InvalidIdentity,
    InvalidNotificationPort,
    InvalidFrameCount,
    InvalidAudioLayout,
    InvalidMidiOffset,
    TooManyMidiEvents,
    TooManyMidiBytes,
    StateTooLarge,
    ErrorMessageTooLarge,
    InvalidLatencyObservation,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ValidationError {}

#[derive(Debug)]
pub enum WireError {
    Io(std::io::Error),
    PayloadTooLarge(usize),
    InvalidJson(serde_json::Error),
}

impl fmt::Display for WireError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "wire I/O failed: {error}"),
            Self::PayloadTooLarge(size) => write!(
                formatter,
                "wire payload has {size} bytes, maximum is {MAX_CONTROL_PAYLOAD_BYTES}"
            ),
            Self::InvalidJson(error) => write!(formatter, "invalid wire JSON: {error}"),
        }
    }
}

impl std::error::Error for WireError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::InvalidJson(error) => Some(error),
            Self::PayloadTooLarge(_) => None,
        }
    }
}

impl From<std::io::Error> for WireError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for WireError {
    fn from(error: serde_json::Error) -> Self {
        Self::InvalidJson(error)
    }
}

pub fn write_frame<T: Serialize>(writer: &mut impl Write, value: &T) -> Result<(), WireError> {
    let payload = serde_json::to_vec(value)?;
    if payload.len() > MAX_CONTROL_PAYLOAD_BYTES {
        return Err(WireError::PayloadTooLarge(payload.len()));
    }
    writer.write_all(&(payload.len() as u32).to_le_bytes())?;
    writer.write_all(&payload)?;
    writer.flush()?;
    Ok(())
}

pub fn read_frame<T: DeserializeOwned>(reader: &mut impl Read) -> Result<T, WireError> {
    let mut size = [0u8; 4];
    reader.read_exact(&mut size)?;
    let size = u32::from_le_bytes(size) as usize;
    if size > MAX_CONTROL_PAYLOAD_BYTES {
        return Err(WireError::PayloadTooLarge(size));
    }
    let mut payload = vec![0; size];
    reader.read_exact(&mut payload)?;
    Ok(serde_json::from_slice(&payload)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[shoop_wasm_test_support::shoop_test]
    fn handshake_rejects_wrong_version_nonce_generation_and_capacity() {
        let nonce = [7; 32];
        let generation = ProcessGeneration(4);
        let hello = WorkerHello::current(nonce, generation);
        assert_eq!(hello.validate(&nonce, generation), Ok(()));

        let mut wrong = hello.clone();
        wrong.protocol_version += 1;
        assert_eq!(
            wrong.validate(&nonce, generation),
            Err(ValidationError::IncompatibleVersion)
        );
        assert_eq!(
            hello.validate(&[8; 32], generation),
            Err(ValidationError::InvalidNonce)
        );
        assert_eq!(
            hello.validate(&nonce, ProcessGeneration(5)),
            Err(ValidationError::StaleGeneration)
        );
        wrong = hello.clone();
        wrong.notification_port = 0;
        assert_eq!(
            wrong.validate(&nonce, generation),
            Err(ValidationError::InvalidNotificationPort)
        );
        wrong = hello;
        wrong.max_block_frames = 64;
        assert_eq!(
            wrong.validate(&nonce, generation),
            Err(ValidationError::IncompatibleCapacity)
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn prototype_block_preserves_midi_bytes_and_offsets() {
        let block = PrototypeBlock {
            sequence: BlockSequence(2),
            generation: ProcessGeneration(3),
            frames: 64,
            audio_inputs: vec![vec![0.25; 64], vec![-0.5; 64]],
            midi_inputs: vec![
                MidiEvent {
                    frame_offset: 3,
                    data: vec![0x90, 60, 100],
                },
                MidiEvent {
                    frame_offset: 63,
                    data: vec![0x80, 60, 0],
                },
            ],
        };
        block.validate().expect("valid block");
        let encoded = serde_json::to_vec(&block).expect("serialize block");
        let decoded: PrototypeBlock = serde_json::from_slice(&encoded).expect("deserialize block");
        assert_eq!(decoded, block);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn prototype_block_rejects_every_capacity_boundary_violation() {
        let base = PrototypeBlock {
            sequence: BlockSequence(1),
            generation: ProcessGeneration(1),
            frames: 2,
            audio_inputs: vec![vec![0.0; 2]],
            midi_inputs: vec![],
        };
        assert_eq!(base.validate(), Ok(()));

        let mut invalid = base.clone();
        invalid.frames = MAX_BLOCK_FRAMES as u32 + 1;
        assert_eq!(invalid.validate(), Err(ValidationError::InvalidFrameCount));

        invalid = base.clone();
        invalid.audio_inputs[0].pop();
        assert_eq!(invalid.validate(), Err(ValidationError::InvalidAudioLayout));

        invalid = base.clone();
        invalid.midi_inputs.push(MidiEvent {
            frame_offset: 2,
            data: vec![0x90],
        });
        assert_eq!(invalid.validate(), Err(ValidationError::InvalidMidiOffset));

        invalid = base.clone();
        invalid.midi_inputs = (0..=MAX_MIDI_EVENTS_PER_BLOCK)
            .map(|_| MidiEvent {
                frame_offset: 0,
                data: vec![],
            })
            .collect();
        assert_eq!(invalid.validate(), Err(ValidationError::TooManyMidiEvents));

        invalid = base;
        invalid.midi_inputs.push(MidiEvent {
            frame_offset: 0,
            data: vec![0; MAX_MIDI_BYTES_PER_BLOCK + 1],
        });
        assert_eq!(invalid.validate(), Err(ValidationError::TooManyMidiBytes));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn control_messages_reject_zero_identity_and_oversized_payloads() {
        let mut request = ControlRequest {
            request_id: RequestId(1),
            chain_id: ChainId(2),
            generation: ProcessGeneration(3),
            kind: ControlRequestKind::Ping,
        };
        assert_eq!(request.validate(), Ok(()));
        request.request_id = RequestId(0);
        assert_eq!(request.validate(), Err(ValidationError::InvalidIdentity));

        request.request_id = RequestId(1);
        request.kind = ControlRequestKind::RestoreState("x".repeat(MAX_STATE_BYTES + 1));
        assert_eq!(request.validate(), Err(ValidationError::StateTooLarge));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn worker_latency_status_round_trips_and_rejects_inconsistent_ranges() {
        let latency = WorkerLatencyObservation {
            minimum_frames: Some(7),
            maximum_frames: Some(7),
            certainty: WorkerLatencyCertainty::Exact,
            sample_rate: 48_000,
            revision: 4,
            diagnostic: WorkerLatencyDiagnostic::CarlaRackAggregate,
        };
        assert_eq!(latency.validate(), Ok(()));
        let response = ControlResponse {
            request_id: RequestId(1),
            chain_id: ChainId(2),
            generation: ProcessGeneration(3),
            kind: ControlResponseKind::Status(WorkerStatus {
                latency,
                ..Default::default()
            }),
        };
        assert_eq!(response.validate(), Ok(()));
        assert!(serde_json::to_vec(&response).unwrap().len() < MAX_CONTROL_PAYLOAD_BYTES);

        let mut invalid = response;
        if let ControlResponseKind::Status(status) = &mut invalid.kind {
            status.latency.maximum_frames = Some(8);
        }
        assert_eq!(
            invalid.validate(),
            Err(ValidationError::InvalidLatencyObservation)
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn framed_wire_round_trip_and_rejects_oversized_or_malformed_input() {
        let request = ControlRequest {
            request_id: RequestId(1),
            chain_id: ChainId(2),
            generation: ProcessGeneration(3),
            kind: ControlRequestKind::Ping,
        };
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &request).expect("write frame");
        let decoded: ControlRequest = read_frame(&mut bytes.as_slice()).expect("read frame");
        assert_eq!(decoded, request);

        let mut oversized = ((MAX_CONTROL_PAYLOAD_BYTES + 1) as u32)
            .to_le_bytes()
            .to_vec();
        oversized.extend_from_slice(b"{}");
        assert!(matches!(
            read_frame::<ControlRequest>(&mut oversized.as_slice()),
            Err(WireError::PayloadTooLarge(_))
        ));

        let mut malformed = 1u32.to_le_bytes().to_vec();
        malformed.push(b'{');
        assert!(matches!(
            read_frame::<ControlRequest>(&mut malformed.as_slice()),
            Err(WireError::InvalidJson(_))
        ));
    }
}
