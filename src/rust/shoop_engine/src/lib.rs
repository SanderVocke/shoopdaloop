#![cfg(not(feature = "prebuild"))]

//! Realtime looping engine: graph, loops, channels, ports.
//!
//! Pure logic plus the application-facing backend driver interface used by the frontend.

#[cfg(feature = "app_backend")]
pub mod app_backend;
pub mod audio_channel;
pub mod audio_midi_loop;
pub mod basic_loop;
pub mod buffer_queue;
pub mod channel_mode;
pub mod chunked_samples;
pub mod control;
#[cfg(feature = "cpal")]
pub mod cpal_driver;
#[cfg(feature = "cpal")]
pub mod cpal_mock;
pub mod decoupled_midi_port;
pub mod driver;
pub mod dummy_driver;
pub mod dummy_midi_port;
pub mod dummy_port;
pub mod engine;
pub mod external_audio_port;
pub mod external_midi_port;
pub mod fx_chain;
pub mod graph;
pub mod graph_build;
pub mod internal_audio_port;
#[cfg(feature = "jack")]
pub mod jack_driver;
pub mod loop_mode;
#[cfg(feature = "lv2")]
pub mod lv2_carla;
pub mod midi;
pub mod midi_buffering_input_port;
pub mod midi_channel;
pub mod midi_event;
pub mod midi_port;
pub mod midi_ringbuffer;
pub mod midi_sorting_buffer;
pub mod midi_state;
pub mod midi_storage;
#[cfg(feature = "midir")]
pub mod midir_driver;
pub mod multichannel_audio;
pub mod port;
pub mod profiling;
pub mod realtime_alloc_guard;
pub mod resample;
pub mod session;
pub mod wave_generator;

pub use audio_channel::{AudioChannel, ChannelError};
pub use audio_midi_loop::{AudioMidiLoop, LoopError};
pub use basic_loop::{BasicLoop, PoiFlags, PointOfInterest, SyncSourceState};
pub use buffer_queue::{BufferQueue, Snapshot};
pub use channel_mode::{channel_process_params, ChannelMode, ProcessFlags};
pub use chunked_samples::ChunkedSamples;
pub use control::{AudioChannelState, AudioPortState, MidiChannelState, MidiPortState};
pub use decoupled_midi_port::DecoupledMidiPort;
pub use driver::{
    cpal_host_names, cpal_input_device_names, cpal_input_device_names_for_host,
    cpal_output_device_names, cpal_output_device_names_for_host, driver_type_supported,
    midir_input_port_names, midir_output_port_names, AudioDriverState, AudioDriverType,
    BackendSessionState,
};
pub use dummy_driver::{DriverMode, DriverSettings, DummyDriver};
pub use dummy_midi_port::DummyMidiPort;
pub use dummy_port::{DummyAudioPort, DummyExternalConnections, DummyPortError, PortId};
pub use engine::LoopState;
pub use fx_chain::{FXChainState, FXChainType};
pub use graph::{processing_order, GraphError, NodeIdx, NodeSpec};
pub use graph_build::{ChannelDesc, GraphDesc, LoopDesc, PortDesc};
pub use internal_audio_port::InternalAudioPort;
pub use loop_mode::LoopMode;
pub use midi_buffering_input_port::MidiBufferingInputPort;
pub use midi_channel::{MidiChannel, MidiChannelError};
pub use midi_event::MidiEvent;
pub use midi_port::MidiPort;
pub use midi_ringbuffer::MidiRingbuffer;
pub use midi_sorting_buffer::MidiSortingBuffer;
pub use midi_state::{MidiStateTracker, TrackWhat};
pub use midi_storage::{Cursor, CursorFindResult, MidiStorage, MidiStorageElem, TruncateSide};
pub use multichannel_audio::{MultichannelAudio, MultichannelAudioError};
pub use port::{
    AudioPort, PortConnectability, PortConnectabilityKind, PortDataType, PortDirection,
};
pub use profiling::{ProfilingReport, ProfilingReportItem};
pub use session::{Port, Session, SessionError};
