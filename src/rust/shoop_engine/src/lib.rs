#![cfg(not(feature = "prebuild"))]

//! Realtime looping engine: graph, loops, channels, ports.
//!
//! Pure logic plus the application-facing backend driver interface used by the frontend.

#[cfg(all(test, target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

#[cfg(any(feature = "app_backend", feature = "native_audio_backend"))]
pub mod app_backend;
pub mod audio_channel;
pub mod audio_midi_loop;
pub mod basic_loop;
pub mod buffer_queue;
#[cfg(feature = "carla")]
pub mod carla_native;
pub mod carla_processor;
#[cfg(not(target_arch = "wasm32"))]
pub mod carla_shared_memory;
#[cfg(all(feature = "carla", not(target_arch = "wasm32")))]
pub mod carla_subprocess;
pub mod channel_mode;
pub mod chunked_samples;
pub mod composite_plan;
pub mod composite_runtime;
pub mod composite_semantics;
pub mod composite_timeline;
pub mod content_snapshot;
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
pub mod graph_scheduler;
pub mod internal_audio_port;
pub mod loop_mode;
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
pub mod oxisynth;
pub mod pending_midi_control;
pub mod port;
pub mod profiling;
pub mod realtime_alloc_guard;
pub mod realtime_lock_guard;
pub mod resample;
pub mod session;
pub mod state;
pub mod state_mirror;
pub mod wave_generator;

pub use audio_channel::{AudioChannel, ChannelError, PreparedAudioChannelData};
pub use audio_midi_loop::{AudioMidiLoop, LoopError};
pub use basic_loop::{BasicLoop, PoiFlags, PointOfInterest, SyncSourceState};
pub use buffer_queue::{BufferQueue, Snapshot};
pub use channel_mode::{channel_process_params, ChannelMode, ProcessFlags};
pub use chunked_samples::ChunkedSamples;
pub use composite_plan::{
    compile_composite_plan, CompiledActionRange, CompiledChildMode, CompiledCompositeKind,
    CompiledCompositePlan, CompiledDesiredState, CompiledPlanAction, CompiledPlanActionKind,
    CompositeCompileError, CompositeDependency, CompositeEntry, CompositePlanDescriptor,
    CompositePlanLimits, CompositeSection, CompositeTimeline, LoopIdentity, LoopTargetCatalog,
    LoopTargetKind, LoopTargetMetadata, MAX_COMPOSITE_BOUNDARY_OUTPUTS, MAX_COMPOSITE_TARGETS,
};
pub use composite_runtime::{
    ActiveCompositeChild, CompositePlanReplacement, CompositeRuntime, CompositeRuntimeCounters,
    CompositeRuntimeError, CompositeRuntimeFault, CompositeTargetAction, CompositeTargetTransition,
    CompositeTransitionBatch, PendingCompositeTransition,
};
pub use composite_semantics::{
    classify_plan_modes, command_disposition, countdown_execution_boundary, dependency_order,
    empty_child_action, entry_duration, half_open_interval_contains,
    nested_iteration_zero_is_same_sample, normalize_coincident_schedule_actions,
    overflow_disposition, pass_end, plan_activation, plan_can_enter_running,
    records_this_occurrence, resolve_target, seek_cycle_offset, source_emits_due_action,
    valid_seek_iteration, BoundaryPhase, CommandDisposition, CommandTiming, CompiledBoundaryAction,
    CompositeKind, DependencyError, DurationError, EmptyChildAction, IntentOrigin, IntentPriority,
    ModePlanError, OverflowDisposition, OverflowSite, PassEnd, PlanActivation, RuntimeStatus,
    TargetIdentity, TargetResolution, TimestampRelation, BOUNDARY_PHASE_ORDER,
};
pub use composite_timeline::{
    AcceptedTimelineControl, BoundaryIntent, BoundaryIntentOrigin, BoundaryTargetAction,
    BoundaryTraceEntry, CompositeBoundaryTimeline, CompositeTimelineBuildError,
    CompositeTimelineControlError, CompositeTimelineCounters, CompositeTimelineFault,
    CompositeTimelineFaultRecord, CompositeTimelineLimits, CompositeTimelineNode,
    CompositeTimelineNodeState, MAX_COMPOSITE_CONTROLS,
};
pub use content_snapshot::{
    AudioContentSnapshot, AudioSnapshotMetadata, ContentMutation, ContentRevision,
    CurrentDataError, MidiContentSnapshot, MidiSnapshotMetadata, SnapshotCurrentness, SnapshotRead,
    StaleReason,
};
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
pub use engine::{
    split, wait_for_command, wait_for_result, Command, CommandReservation, CommandSequence,
    CompositeTraceSnapshot, Engine, EngineHandle, SendError, Stats, WaitError,
    DEFAULT_WAIT_TIMEOUT,
};
pub use fx_chain::{FXChainState, FXChainType};
pub use graph::{processing_order, GraphError, NodeIdx, NodeSpec};
pub use graph_build::{ChannelDesc, GraphDesc, LoopDesc, PortDesc};
pub use internal_audio_port::InternalAudioPort;
pub use loop_mode::LoopMode;
pub use midi_buffering_input_port::MidiBufferingInputPort;
#[cfg(any(feature = "app_backend", feature = "native_audio_backend"))]
pub use midi_channel::PreparedMidiChannelData;
pub use midi_channel::{MidiChannel, MidiChannelError};
pub use midi_event::MidiEvent;
pub use midi_port::MidiPort;
pub use midi_ringbuffer::MidiRingbuffer;
pub use midi_sorting_buffer::MidiSortingBuffer;
pub use midi_state::{MidiStateTracker, TrackWhat};
pub use midi_storage::{Cursor, CursorFindResult, MidiStorage, MidiStorageElem, TruncateSide};
pub use multichannel_audio::{MultichannelAudio, MultichannelAudioError};
pub use pending_midi_control::{PendingMidiControlState, MAX_PENDING_MIDI_CONTROLS};
pub use port::{
    AudioPort, PortConnectability, PortConnectabilityKind, PortDataType, PortDirection,
};
pub use profiling::{ProfilingReport, ProfilingReportItem};
pub use session::{
    build_schedule, AudioRingbufferAdoption, AudioRingbufferAdoptionChannelShape,
    AudioRingbufferAdoptionShape, Port, PreparedAudioRingbufferAdoptionChannel, PreparedSchedule,
    ReclaimedCompositeTimeline, RejectedCompositeTimeline, Session, SessionError, Topology,
    MAX_AUDIO_RINGBUFFER_ADOPTIONS, MAX_AUDIO_RINGBUFFER_ADOPTION_CHANNELS,
};
pub use state::{
    AudioChannelState, AudioPortState, LatestMidiMessage, LoopState, MidiChannelState,
    MidiPortState,
};
pub use state_mirror::{
    AudioChannelStateMirror, AudioPortStateMirror, CompositeStateMirror,
    CompositeStateMirrorSnapshot, LoopStateMirror, MidiChannelStateMirror, MidiPortStateMirror,
};
