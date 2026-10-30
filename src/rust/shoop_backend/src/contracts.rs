//! Behaviour every `Backend` implementation shares, written against the public `Backend` API.
//!
//! A backend supplies its environment through [`ContractProfile`]. Parts that need a
//! capability the backend does not report are skipped, and the backend must refuse them.
//! After mutating, the contracts call `Backend::wait_idle` before they inspect `poll()`, so
//! an asynchronous backend must have confirmed every earlier request by then. Contracts that
//! need exactly rendered frames run through a [`TimingHarness`] instead.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use anyhow::Result;
use shoop_app_api::AudioDriverConfig;

use crate::BackendPortDataType::{Audio, Midi};
use crate::BackendPortDirection::{Input, Output};
use crate::{
    Backend, BackendAudioChannelUpdate, BackendAudioContent, BackendBusChannelId,
    BackendBusControl, BackendBusFxControl, BackendBusId, BackendBusRequest, BackendCapability,
    BackendChannelMode, BackendConfirmedLink, BackendGrabRequest, BackendHostPortDescriptor,
    BackendLoopContent, BackendLoopContentUpdate, BackendLoopId, BackendLoopMode, BackendLoopState,
    BackendMidiContent, BackendMidiEvent, BackendMixerLink, BackendPortDataType,
    BackendPortDescriptor, BackendPortDirection, BackendPortId, BackendPortOwner, BackendPortRole,
    BackendProcessorLatencyAdjustment, BackendRecordingOffsetAdjustment, BackendSessionBus,
    BackendSessionBusChannel, BackendSessionData, BackendSessionMixerRoute, BackendSessionPort,
    BackendSessionTrack, BackendSnapshot, BackendTrackControl, BackendTrackCreation,
    BackendTrackFxControl, BackendTrackId, BackendTrackState, BackendTrackTopology,
    DirectTrackRequest, EngineBackend, TrackRequest, MASTER_BUS_CHANNEL_LABELS, MASTER_BUS_ID,
    MASTER_BUS_NAME, WEB_AUDIO_CAPTURE_PORTS, WEB_AUDIO_DESTINATION_PORTS,
};

/// The host ports a backend offers the contracts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractProfile {
    /// A host audio source that track audio inputs connect to.
    pub audio_capture_port: String,
    /// A host audio sink that track audio outputs connect to.
    pub audio_playback_port: String,
    /// A host MIDI source, if the environment has one.
    pub midi_capture_port: Option<String>,
    /// A host MIDI sink, if the environment has one.
    pub midi_playback_port: Option<String>,
}

impl ContractProfile {
    /// The representative host ports of the fake and the local dummy backends.
    pub fn dummy_host() -> Self {
        Self {
            audio_capture_port: "system:capture_1".to_owned(),
            audio_playback_port: "system:playback_1".to_owned(),
            midi_capture_port: Some("controller:midi_out".to_owned()),
            midi_playback_port: Some("synth:midi_in".to_owned()),
        }
    }

    /// The Web Audio device ports, without Web MIDI endpoints.
    pub fn web_audio() -> Self {
        Self {
            audio_capture_port: WEB_AUDIO_CAPTURE_PORTS[0].to_owned(),
            audio_playback_port: WEB_AUDIO_DESTINATION_PORTS[0].to_owned(),
            midi_capture_port: None,
            midi_playback_port: None,
        }
    }
}

/// Runs every contract that needs no [`TimingHarness`]. The session contract replaces the
/// session, so it runs last.
pub fn backend_contracts(backend: &mut dyn Backend, profile: &ContractProfile) {
    backend_contract(backend);
    direct_track_contract(backend);
    connection_contract(backend, profile);
    capability_refusal_contract(backend);
    track_removal_contract(backend, profile);
    track_controls_contract(backend);
    mixer_contract(backend);
    clear_contract(backend);
    new_session_contract(backend, profile);
}

/// Runs every [`TimingHarness`] contract, each on a fresh harness.
pub fn harness_contracts<H: TimingHarness>(mut fresh: impl FnMut() -> H) {
    transition_timing_contract(&mut fresh());
    signal_chain_contract(&mut fresh());
    loop_playback_contract(&mut fresh());
}

fn create_track(
    backend: &mut dyn Backend,
    port_name_base: &str,
    audio_channels: u32,
    midi: bool,
    initial_loops: usize,
) -> BackendTrackCreation {
    backend
        .create_direct_track(DirectTrackRequest {
            port_name_base: port_name_base.to_owned(),
            audio_channels,
            midi,
            initial_loops,
        })
        .unwrap()
}

fn ports_with_role(created: &BackendTrackCreation, role: BackendPortRole) -> Vec<BackendPortId> {
    created
        .ports
        .iter()
        .filter(|port| port.role == role)
        .map(|port| port.id)
        .collect()
}

fn master_channels(backend: &mut dyn Backend) -> Vec<BackendBusChannelId> {
    backend
        .poll()
        .unwrap()
        .mixer
        .buses
        .get(&MASTER_BUS_ID)
        .expect("the Master bus is missing")
        .channels
        .iter()
        .map(|channel| channel.id)
        .collect()
}

fn port_with_role(created: &BackendTrackCreation, role: BackendPortRole) -> &BackendPortDescriptor {
    created
        .ports
        .iter()
        .find(|port| port.role == role)
        .unwrap_or_else(|| panic!("created track has no {role:?} port"))
}

/// Loops of a track record immediately and follow a sync source.
pub fn backend_contract(backend: &mut dyn Backend) {
    let created = backend
        .create_direct_track(DirectTrackRequest {
            port_name_base: "contract_loops".to_owned(),
            audio_channels: 1,
            midi: false,
            initial_loops: 2,
        })
        .unwrap();
    let sync = created.loops[0];
    let follower = created.loops[1];
    backend.wait_idle();
    backend
        .transition_loop(follower, BackendLoopMode::Recording, None)
        .unwrap();
    backend.wait_idle();
    let snapshot = backend.poll().unwrap();
    assert!(snapshot.loops.contains_key(&sync));
    assert_eq!(
        snapshot.loops.get(&follower).unwrap().mode,
        BackendLoopMode::Recording
    );
    backend.set_loop_sync_source(follower, Some(sync)).unwrap();
    backend.wait_idle();
}

/// Direct tracks expose their shape, controls, loop settings and loop content.
pub fn direct_track_contract(backend: &mut dyn Backend) {
    let created = backend
        .create_direct_track(DirectTrackRequest {
            port_name_base: "contract".to_owned(),
            audio_channels: 2,
            midi: true,
            initial_loops: 2,
        })
        .unwrap();
    backend
        .set_track_control(created.track_id, BackendTrackControl::OutputGainDb(-6.0))
        .unwrap();
    backend.set_loop_gain(created.loops[0], 0.5).unwrap();
    backend.set_loop_balance(created.loops[0], 0.25).unwrap();
    let injected = backend.inject_midi_input(
        created.track_id,
        &[BackendMidiEvent {
            time: 0,
            data: vec![0x90, 60, 100],
        }],
    );
    if backend
        .capabilities()
        .supports(BackendCapability::MidiInjection)
    {
        injected.unwrap();
    } else {
        assert!(injected.is_err(), "unreported MIDI injection was accepted");
    }
    let third = backend.add_loop_to_track(created.track_id).unwrap();
    backend.wait_idle();
    let snapshot = backend.poll().unwrap();
    let track = &snapshot.tracks[&created.track_id];
    assert_eq!(track.audio_channels, 2);
    assert!(track.midi);
    assert_eq!(track.output_gain_db, -6.0);
    assert!(snapshot.loops[&created.loops[0]].stereo);
    assert_eq!(snapshot.loops[&created.loops[0]].gain, 0.5);
    assert_eq!(snapshot.loops[&created.loops[0]].balance, 0.25);
    assert!(snapshot.loops.contains_key(&third));
    assert_eq!(
        backend
            .loop_audio_data(created.loops[0])
            .unwrap()
            .unwrap()
            .len(),
        2
    );
}

