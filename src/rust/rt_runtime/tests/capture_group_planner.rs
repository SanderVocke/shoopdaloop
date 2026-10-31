mod support;
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    executor::{Command, GraphRuntime, PreparedNode},
    graph::{compile_planned, Connection, Endpoint, GraphDescription, PortCounts, PortType},
    midi_state::MidiState,
    planner::*,
    processor::{ProcessBlock, Processor},
    resource::*,
    transport::{Lane, Outcome},
    Budgets, Config, Epoch, Error, Frame, NodeId, PlannerId,
};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;
#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
fn node() -> NodeId {
    NodeId::from_parts(0, 1).unwrap()
}
fn planner() -> PlannerId {
    PlannerId::from_parts(0, 1).unwrap()
}
#[derive(Clone, Copy, Default)]
struct Decision {
    interval: Option<CaptureIntervalId>,
    stopped: bool,
}
struct CaptureClock {
    group: CaptureGroupId,
    take: CaptureTakeId,
    decisions: [Decision; 1],
    snapshots: [Option<Snapshot>; 2],
}
impl Planner for CaptureClock {
    type Decision = Decision;
    type Observation = ();
    type Operation = ();
    type Result = ();
    fn spec(&self) -> Option<PlannerSpec> {
        Some(PlannerSpec {
            id: planner(),
            decision_slots: 1,
            observation_capacity: 0,
            operation_bytes: 0,
        })
    }
    fn decisions(&self) -> &[Decision] {
        &self.decisions
    }
    fn plan(&mut self, mut context: PlanContext<'_>) -> Result<Frame, Error> {
        if context.frame == Frame(0) {
            context
                .resources
                .capture()
                .activate(self.take, context.frame)?;
        }
        self.decisions[0].interval = None;
        if !self.decisions[0].stopped {
            match context
                .resources
                .capture()
                .reserve(self.take, context.frame, 1)
            {
                Ok(interval) => self.decisions[0].interval = Some(interval),
                Err(Error::Capacity) => {
                    assert_eq!(context.frame, Frame(24));
                    context.resources.capture().freeze(
                        self.take,
                        context.frame,
                        Some(Error::Capacity),
                        &mut self.snapshots,
                    )?;
                    self.decisions[0].stopped = true;
                }
                Err(error) => return Err(error),
            }
        }
        Ok(context.frame.checked_add(1)?.min(context.ceiling))
    }
    fn finish(&mut self, mut context: FinishContext<'_, ()>) -> Result<(), Error> {
        assert!(context.observations.is_empty());
        if let Some(interval) = self.decisions[0].interval {
            context.resources.capture().commit(interval)?;
        }
        if context.interval.end == Frame(32) {
            assert!(self.decisions[0].stopped);
            for snapshot in self.snapshots.iter().flatten() {
                assert_eq!(snapshot.description.frames, 24);
                assert_eq!(snapshot.cut_frame, Frame(24));
                assert_eq!(snapshot.capture.unwrap().incomplete, Some(Error::Capacity));
            }
            let audio = context
                .resources
                .read_index(self.snapshots[0].unwrap().id)?;
            let mut cursor = context.resources.render().cursor_at(audio, 23 * 8)?;
            let mut samples = [0; 8];
            assert_eq!(
                context.resources.render().read(&mut cursor, &mut samples)?,
                8
            );
            assert_eq!(&samples[..4], &1f32.to_le_bytes());
            assert_eq!(&samples[4..], &(-1f32).to_le_bytes());
            let midi = context
                .resources
                .read_index(self.snapshots[1].unwrap().id)?;
            let positioned = context.resources.render().midi_seek(midi, 24)?;
            assert_eq!(
                positioned
                    .state
                    .channel(0)
                    .unwrap()
                    .note(60)
                    .unwrap()
                    .velocity(),
                24
            );
            for snapshot in &mut self.snapshots {
                context
                    .resources
                    .release_snapshot(snapshot.take().unwrap().id)?;
            }
            context.resources.capture().release_group(self.group)?;
        }
        Ok(())
    }
}
struct CaptureNode {
    midi: MidiBuffer,
}
impl Processor<CaptureClock> for CaptureNode {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 1,
            audio_outputs: 2,
            ..PortCounts::default()
        }
    }
    fn planning(&self) -> ConsumerAccess {
        ConsumerAccess {
            decision_required: true,
            observations: 0,
            operation_bytes: 0,
        }
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, CaptureClock>) -> Result<(), Error> {
        assert_eq!(block.frames, 1);
        let decision = *block.decision.as_ref().unwrap().value;
        let sample = {
            let (input, output) = block.audio.pair(0, 0).unwrap();
            output[0] = input[0] * if decision.stopped { 2. } else { 1. };
            output[0]
        };
        block.audio.output(1).unwrap()[0] = 7.;
        if let Some(interval) = decision.interval {
            block
                .resources
                .render()
                .stage_audio(interval, 0, &[sample, -sample])?;
            self.midi.clear();
            self.midi.push(0, 1, &[0x90, 60, block.frame.0 as u8 + 1])?;
            block
                .resources
                .render()
                .stage_midi(interval, 1, &self.midi)?;
        }
        Ok(())
    }
}
#[shoop_test]
fn reservation_failure_changes_dependent_output_before_the_affected_sample() {
    for partition in [&[32][..], &[1; 32][..], &[7, 9, 5, 11][..]] {
        let config = Config {
            max_frames: 32,
            max_nodes: 1,
            max_ports: 8,
            max_edges: 8,
            audio_inputs: 1,
            audio_outputs: 2,
            midi_inputs: 0,
            midi_outputs: 0,
            midi_events_per_port: 8,
            midi_bytes_per_port: 128,
            resource_slots: 8,
            resource_pages: 64,
            page_bytes: 16,
            budgets: Budgets {
                transfer_bytes: 16,
                validation_steps: 512,
                subranges: 0,
                ..Budgets::default()
            },
            ..Config::default()
        }
        .prepare()
        .unwrap();
        let mut store = ResourceStore::with_capture_groups(
            config,
            Some(MidiIndexConfig {
                versions: 2,
                checkpoints_per_version: 4,
                replay_events: 8,
            }),
            CaptureGroupConfig {
                groups: 1,
                members_per_group: 2,
                staging_bytes_per_group: 480,
            },
        )
        .unwrap();
        let resources = [store.create().unwrap(), store.create().unwrap()];
        let group = store
            .bind_capture_group(&[
                CaptureMember {
                    resource: resources[0],
                    format: Format::Audio { channels: 2 },
                    midi_events: 0,
                    midi_bytes: 0,
                },
                CaptureMember {
                    resource: resources[1],
                    format: Format::Midi,
                    midi_events: 8,
                    midi_bytes: 128,
                },
            ])
            .unwrap();
        let take = store
            .prepare_capture_take(group, &[MidiState::default()])
            .unwrap();
        let clock = CaptureClock {
            group,
            take,
            decisions: [Decision::default()],
            snapshots: [None; 2],
        };
        let (mut control, mut runtime) = GraphRuntime::prepare_with_planner_and_resources(
            config,
            vec![PreparedNode::new(
                node(),
                CaptureNode {
                    midi: MidiBuffer::new(8, 128).unwrap(),
                },
            )],
            clock,
            store,
        )
        .unwrap();
        let plan = compile_planned(
            runtime.catalog(),
            &GraphDescription {
                nodes: vec![node()],
                connections: vec![
                    Connection {
                        source: Endpoint {
                            node: None,
                            kind: PortType::Audio,
                            port: 0,
                        },
                        destination: Endpoint {
                            node: Some(node()),
                            kind: PortType::Audio,
                            port: 0,
                        },
                    },
                    Connection {
                        source: Endpoint {
                            node: Some(node()),
                            kind: PortType::Audio,
                            port: 0,
                        },
                        destination: Endpoint {
                            node: None,
                            kind: PortType::Audio,
                            port: 0,
                        },
                    },
                    Connection {
                        source: Endpoint {
                            node: Some(node()),
                            kind: PortType::Audio,
                            port: 1,
                        },
                        destination: Endpoint {
                            node: None,
                            kind: PortType::Audio,
                            port: 1,
                        },
                    },
                ],
            },
            &[DecisionBinding {
                planner: planner(),
                consumer: node(),
                decision_slot: 0,
            }],
        )
        .unwrap();
        control
            .submit(Lane::Normal, Command::commit(Epoch(0), plan))
            .unwrap_or_else(|e| panic!("{:?}", e.error));
        let mut at = 0;
        for &frames in partition {
            let mut output = [0.; 32];
            let mut unrelated = [0.; 32];
            assert_no_heap(|| {
                runtime.process(
                    AudioBlock {
                        frame: Frame(at),
                        frames,
                        inputs: &[&[1.; 32]],
                        outputs: &mut [&mut output, &mut unrelated],
                    },
                    MidiBlock {
                        inputs: &[],
                        outputs: &mut [],
                    },
                )
            })
            .unwrap();
            for i in 0..frames {
                assert_eq!(output[i], if at + (i as u64) < 24 { 1. } else { 2. });
                assert_eq!(unrelated[i], 7.);
            }
            at += frames as u64;
        }
        assert_eq!(runtime.capture_group_usage().free_groups, 1);
        assert_eq!(control.try_completion().unwrap().outcome, Outcome::Applied);
        for resource in resources {
            control
                .submit(
                    Lane::Normal,
                    Command::resource(Request::Remove { resource }),
                )
                .unwrap_or_else(|e| panic!("{:?}", e.error));
        }
        let mut out = [0.; 1];
        let mut other = [0.; 1];
        assert_no_heap(|| {
            runtime.process(
                AudioBlock {
                    frame: Frame(32),
                    frames: 1,
                    inputs: &[&[1.]],
                    outputs: &mut [&mut out, &mut other],
                },
                MidiBlock {
                    inputs: &[],
                    outputs: &mut [],
                },
            )
        })
        .unwrap();
        for _ in 0..2 {
            assert_eq!(control.try_completion().unwrap().outcome, Outcome::Applied);
        }
        assert_eq!(runtime.resource_usage().free_pages, 64);
        assert_eq!(runtime.resource_usage().free_versions, 8);
        assert_eq!(runtime.resource_usage().free_snapshots, 8);
        assert_eq!(runtime.midi_index_usage().free_versions, 2);
    }
}
