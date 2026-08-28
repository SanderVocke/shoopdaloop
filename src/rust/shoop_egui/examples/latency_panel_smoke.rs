use std::sync::Arc;

use shoop_egui::*;

struct Preview {
    panel: LatencyPanel,
    started: std::time::Instant,
}

impl Preview {
    fn new(context: &egui::Context) -> Self {
        shoop_egui::initialize(context);
        let mut panel = LatencyPanel::default();
        panel.open();
        Self {
            panel,
            started: std::time::Instant::now(),
        }
    }
}

impl eframe::App for Preview {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let context = ui.ctx().clone();
        let index = ((self.started.elapsed().as_secs() / 3) % 4) as usize;
        let track_id = TrackId::from_raw(index as u64 + 1);
        let processor = match index {
            1 => Some(TrackProcessorTypeId::new(TrackProcessorTypeId::EXTERNAL)),
            2 => Some(TrackProcessorTypeId::new(
                TrackProcessorTypeId::CARLA_PATCHBAY,
            )),
            3 => Some(TrackProcessorTypeId::new(TrackProcessorTypeId::OXISYNTH)),
            _ => None,
        };
        let topology = processor
            .clone()
            .map(|processor_type| TrackTopology::DryWet {
                dry_audio_channels: 2,
                wet_audio_channels: 2,
                dry_midi: true,
                processor_type,
            })
            .unwrap_or(TrackTopology::Direct);
        let fx = processor.map(|processor_type| TrackFxState {
            processor_type,
            active: true,
            visible: false,
            lifecycle: FxLifecycle::Running,
            generation: 1,
            crash_summary: None,
            logs: Arc::from([]),
            editor: None,
            latency: LatencyObservationState {
                minimum_frames: Some(3),
                maximum_frames: Some(if index == 3 { 63 } else { 7 }),
                certainty: LatencyCertaintyState::Range,
                sample_rate: 48_000,
                revision: 4,
            },
            latency_provider: if index == 3 {
                LatencyProviderState::BuiltInSynthPhaseRange
            } else {
                LatencyProviderState::CarlaPatchbayGraphRange
            },
        });
        let input = PortId::from_raw(10);
        let output = PortId::from_raw(11);
        let host = HostPortId::new("system:playback_1");
        let observation = LatencyObservationState {
            minimum_frames: Some(4),
            maximum_frames: Some(6),
            certainty: LatencyCertaintyState::Range,
            sample_rate: 48_000,
            revision: 2,
        };
        let connections = ConnectionViewState {
            backend_available: true,
            application_ports: Arc::from([
                ApplicationPortState {
                    id: input,
                    owner: ApplicationPortOwner::Track {
                        track_id,
                        kind: TrackPortOwnerKind::Main,
                    },
                    name: "Capture input".to_owned(),
                    data_type: PortDataType::Audio,
                    direction: PortDirection::Input,
                    role: PortRole::AudioInput,
                    connection_policy: ConnectionPolicy::UserManaged,
                    capture_latency: observation,
                    playback_latency: Default::default(),
                },
                ApplicationPortState {
                    id: output,
                    owner: ApplicationPortOwner::Track {
                        track_id,
                        kind: TrackPortOwnerKind::Main,
                    },
                    name: "Cue output".to_owned(),
                    data_type: PortDataType::Audio,
                    direction: PortDirection::Output,
                    role: PortRole::AudioOutput,
                    connection_policy: ConnectionPolicy::UserManaged,
                    capture_latency: Default::default(),
                    playback_latency: LatencyObservationState {
                        minimum_frames: Some(8),
                        maximum_frames: Some(8),
                        certainty: LatencyCertaintyState::Exact,
                        sample_rate: 48_000,
                        revision: 3,
                    },
                },
            ]),
            host_ports: Arc::from([HostPortState {
                id: host.clone(),
                name: "Speakers".to_owned(),
                data_type: PortDataType::Audio,
                direction: PortDirection::Input,
            }]),
            confirmed_links: Arc::from([ConfirmedConnectionState {
                application_port_id: output,
                host_port_id: host.clone(),
            }]),
            ..Default::default()
        };
        let mut diagnostics = LatencyDiagnosticsState::default();
        diagnostics.plot_len = 4;
        diagnostics.plot_cursor = 4;
        diagnostics.applied_capture_plot[..4].copy_from_slice(&[3, 5, 5, 7]);
        diagnostics.render_advance_plot[..4].copy_from_slice(&[0, 3, 7, 7]);
        diagnostics.active_postroll_plot[..4].copy_from_slice(&[0, 0, 4, 2]);
        let status = StatusState {
            sample_rate: 48_000,
            latency_diagnostics: diagnostics,
            backend_capture_latency: observation,
            ..Default::default()
        };
        let component = |kind, enabled, value_mode| LatencyComponentPolicyState {
            kind,
            enabled,
            value_mode,
            range_selection: LatencyRangeSelectionState::Maximum,
        };
        let track = TrackState {
            id: track_id,
            name: ["Direct", "External", "Carla Patchbay", "Built-in Synth"][index].to_owned(),
            topology,
            fx,
            port_ids: Arc::from([input, output]),
            latency_policy: TrackLatencyPolicyState {
                cue_followed: true,
                cue_output: Some(CueOutputSelection::HostPort(host)),
                components: Arc::from([
                    component(
                        LatencyComponentKind::ExternalCapture,
                        true,
                        LatencyValueMode::Automatic,
                    ),
                    component(
                        LatencyComponentKind::Processor,
                        index > 0,
                        LatencyValueMode::AutomaticPlusTrim(-1),
                    ),
                    component(
                        LatencyComponentKind::CuePlayback,
                        true,
                        LatencyValueMode::Automatic,
                    ),
                    component(
                        LatencyComponentKind::BackendBuffering,
                        false,
                        LatencyValueMode::Automatic,
                    ),
                    component(
                        LatencyComponentKind::Manual,
                        true,
                        LatencyValueMode::Manual(2),
                    ),
                ]),
                revision: 7,
                ..Default::default()
            },
            loops: vec![LoopState {
                id: LoopId::from_raw(20),
                name: "Frozen take".to_owned(),
                latency: TakeLatencyProvenanceState {
                    capture_alignment_frames: 13,
                    retained_before_frames: 2,
                    retained_after_frames: 13,
                    certainty: LatencyCertaintyState::Range,
                    observation_min_frames: Some(11),
                    observation_max_frames: Some(13),
                    observation_sample_rate: 48_000,
                    observation_revision: 6,
                    changed_during_operation: true,
                    incomplete: index == 1,
                    ..Default::default()
                },
                ..Default::default()
            }],
            ..Default::default()
        };
        let _ = self.panel.show(
            &context,
            &track,
            Some(LatencyPanelContext {
                status: &status,
                connections: &connections,
            }),
        );
        context.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

fn main() -> eframe::Result {
    eframe::run_native(
        "Latency panel usability preview",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([1000.0, 760.0]),
            ..Default::default()
        },
        Box::new(|creation| Ok(Box::new(Preview::new(&creation.egui_ctx)))),
    )
}