fn assert_host_port(
    host_ports: &BTreeMap<String, BackendHostPortDescriptor>,
    id: &str,
    data_type: BackendPortDataType,
    direction: BackendPortDirection,
) {
    let port = host_ports
        .get(id)
        .unwrap_or_else(|| panic!("host port {id} is missing"));
    assert_eq!(
        (port.data_type, port.direction),
        (data_type, direction),
        "host port {id}"
    );
}

fn assert_link_cycle(backend: &mut dyn Backend, port: &BackendPortDescriptor, host_port: &str) {
    let link = BackendConfirmedLink {
        application_port_id: port.id,
        host_port_id: host_port.to_owned(),
    };
    backend
        .set_port_connected(port.id, host_port, true)
        .unwrap();
    backend
        .set_port_connected(port.id, host_port, true)
        .unwrap();
    backend.wait_idle();
    let connections = backend.poll().unwrap().connections;
    assert!(
        connections.confirmed_links.contains(&link),
        "{} is not linked to {host_port}",
        port.name
    );
    backend
        .set_port_connected(port.id, host_port, false)
        .unwrap();
    backend.wait_idle();
    let connections = backend.poll().unwrap().connections;
    assert!(
        !connections.confirmed_links.contains(&link),
        "{} is still linked to {host_port}",
        port.name
    );
}

/// Track ports connect to the host ports of the profile and refuse unknown host ports.
pub fn connection_contract(backend: &mut dyn Backend, profile: &ContractProfile) {
    let created = backend
        .create_direct_track(DirectTrackRequest {
            port_name_base: "contract_connections".to_owned(),
            audio_channels: 1,
            midi: true,
            initial_loops: 1,
        })
        .unwrap();
    assert_eq!(created.ports.len(), 4);
    let audio_input = port_with_role(&created, BackendPortRole::AudioInput);
    assert_eq!(audio_input.direction, Input);
    assert_eq!(audio_input.data_type, Audio);
    backend.wait_idle();
    let snapshot = backend.poll().unwrap().connections;
    assert!(snapshot.available);
    assert_eq!(
        snapshot.application_ports.get(&audio_input.id),
        Some(audio_input)
    );
    let hosts = &snapshot.host_ports;
    assert_host_port(hosts, &profile.audio_capture_port, Audio, Output);
    assert_host_port(hosts, &profile.audio_playback_port, Audio, Input);
    if let Some(midi_capture) = &profile.midi_capture_port {
        assert_host_port(hosts, midi_capture, Midi, Output);
    }
    if let Some(midi_playback) = &profile.midi_playback_port {
        assert_host_port(hosts, midi_playback, Midi, Input);
    }
    assert!(
        !hosts.keys().any(|id| id.contains("contract_connections")),
        "application ports are listed as host ports"
    );

    assert_link_cycle(backend, audio_input, &profile.audio_capture_port);
    let audio_output = port_with_role(&created, BackendPortRole::AudioOutput);
    assert_link_cycle(backend, audio_output, &profile.audio_playback_port);
    if let Some(midi_capture) = &profile.midi_capture_port {
        let midi_input = port_with_role(&created, BackendPortRole::MidiInput);
        assert_link_cycle(backend, midi_input, midi_capture);
    }
    if let Some(midi_playback) = &profile.midi_playback_port {
        let midi_output = port_with_role(&created, BackendPortRole::MidiOutput);
        assert_link_cycle(backend, midi_output, midi_playback);
    }
    assert!(backend
        .set_port_connected(audio_input.id, "missing:endpoint", true)
        .is_err());
}

/// Every capability the backend does not report is refused.
pub fn capability_refusal_contract(backend: &mut dyn Backend) {
    let unsupported = backend.capabilities().unsupported().collect::<Vec<_>>();
    if unsupported.is_empty() {
        return;
    }
    let created = backend
        .create_direct_track(DirectTrackRequest {
            port_name_base: "contract_refusals".to_owned(),
            audio_channels: 1,
            midi: true,
            initial_loops: 1,
        })
        .unwrap();
    backend.wait_idle();
    for capability in unsupported {
        assert!(
            attempt_capability(backend, capability, created.track_id, created.loops[0]).is_err(),
            "unreported {capability:?} was accepted"
        );
    }
}

/// Performs one representative operation that needs `capability`.
pub(crate) fn attempt_capability(
    backend: &mut dyn Backend,
    capability: BackendCapability,
    track_id: BackendTrackId,
    loop_id: BackendLoopId,
) -> Result<()> {
    match capability {
        BackendCapability::SessionCapture => backend.capture_session().map(drop),
        BackendCapability::SessionContentLoad => {
            let mut session = backend.capture_session()?;
            let content = session
                .tracks
                .iter_mut()
                .flat_map(|track| &mut track.loops)
                .find(|content| content.source_id == loop_id.raw())
                .unwrap();
            content.length = 4;
            backend.replace_session(&session).map(drop)
        }
        BackendCapability::CompositeLoops => backend.create_composite_loop().map(drop),
        BackendCapability::Grab => backend.grab_loops(&[BackendGrabRequest {
            loop_id,
            reverse_start_cycle: None,
            cycles_length: None,
            go_to_cycle: None,
            go_to_mode: BackendLoopMode::Playing,
        }]),
        BackendCapability::ReplacingMode => {
            backend.transition_loop(loop_id, BackendLoopMode::Replacing, None)
        }
        BackendCapability::DryWetModes => {
            backend.transition_loop(loop_id, BackendLoopMode::PlayingDryThroughWet, None)
        }
        BackendCapability::DryWetTracks => backend
            .create_track(TrackRequest {
                port_name_base: "dry_wet".to_owned(),
                topology: BackendTrackTopology::DryWetExternal {
                    dry_audio_channels: 1,
                    wet_audio_channels: 1,
                    dry_midi: false,
                },
                initial_loops: 1,
            })
            .map(drop),
        BackendCapability::TrackFx => {
            backend.set_track_fx_control(track_id, BackendTrackFxControl::SetActive(true))
        }
        BackendCapability::BusFx => {
            backend.set_bus_fx_control(MASTER_BUS_ID, BackendBusFxControl::SetActive(true))
        }
        BackendCapability::LatencyCompensation => backend.set_track_latency(
            track_id,
            BackendRecordingOffsetAdjustment::ManualOverride(0),
            BackendProcessorLatencyAdjustment::ManualOverride,
            0,
        ),
        BackendCapability::TakeAlignment => backend.set_take_alignment(loop_id, 0),
        BackendCapability::LoopContentImport => backend.replace_loop_content(
            loop_id,
            &BackendLoopContentUpdate {
                audio: vec![BackendAudioChannelUpdate {
                    channel: 0,
                    samples: vec![0.0; 4],
                    start_offset: None,
                    capture_alignment_frames: None,
                    preplay: None,
                }],
                midi: Vec::new(),
                length: Some(4),
            },
        ),
        BackendCapability::LoopTimingEdits => backend.set_loop_length(loop_id, 4),
        BackendCapability::BusManagement => backend
            .create_bus(BackendBusRequest {
                name: "Cue".to_owned(),
                channel_count: 2,
                fx: None,
            })
            .map(drop),
        BackendCapability::AudioDriverSwitching => backend
            .preflight_audio_driver(&AudioDriverConfig::default())
            .map(drop),
        BackendCapability::MidiInjection => backend.inject_midi_input(
            track_id,
            &[BackendMidiEvent {
                time: 0,
                data: vec![0x90, 60, 100],
            }],
        ),
    }
}

/// A removed track leaves no loops, ports or links behind, and its identities stay unused.
pub fn track_removal_contract(backend: &mut dyn Backend, profile: &ContractProfile) {
    let removed = create_track(backend, "contract_removed", 1, true, 2);
    let kept = create_track(backend, "contract_kept", 1, false, 1);
    let input = port_with_role(&removed, BackendPortRole::AudioInput).id;
    let output = port_with_role(&removed, BackendPortRole::AudioOutput).id;
    backend
        .set_port_connected(input, &profile.audio_capture_port, true)
        .unwrap();
    let master = master_channels(backend);
    backend.set_mixer_route(output, master[0], true).unwrap();
    backend.remove_track(removed.track_id).unwrap();
    backend.wait_idle();

    let snapshot = backend.poll().unwrap();
    let removed_ports = removed
        .ports
        .iter()
        .map(|port| port.id)
        .collect::<BTreeSet<_>>();
    assert!(!snapshot.tracks.contains_key(&removed.track_id));
    assert!(snapshot.tracks.contains_key(&kept.track_id));
    assert!(removed
        .loops
        .iter()
        .all(|loop_id| !snapshot.loops.contains_key(loop_id)));
    assert!(snapshot.loops.contains_key(&kept.loops[0]));
    let connections = &snapshot.connections;
    assert!(removed_ports
        .iter()
        .all(|port| !connections.application_ports.contains_key(port)));
    assert!(connections
        .confirmed_links
        .iter()
        .all(|link| !removed_ports.contains(&link.application_port_id)));
    assert!(snapshot
        .mixer
        .confirmed_links
        .iter()
        .all(|link| !removed_ports.contains(&link.source_port_id)));

    assert!(backend
        .set_track_control(removed.track_id, BackendTrackControl::OutputMute(true))
        .is_err());
    assert!(backend
        .transition_loop(removed.loops[0], BackendLoopMode::Recording, None)
        .is_err());
    assert!(backend
        .set_port_connected(input, &profile.audio_capture_port, true)
        .is_err());
    let created = create_track(backend, "contract_removed", 1, true, 2);
    assert_ne!(created.track_id, removed.track_id);
    assert!(created
        .loops
        .iter()
        .all(|loop_id| !removed.loops.contains(loop_id)));
    assert!(created
        .ports
        .iter()
        .all(|port| !removed_ports.contains(&port.id)));
}

/// Track input and output controls are confirmed in the snapshot.
pub fn track_controls_contract(backend: &mut dyn Backend) {
    let created = create_track(backend, "contract_controls", 2, false, 1);
    backend.wait_idle();
    let monitoring = !backend.poll().unwrap().tracks[&created.track_id].input_monitoring;
    for control in [
        BackendTrackControl::OutputGainDb(-3.0),
        BackendTrackControl::OutputBalance(-0.5),
        BackendTrackControl::OutputMute(true),
        BackendTrackControl::InputGainDb(-9.0),
        BackendTrackControl::InputBalance(0.5),
        BackendTrackControl::InputMonitoring(monitoring),
    ] {
        backend
            .set_track_control(created.track_id, control)
            .unwrap();
    }
    backend.wait_idle();
    let snapshot = backend.poll().unwrap();
    let track = &snapshot.tracks[&created.track_id];
    assert_eq!(
        (
            track.output_gain_db,
            track.output_balance,
            track.output_muted
        ),
        (-3.0, -0.5, true)
    );
    assert_eq!(
        (
            track.input_gain_db,
            track.input_balance,
            track.input_monitoring
        ),
        (-9.0, 0.5, monitoring)
    );
}

/// Track outputs route to the stereo Master bus, whose controls are confirmed in the snapshot.
pub fn mixer_contract(backend: &mut dyn Backend) {
    let snapshot = backend.poll().unwrap();
    let master = snapshot
        .mixer
        .buses
        .get(&MASTER_BUS_ID)
        .expect("the Master bus is missing");
    assert_eq!(master.name, MASTER_BUS_NAME);
    let channels = master_channels(backend);
    assert_eq!(channels.len(), 2);
    let created = create_track(backend, "contract_mixer", 2, false, 1);
    let links = ports_with_role(&created, BackendPortRole::AudioOutput)
        .into_iter()
        .zip(channels.iter().copied())
        .map(
            |(source_port_id, destination_channel_id)| BackendMixerLink {
                source_port_id,
                destination_channel_id,
            },
        )
        .collect::<Vec<_>>();
    for link in &links {
        backend
            .set_mixer_route(link.source_port_id, link.destination_channel_id, true)
            .unwrap();
    }
    let input = port_with_role(&created, BackendPortRole::AudioInput).id;
    assert!(backend.set_mixer_route(input, channels[0], true).is_err());
    for control in [
        BackendBusControl::GainDb(-6.0),
        BackendBusControl::Balance(0.25),
        BackendBusControl::Mute(true),
    ] {
        backend.set_bus_control(MASTER_BUS_ID, control).unwrap();
    }
    backend.wait_idle();
    let snapshot = backend.poll().unwrap();
    assert!(links
        .iter()
        .all(|link| snapshot.mixer.confirmed_links.contains(link)));
    let master = &snapshot.mixer.buses[&MASTER_BUS_ID];
    assert_eq!(
        (master.gain_db, master.balance, master.muted),
        (-6.0, 0.25, true)
    );

    backend
        .set_mixer_route(
            links[0].source_port_id,
            links[0].destination_channel_id,
            false,
        )
        .unwrap();
    for control in [
        BackendBusControl::GainDb(0.0),
        BackendBusControl::Balance(0.0),
        BackendBusControl::Mute(false),
    ] {
        backend.set_bus_control(MASTER_BUS_ID, control).unwrap();
    }
    backend.wait_idle();
    let snapshot = backend.poll().unwrap();
    assert!(!snapshot.mixer.confirmed_links.contains(&links[0]));
    assert!(snapshot.mixer.confirmed_links.contains(&links[1]));
    let master = &snapshot.mixer.buses[&MASTER_BUS_ID];
    assert_eq!(
        (master.gain_db, master.balance, master.muted),
        (0.0, 0.0, false)
    );
}

/// Clearing a recorded loop stops and empties it, and it can record again. The loop records
/// while `advance` renders; [`loop_playback_contract`] checks the recorded content exactly.
pub fn clear_contract(backend: &mut dyn Backend) {
    let created = create_track(backend, "contract_clear", 1, false, 1);
    let loop_id = created.loops[0];
    backend
        .transition_loop(loop_id, BackendLoopMode::Recording, None)
        .unwrap();
    backend.wait_idle();
    backend.advance(Duration::from_millis(20));
    backend
        .transition_loop(loop_id, BackendLoopMode::Playing, None)
        .unwrap();
    backend.clear_loop(loop_id).unwrap();
    backend.wait_idle();
    let state = backend.poll().unwrap().loops[&loop_id].clone();
    assert_eq!((state.mode, state.length), (BackendLoopMode::Stopped, 0));
    let audio = backend
        .loop_audio_data(loop_id)
        .unwrap()
        .unwrap_or_default();
    assert!(audio.iter().all(|channel| channel.is_empty()));
    backend
        .transition_loop(loop_id, BackendLoopMode::Recording, None)
        .unwrap();
    backend.wait_idle();
    assert_eq!(
        backend.poll().unwrap().loops[&loop_id].mode,
        BackendLoopMode::Recording
    );
    backend
        .transition_loop(loop_id, BackendLoopMode::Stopped, None)
        .unwrap();
    backend.wait_idle();
}

fn session_port(
    source_id: u64,
    owner: BackendPortOwner,
    name: String,
    data_type: BackendPortDataType,
    direction: BackendPortDirection,
    role: BackendPortRole,
) -> BackendSessionPort {
    BackendSessionPort {
        source_id,
        descriptor: BackendPortDescriptor {
            id: BackendPortId::from_raw(source_id),
            owner,
            name,
            data_type,
            direction,
            role,
        },
        external_connections: Vec::new(),
    }
}

/// A structure-only direct track. Ports follow the creation order of direct tracks: input
/// and output per audio channel, then MIDI input and output; their IDs start after
/// `100 * source_id`.
fn session_track(
    source_id: u64,
    port_name_base: &str,
    audio_channels: u32,
    midi: bool,
    loops: &[(u64, f32)],
) -> BackendSessionTrack {
    let topology = BackendTrackTopology::Direct {
        audio_channels,
        midi,
    };
    let base = port_name_base;
    let mut shapes = Vec::new();
    for index in 0..audio_channels {
        let suffix = if audio_channels == 1 {
            String::new()
        } else {
            format!("_{}", index + 1)
        };
        let input = format!("{base}_direct_in{suffix}");
        shapes.push((input, Audio, Input, BackendPortRole::AudioInput));
        let output = format!("{base}_direct_out{suffix}");
        shapes.push((output, Audio, Output, BackendPortRole::AudioOutput));
    }
    if midi {
        let input = format!("{base}_direct_midi_in");
        shapes.push((input, Midi, Input, BackendPortRole::MidiInput));
        let output = format!("{base}_direct_midi_out");
        shapes.push((output, Midi, Output, BackendPortRole::MidiOutput));
    }
    let ports = shapes
        .into_iter()
        .zip(1..)
        .map(|((name, data_type, direction, role), index)| {
            let owner = BackendPortOwner::Track;
            let id = source_id * 100 + index;
            session_port(id, owner, name, data_type, direction, role)
        })
        .collect();
    let loops = loops
        .iter()
        .map(|&(source_id, gain)| BackendLoopContent {
            source_id,
            length: 0,
            gain,
            balance: 0.0,
            audio: (0..audio_channels)
                .map(|_| BackendAudioContent {
                    mode: BackendChannelMode::Direct,
                    samples: Vec::new(),
                    gain: 1.0,
                    start_offset: 0,
                    capture_alignment_frames: 0,
                    preplay: 0,
                })
                .collect(),
            midi: (0..u32::from(midi))
                .map(|_| BackendMidiContent {
                    mode: BackendChannelMode::Direct,
                    length: 0,
                    start_state: Vec::new(),
                    events: Vec::new(),
                    start_offset: 0,
                    capture_alignment_frames: 0,
                    preplay: 0,
                })
                .collect(),
        })
        .collect();
    BackendSessionTrack {
        source_id,
        port_name_base: port_name_base.to_owned(),
        topology: topology.clone(),
        state: BackendTrackState {
            topology,
            audio_channels,
            midi,
            ..Default::default()
        },
        loops,
        ports,
        processor_state: None,
        builtin_fx_midi_cc_assignments: Vec::new(),
        oxisynth_midi_cc_assignments: Vec::new(),
    }
}

/// A new session of the core set: a sync track, a stereo MIDI track and the Master bus,
/// connected to the profile's host ports and routed to Master.
fn new_session_document(sample_rate: u32, profile: &ContractProfile) -> BackendSessionData {
    let mut sync = session_track(1, "contract_session_sync", 1, false, &[(11, 1.0)]);
    sync.ports[0]
        .external_connections
        .push(profile.audio_capture_port.clone());
    let mut track = session_track(2, "contract_session", 2, true, &[(21, 0.75), (22, 1.0)]);
    track.state.output_gain_db = -2.0;
    track.state.input_monitoring = true;
    if let Some(midi_capture) = &profile.midi_capture_port {
        track.ports[4]
            .external_connections
            .push(midi_capture.clone());
    }
    let master_owner = BackendPortOwner::Bus(MASTER_BUS_ID);
    let channels = MASTER_BUS_CHANNEL_LABELS
        .iter()
        .zip(1_u64..)
        .map(|(label, index)| {
            let mut output_port = session_port(
                910 + index,
                master_owner,
                format!("master_out_{index}"),
                Audio,
                Output,
                BackendPortRole::AudioOutput,
            );
            if index == 1 {
                output_port
                    .external_connections
                    .push(profile.audio_playback_port.clone());
            }
            BackendSessionBusChannel {
                source_id: 900 + index,
                label: (*label).to_owned(),
                output_port,
            }
        })
        .collect();
    BackendSessionData {
        sample_rate,
        tracks: vec![sync, track],
        buses: vec![BackendSessionBus {
            source_id: MASTER_BUS_ID.raw(),
            name: MASTER_BUS_NAME.to_owned(),
            channels,
            gain_db: -3.0,
            balance: 0.25,
            muted: false,
            processor_type: None,
            processor_state: None,
            builtin_fx_midi_cc_assignments: Vec::new(),
        }],
        mixer_routes: [(102, 901), (202, 901), (204, 902)]
            .into_iter()
            .map(
                |(source_port_id, destination_channel_id)| BackendSessionMixerRoute {
                    source_port_id,
                    destination_channel_id,
                },
            )
            .collect(),
        global_ports: vec![session_port(
            990,
            BackendPortOwner::GlobalFxControl,
            "Global FX Control MIDI In".to_owned(),
            Midi,
            Input,
            BackendPortRole::MidiInput,
        )],
        use_legacy_browser_default_routes: false,
    }
}

type SessionStructure = (
    Vec<BackendTrackId>,
    Vec<BackendLoopId>,
    Vec<BackendPortId>,
    BTreeSet<BackendConfirmedLink>,
    Vec<BackendBusId>,
    BTreeSet<BackendMixerLink>,
);

fn session_structure(backend: &mut dyn Backend) -> SessionStructure {
    backend.wait_idle();
    let snapshot = backend.poll().unwrap();
    (
        snapshot.tracks.keys().copied().collect(),
        snapshot.loops.keys().copied().collect(),
        snapshot
            .connections
            .application_ports
            .keys()
            .copied()
            .collect(),
        snapshot.connections.confirmed_links,
        snapshot.mixer.buses.keys().copied().collect(),
        snapshot.mixer.confirmed_links,
    )
}

/// Replacing the session with a structure-only document of the core set yields exactly that
/// session. Documents the backend cannot apply are rejected whole.
pub fn new_session_contract(backend: &mut dyn Backend, profile: &ContractProfile) {
    let previous = create_track(backend, "contract_session_previous", 1, false, 1);
    let input = port_with_role(&previous, BackendPortRole::AudioInput).id;
    backend
        .set_port_connected(input, &profile.audio_capture_port, true)
        .unwrap();
    let sample_rate = backend.poll().unwrap().status.sample_rate;
    let document = new_session_document(sample_rate, profile);
    let before = session_structure(backend);

    if !backend
        .capabilities()
        .supports(BackendCapability::SessionContentLoad)
    {
        let mut with_content = document.clone();
        let loop_ = &mut with_content.tracks[0].loops[0];
        loop_.length = 4;
        loop_.audio[0].samples = vec![0.5; 4];
        assert!(backend.replace_session(&with_content).is_err());
        assert_eq!(session_structure(backend), before);
    }
    let mut malformed = document.clone();
    malformed.tracks[1].ports.pop();
    assert!(backend.replace_session(&malformed).is_err());
    assert_eq!(session_structure(backend), before);

    let mapping = backend.replace_session(&document).unwrap();
    backend.wait_idle();
    let snapshot = backend.poll().unwrap();
    assert_eq!(
        snapshot.tracks.keys().copied().collect::<BTreeSet<_>>(),
        mapping
            .tracks
            .values()
            .map(|created| created.track_id)
            .collect()
    );
    assert_eq!(
        snapshot.loops.keys().copied().collect::<BTreeSet<_>>(),
        mapping.loops.values().copied().collect()
    );
    let mut track_ports = BTreeSet::new();
    for source in &document.tracks {
        let created = &mapping.tracks[&source.source_id];
        let state = &snapshot.tracks[&created.track_id];
        assert_eq!(
            (state.audio_channels, state.midi),
            (source.state.audio_channels, source.state.midi)
        );
        assert_eq!(
            (state.output_gain_db, state.input_monitoring),
            (source.state.output_gain_db, source.state.input_monitoring)
        );
        assert_eq!(created.ports.len(), source.ports.len());
        for source_loop in &source.loops {
            let loop_id = mapping.loops[&source_loop.source_id];
            assert!(created.loops.contains(&loop_id));
            let state = &snapshot.loops[&loop_id];
            assert_eq!(
                (state.mode, state.length, state.gain),
                (BackendLoopMode::Stopped, 0, source_loop.gain)
            );
        }
        track_ports.extend(created.ports.iter().map(|port| port.id));
    }
    assert_eq!(
        snapshot
            .connections
            .application_ports
            .values()
            .filter(|port| port.owner == BackendPortOwner::Track)
            .map(|port| port.id)
            .collect::<BTreeSet<_>>(),
        track_ports
    );
    let link = |source: u64, host_port: &str| BackendConfirmedLink {
        application_port_id: mapping.ports[&source],
        host_port_id: host_port.to_owned(),
    };
    let links = &snapshot.connections.confirmed_links;
    assert!(links.contains(&link(101, &profile.audio_capture_port)));
    assert!(links.contains(&link(911, &profile.audio_playback_port)));
    if let Some(midi_capture) = &profile.midi_capture_port {
        assert!(links.contains(&link(205, midi_capture)));
    }
    let master_id = mapping.buses[&MASTER_BUS_ID.raw()];
    assert_eq!(
        snapshot.mixer.buses.keys().copied().collect::<Vec<_>>(),
        [master_id]
    );
    let master = &snapshot.mixer.buses[&master_id];
    assert_eq!(
        (master.gain_db, master.balance, master.muted),
        (-3.0, 0.25, false)
    );
    assert_eq!(
        snapshot.mixer.confirmed_links,
        document
            .mixer_routes
            .iter()
            .map(|route| BackendMixerLink {
                source_port_id: mapping.ports[&route.source_port_id],
                destination_channel_id: mapping.bus_channels[&route.destination_channel_id],
            })
            .collect()
    );
    backend
        .set_loop_sync_source(mapping.loops[&21], Some(mapping.loops[&11]))
        .unwrap();
    backend.wait_idle();
}

/// The capture signal of the timing contracts at an absolute frame.
pub fn timing_ramp(frame: u64) -> f32 {
    (frame % 1000) as f32 / 1000.0
}

/// Drives a backend one frame at a time for the contracts run by [`harness_contracts`], each
/// of which starts on a fresh harness.
///
/// Frame conventions:
/// - `frame()` is the number of frames rendered so far. `step()` renders exactly frame
///   `frame()`, with [`timing_ramp`]`(frame())` on the profile's audio capture port, and
///   discards the output.
/// - Requests made between steps reach the backend before the next rendered frame. An
///   immediate request made while `frame() == f` takes effect at frame `f`, which is the
///   first frame rendered in the new mode. The contract checks it after the next step.
/// - After a step, `poll()` reflects every rendered frame and every boundary up to
///   `frame()`: it reports the mode, `position` and planned transition of the next frame
///   to render. A sync-source wrap between the last rendered frame and the next one is
///   already counted, and a transition due there is already visible.
/// - A `cycles_delay` of `n` takes effect at the `(n + 1)`-th wrap of the sync source after
///   the request; a wrap already reported at the time of the request does not count. Until
///   then `poll()` reports it as `next_mode`, with a `next_transition_delay` that starts at
///   `n` and drops by one at every wrap. A request nearer than the planned one replaces it.
/// - A loop without a sync source that is not playing applies every request immediately,
///   whatever its delay.
/// - Recorded content is read with `Backend::loop_audio_data_with_metadata`. The logical
///   content of a loop of length `L` is `samples[start_offset..start_offset + L]`; a backend
///   may keep material before the logical start.
/// - A loop playing from position `p`, as reported by `poll()`, renders sample `p` of its
///   logical content in the next frame.
/// - The track `output_peaks` and the bus `output_peaks_db` meters report, in dBFS, the peak
///   of the frames rendered since the previous `poll()`. A track input with monitoring
///   reaches the track output within the same frame. Control changes take effect within
///   16 frames.
/// - The contracts count frames from `frame()` after their setup, which may call
///   `Backend::wait_idle`. They do not call `wait_idle` while counting.
pub trait TimingHarness {
    fn backend(&mut self) -> &mut dyn Backend;
    fn profile(&self) -> &ContractProfile;
    fn frame(&self) -> u64;
    fn step(&mut self) -> Result<()>;
}

/// The legacy Web Audio backend at 1 kHz with one-frame quanta.
pub struct WebAudioTimingHarness {
    backend: EngineBackend,
    profile: ContractProfile,
    frame: u64,
}

impl WebAudioTimingHarness {
    pub fn new() -> Result<Self> {
        let mut backend = EngineBackend::new_web_audio(1_000, 1)?;
        backend.configure_web_audio_channels(1, 1)?;
        Ok(Self {
            backend,
            profile: ContractProfile::web_audio(),
            frame: 0,
        })
    }
}

impl TimingHarness for WebAudioTimingHarness {
    fn backend(&mut self) -> &mut dyn Backend {
        &mut self.backend
    }

    fn profile(&self) -> &ContractProfile {
        &self.profile
    }

    fn frame(&self) -> u64 {
        self.frame
    }

    fn step(&mut self) -> Result<()> {
        let input = [timing_ramp(self.frame)];
        let mut output = [0.0];
        self.backend
            .process_audio_quantum(&input, 1, &mut output, 1, 1)?;
        self.frame += 1;
        Ok(())
    }
}

const SYNC_CYCLE: u64 = 16;
const STEP_LIMIT: u64 = 256;

/// Frames in this scene are relative to the frame at which counting starts.
struct TimingScene {
    origin: u64,
    sync: BackendLoopId,
    follower: BackendLoopId,
    unsynced: BackendLoopId,
    sync_playing_from: Option<u64>,
}

impl TimingScene {
    fn set_up(harness: &mut dyn TimingHarness) -> Self {
        let capture = harness.profile().audio_capture_port.clone();
        let backend = harness.backend();
        let mut track = |name: &str, initial_loops| {
            let created = backend
                .create_direct_track(DirectTrackRequest {
                    port_name_base: name.to_owned(),
                    audio_channels: 1,
                    midi: false,
                    initial_loops,
                })
                .unwrap();
            let input = port_with_role(&created, BackendPortRole::AudioInput);
            backend
                .set_port_connected(input.id, &capture, true)
                .unwrap();
            created
        };
        let sync_track = track("timing_sync", 1);
        let other = track("timing_other", 2);
        backend
            .set_loop_sync_source(other.loops[0], Some(sync_track.loops[0]))
            .unwrap();
        backend.wait_idle();
        Self {
            origin: harness.frame(),
            sync: sync_track.loops[0],
            follower: other.loops[0],
            unsynced: other.loops[1],
            sync_playing_from: None,
        }
    }

    fn now(&self, harness: &dyn TimingHarness) -> u64 {
        harness.frame() - self.origin
    }

    fn state(&self, harness: &mut dyn TimingHarness, loop_id: BackendLoopId) -> BackendLoopState {
        harness.backend().poll().unwrap().loops[&loop_id].clone()
    }

    fn request(
        &self,
        harness: &mut dyn TimingHarness,
        loop_id: BackendLoopId,
        mode: BackendLoopMode,
        cycles_delay: Option<u32>,
    ) {
        harness
            .backend()
            .transition_loop(loop_id, mode, cycles_delay)
            .unwrap();
    }

    /// Steps one frame and checks that the playing sync loop keeps wrapping on its grid.
    fn step(&self, harness: &mut dyn TimingHarness) {
        harness.step().expect("timing harness step failed");
        if let Some(start) = self.sync_playing_from {
            let now = self.now(harness);
            let sync = self.state(harness, self.sync);
            assert_eq!(sync.mode, BackendLoopMode::Playing, "sync loop at {now}");
            assert_eq!(
                u64::from(sync.position),
                (now - start) % SYNC_CYCLE,
                "sync loop position at {now}"
            );
        }
    }

    fn step_to(&self, harness: &mut dyn TimingHarness, frame: u64) {
        assert!(self.now(harness) <= frame);
        while self.now(harness) < frame {
            self.step(harness);
        }
    }

    /// Steps until `poll()` reports the mode `to`, checking at every frame before that the
    /// countdown of a transition requested at the current frame with `cycles_delay` of
    /// `delay`, and returns the first frame reporting `to`.
    fn step_through_plan(
        &self,
        harness: &mut dyn TimingHarness,
        loop_id: BackendLoopId,
        from: BackendLoopMode,
        to: BackendLoopMode,
        delay: u64,
    ) -> u64 {
        let start = self
            .sync_playing_from
            .expect("the sync loop is not playing");
        let wraps = |frame: u64| (frame - start) / SYNC_CYCLE;
        let requested = self.now(harness);
        for _ in 0..STEP_LIMIT {
            self.step(harness);
            let now = self.now(harness);
            let state = self.state(harness, loop_id);
            if state.mode == to {
                return now;
            }
            let remaining = delay
                .checked_sub(wraps(now) - wraps(requested))
                .unwrap_or_else(|| panic!("{to:?} was due before {now}"));
            assert_eq!(
                (
                    state.mode,
                    state.next_mode,
                    state.next_transition_delay.map(u64::from)
                ),
                (from, Some(to), Some(remaining)),
                "planned transition at {now}"
            );
        }
        panic!("{loop_id:?} never reached {to:?}");
    }

    fn logical_content(&self, harness: &mut dyn TimingHarness, loop_id: BackendLoopId) -> Vec<f32> {
        let length = self.state(harness, loop_id).length as usize;
        let data = harness
            .backend()
            .loop_audio_data_with_metadata(loop_id)
            .unwrap()
            .expect("loop content is unavailable");
        let channel = &data.channels[0];
        let start = usize::try_from(channel.start_offset).expect("negative start offset");
        channel.samples[start..start + length].to_vec()
    }

    fn ramp(&self, from: u64, length: u64) -> Vec<f32> {
        (from..from + length)
            .map(|frame| timing_ramp(self.origin + frame))
            .collect()
    }

    fn immediate_sync_loop(&mut self, harness: &mut dyn TimingHarness) {
        self.step_to(harness, 3);
        self.request(harness, self.sync, BackendLoopMode::Recording, None);
        self.step(harness);
        assert_eq!(
            self.state(harness, self.sync).mode,
            BackendLoopMode::Recording
        );
        self.step_to(harness, 19);
        self.request(harness, self.sync, BackendLoopMode::Playing, None);
        self.sync_playing_from = Some(19);
        self.step(harness);
        assert_eq!(self.state(harness, self.sync).length, SYNC_CYCLE as u32);
        assert_eq!(
            self.logical_content(harness, self.sync),
            self.ramp(3, SYNC_CYCLE)
        );
    }

    fn synced_record_after_one_cycle(&self, harness: &mut dyn TimingHarness) {
        use BackendLoopMode::{Recording, Stopped};
        self.step_to(harness, 20);
        self.request(harness, self.follower, Recording, Some(1));
        let recording = self.step_through_plan(harness, self.follower, Stopped, Recording, 1);
        assert_eq!(recording, 51, "synced recording start");
    }

    fn synced_play_after_two_cycles(&self, harness: &mut dyn TimingHarness) {
        use BackendLoopMode::{Playing, Recording};
        self.request(harness, self.follower, Playing, Some(2));
        let playing = self.step_through_plan(harness, self.follower, Recording, Playing, 2);
        assert_eq!(playing, 99, "synced playback start");
        assert_eq!(self.state(harness, self.follower).length, 48);
        assert_eq!(
            self.logical_content(harness, self.follower),
            self.ramp(51, 48)
        );
    }

    fn nearer_stop_supersedes(&self, harness: &mut dyn TimingHarness) {
        use BackendLoopMode::{Playing, Stopped};
        self.request(harness, self.follower, Stopped, Some(3));
        self.request(harness, self.follower, Stopped, Some(1));
        let stopped = self.step_through_plan(harness, self.follower, Playing, Stopped, 1);
        assert_eq!(stopped, 131, "superseding stop");
        for _ in 0..3 * SYNC_CYCLE {
            self.step(harness);
            let state = self.state(harness, self.follower);
            assert_eq!((state.mode, state.next_mode), (Stopped, None));
        }
    }

    fn unsynced_delay_is_immediate(&self, harness: &mut dyn TimingHarness) {
        let requested = self.now(harness);
        self.request(harness, self.unsynced, BackendLoopMode::Recording, Some(2));
        self.step(harness);
        assert_eq!(
            self.state(harness, self.unsynced).mode,
            BackendLoopMode::Recording
        );
        self.step_to(harness, requested + 8);
        self.request(harness, self.unsynced, BackendLoopMode::Playing, None);
        self.step(harness);
        assert_eq!(self.state(harness, self.unsynced).length, 8);
        assert_eq!(
            self.logical_content(harness, self.unsynced),
            self.ramp(requested, 8)
        );
    }
}

/// Transition frames, loop lengths and recorded content of the core workflow.
///
/// Two one-channel tracks take their input from the profile's audio capture port: one holds
/// the sync loop, the other a loop that follows it and a loop without a sync source. With
/// frames counted from the start:
/// - the sync loop records immediately from 3 and plays immediately from 19: length 16,
///   content of frames 3–18, wrapping at 35, 51, 67, …;
/// - the follower, asked at 20 to record after 1 cycle, plans it and records from 51;
/// - asked at 51 to play after 2 cycles, it plays from 99 with length 48 and the content of
///   frames 51–98;
/// - asked at 99 to stop after 3 cycles and then after 1, the nearer request replaces the
///   other: it stops at 131 and stays stopped;
/// - the loop without a sync source, asked to record after 2 cycles while stopped, records
///   immediately.
pub fn transition_timing_contract(harness: &mut dyn TimingHarness) {
    let mut scene = TimingScene::set_up(harness);
    scene.immediate_sync_loop(harness);
    scene.synced_record_after_one_cycle(harness);
    scene.synced_play_after_two_cycles(harness);
    scene.nearer_stop_supersedes(harness);
    scene.unsynced_delay_is_immediate(harness);
}

/// Settling frames after a control change before levels are compared.
const SETTLE_FRAMES: usize = 16;
const SILENCE_DB: f32 = -100.0;
const LEVEL_TOLERANCE_DB: f32 = 0.05;

fn amplitude_db(amplitude: f32) -> f32 {
    20.0 * amplitude.log10()
}

/// An expected meter reading relative to a reference amplitude.
#[derive(Clone, Copy, Debug)]
enum Level {
    Silent,
    Db(f32),
}

fn assert_levels(measured: &[f32], reference: f32, expected: [Level; 2], what: &str) {
    assert_eq!(measured.len(), 2, "{what} meters");
    for (channel, (&measured, expected)) in measured.iter().zip(expected).enumerate() {
        match expected {
            Level::Silent => assert!(
                measured <= SILENCE_DB,
                "{what} channel {channel}: {measured} dB is not silent"
            ),
            Level::Db(relative) => {
                let expected = amplitude_db(reference) + relative;
                assert!(
                    (measured - expected).abs() <= LEVEL_TOLERANCE_DB,
                    "{what} channel {channel}: {measured} dB instead of {expected} dB"
                );
            }
        }
    }
}

fn step_and_poll(harness: &mut dyn TimingHarness) -> BackendSnapshot {
    harness.step().expect("timing harness step failed");
    harness.backend().poll().unwrap()
}

/// Lets changes settle, then renders one frame and returns the snapshot with that frame's
/// capture value.
fn settle_and_measure(harness: &mut dyn TimingHarness) -> (BackendSnapshot, f32) {
    for _ in 0..SETTLE_FRAMES {
        step_and_poll(harness);
    }
    let input = timing_ramp(harness.frame());
    (step_and_poll(harness), input)
}

enum Change {
    Track(BackendTrackControl),
    Master(BackendBusControl),
}

fn check_signal_levels(
    harness: &mut dyn TimingHarness,
    track: BackendTrackId,
    output: [Level; 2],
    master: [Level; 2],
    what: &str,
) {
    let (snapshot, input) = settle_and_measure(harness);
    assert_levels(&snapshot.tracks[&track].output_peaks, input, output, what);
    let master_peaks = &snapshot.mixer.buses[&MASTER_BUS_ID].output_peaks_db;
    assert_levels(master_peaks, input, master, &format!("Master with {what}"));
}

fn apply_change(harness: &mut dyn TimingHarness, track: BackendTrackId, change: Change) {
    match change {
        Change::Track(control) => harness.backend().set_track_control(track, control),
        Change::Master(control) => harness.backend().set_bus_control(MASTER_BUS_ID, control),
    }
    .unwrap();
}

/// The audible effect of track controls, Master controls and routes to Master.
///
/// A stereo track takes both inputs from the profile's audio capture port. Its output
/// meters and the Master meters are compared with the capture signal of the measured frame.
pub fn signal_chain_contract(harness: &mut dyn TimingHarness) {
    use BackendBusControl as Bus;
    use BackendTrackControl as Track;
    use Level::{Db, Silent};
    let capture = harness.profile().audio_capture_port.clone();
    let backend = harness.backend();
    let created = create_track(backend, "contract_signal", 2, false, 1);
    let track = created.track_id;
    for input in ports_with_role(&created, BackendPortRole::AudioInput) {
        backend.set_port_connected(input, &capture, true).unwrap();
    }
    backend
        .set_track_control(track, Track::InputMonitoring(false))
        .unwrap();
    let channels = master_channels(backend);
    let outputs = ports_with_role(&created, BackendPortRole::AudioOutput);
    backend.wait_idle();

    check_signal_levels(harness, track, [Silent; 2], [Silent; 2], "no monitoring");
    harness
        .backend()
        .set_track_control(track, Track::InputMonitoring(true))
        .unwrap();
    check_signal_levels(harness, track, [Db(0.0); 2], [Silent; 2], "monitoring");
    for (output, channel) in outputs.iter().zip(&channels) {
        harness
            .backend()
            .set_mixer_route(*output, *channel, true)
            .unwrap();
    }
    check_signal_levels(
        harness,
        track,
        [Db(0.0); 2],
        [Db(0.0); 2],
        "routes to Master",
    );

    for (change, revert, output, master) in [
        (
            Change::Track(Track::InputGainDb(-6.0)),
            Change::Track(Track::InputGainDb(0.0)),
            [Db(-6.0); 2],
            [Db(-6.0); 2],
        ),
        (
            Change::Track(Track::InputBalance(-1.0)),
            Change::Track(Track::InputBalance(0.0)),
            [Db(0.0), Silent],
            [Db(0.0), Silent],
        ),
        (
            Change::Track(Track::InputMonitoring(false)),
            Change::Track(Track::InputMonitoring(true)),
            [Silent; 2],
            [Silent; 2],
        ),
        (
            Change::Track(Track::OutputGainDb(-6.0)),
            Change::Track(Track::OutputGainDb(0.0)),
            [Db(-6.0); 2],
            [Db(-6.0); 2],
        ),
        (
            Change::Track(Track::OutputBalance(1.0)),
            Change::Track(Track::OutputBalance(0.0)),
            [Silent, Db(0.0)],
            [Silent, Db(0.0)],
        ),
        (
            Change::Track(Track::OutputMute(true)),
            Change::Track(Track::OutputMute(false)),
            [Silent; 2],
            [Silent; 2],
        ),
        (
            Change::Master(Bus::GainDb(-6.0)),
            Change::Master(Bus::GainDb(0.0)),
            [Db(0.0); 2],
            [Db(-6.0); 2],
        ),
        (
            Change::Master(Bus::Balance(-1.0)),
            Change::Master(Bus::Balance(0.0)),
            [Db(0.0); 2],
            [Db(0.0), Silent],
        ),
        (
            Change::Master(Bus::Mute(true)),
            Change::Master(Bus::Mute(false)),
            [Db(0.0); 2],
            [Silent; 2],
        ),
    ] {
        let what = match &change {
            Change::Track(control) => format!("{control:?}"),
            Change::Master(control) => format!("Master {control:?}"),
        };
        apply_change(harness, track, change);
        check_signal_levels(harness, track, output, master, &what);
        apply_change(harness, track, revert);
    }
    harness
        .backend()
        .set_mixer_route(outputs[0], channels[0], false)
        .unwrap();
    check_signal_levels(
        harness,
        track,
        [Db(0.0); 2],
        [Silent, Db(0.0)],
        "one route to Master",
    );
}

/// Records `frames` frames immediately, plays the loop and returns the recorded content.
fn record_loop(harness: &mut dyn TimingHarness, loop_id: BackendLoopId, frames: u64) -> Vec<f32> {
    let start = harness.frame();
    harness
        .backend()
        .transition_loop(loop_id, BackendLoopMode::Recording, None)
        .unwrap();
    for _ in 0..frames {
        step_and_poll(harness);
    }
    harness
        .backend()
        .transition_loop(loop_id, BackendLoopMode::Playing, None)
        .unwrap();
    let snapshot = step_and_poll(harness);
    assert_eq!(snapshot.loops[&loop_id].length, frames as u32);
    let data = harness
        .backend()
        .loop_audio_data_with_metadata(loop_id)
        .unwrap()
        .expect("loop content is unavailable");
    let expected = (start..start + frames).map(timing_ramp).collect::<Vec<_>>();
    for channel in &data.channels {
        let offset = usize::try_from(channel.start_offset).expect("negative start offset");
        assert_eq!(channel.samples[offset..offset + frames as usize], expected);
    }
    expected
}

/// Recorded content plays with the loop's gain and balance, and a cleared loop is silent,
/// empty and records anew.
///
/// A stereo track records both channels from the profile's audio capture port without
/// monitoring; its output meters are compared with the recorded sample at the position
/// `poll()` reported before each step.
pub fn loop_playback_contract(harness: &mut dyn TimingHarness) {
    const LENGTH: u64 = 16;
    let capture = harness.profile().audio_capture_port.clone();
    let backend = harness.backend();
    let created = create_track(backend, "contract_playback", 2, false, 1);
    let (track, loop_id) = (created.track_id, created.loops[0]);
    for input in ports_with_role(&created, BackendPortRole::AudioInput) {
        backend.set_port_connected(input, &capture, true).unwrap();
    }
    backend
        .set_track_control(track, BackendTrackControl::InputMonitoring(false))
        .unwrap();
    backend.wait_idle();

    for _ in 0..SETTLE_FRAMES {
        step_and_poll(harness);
    }
    let content = record_loop(harness, loop_id, LENGTH);

    for (gain, balance, factors) in [
        (1.0, 0.0, [1.0, 1.0]),
        (0.5, 0.0, [0.5, 0.5]),
        (0.5, -1.0, [0.5, 0.0]),
    ] {
        let backend = harness.backend();
        backend.set_loop_gain(loop_id, gain).unwrap();
        backend.set_loop_balance(loop_id, balance).unwrap();
        for _ in 0..LENGTH {
            step_and_poll(harness);
        }
        for _ in 0..LENGTH {
            let position = harness.backend().poll().unwrap().loops[&loop_id].position as usize;
            let snapshot = step_and_poll(harness);
            let state = &snapshot.loops[&loop_id];
            assert_eq!((state.gain, state.balance), (gain, balance));
            let expected = factors.map(|factor| {
                if factor == 0.0 {
                    Level::Silent
                } else {
                    Level::Db(amplitude_db(factor))
                }
            });
            let what = format!("loop gain {gain} balance {balance} at {position}");
            assert_levels(
                &snapshot.tracks[&track].output_peaks,
                content[position],
                expected,
                &what,
            );
        }
    }

    harness.backend().clear_loop(loop_id).unwrap();
    let snapshot = step_and_poll(harness);
    let state = &snapshot.loops[&loop_id];
    assert_eq!((state.mode, state.length), (BackendLoopMode::Stopped, 0));
    let audio = harness
        .backend()
        .loop_audio_data(loop_id)
        .unwrap()
        .unwrap_or_default();
    assert!(audio.iter().all(|channel| channel.is_empty()));
    let snapshot = step_and_poll(harness);
    assert_levels(
        &snapshot.tracks[&track].output_peaks,
        1.0,
        [Level::Silent; 2],
        "cleared loop",
    );
    record_loop(harness, loop_id, 8);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FakeBackend;

    #[shoop_wasm_test_support::shoop_test]
    fn fake_backend_satisfies_contracts() {
        backend_contracts(&mut FakeBackend::default(), &ContractProfile::dummy_host());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn fake_backend_without_capabilities_satisfies_contracts() {
        let mut backend = FakeBackend::default();
        for capability in BackendCapability::ALL {
            backend.set_capability(capability, false);
        }
        backend_contracts(&mut backend, &ContractProfile::dummy_host());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn engine_dummy_backend_satisfies_contracts() {
        let mut backend = EngineBackend::new_dummy(48_000, 256).unwrap();
        backend_contracts(&mut backend, &ContractProfile::dummy_host());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn engine_web_audio_backend_satisfies_contracts() {
        let mut backend = EngineBackend::new_web_audio(48_000, 128).unwrap();
        backend.configure_web_audio_channels(1, 1).unwrap();
        backend_contracts(&mut backend, &ContractProfile::web_audio());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn legacy_web_audio_backend_satisfies_harness_contracts() {
        harness_contracts(|| WebAudioTimingHarness::new().unwrap());
    }
}
