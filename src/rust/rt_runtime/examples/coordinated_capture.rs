//! Reserve a stereo/MIDI prefix before rendering and retain the cleanup ledger.

use rt_runtime::buffer::{AudioBlock, MidiBlock, MidiBuffer};
use rt_runtime::executor::{GraphRuntime, PreparedNode};
use rt_runtime::graph::{Connection, Endpoint, GraphDescription, PortCounts, PortType};
use rt_runtime::midi_state::MidiState;
use rt_runtime::planner::{
    ConsumerAccess, DecisionBinding, FinishContext, PlanContext, Planner, PlannerSpec,
};
use rt_runtime::processor::{ProcessBlock, Processor};
use rt_runtime::resource::*;
use rt_runtime::{Budgets, Config, Error, Frame, NodeId, PlannerId, ResourceId, Revision};
use shoop_wasm_test_support::heap::{assert_no_heap, TrackingAllocator};

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

fn node() -> NodeId {
    NodeId::from_parts(0, 1).unwrap()
}
fn planner() -> PlannerId {
    PlannerId::from_parts(0, 1).unwrap()
}
fn initial() -> MidiState {
    let mut state = MidiState::default();
    state.apply_packet(&[0x90, 70, 99], 3).unwrap();
    state
}
#[derive(Clone, Copy, Default)]
struct Decision {
    interval: Option<CaptureIntervalId>,
    members: [usize; 2],
    failed: bool,
}
struct Clock {
    group: CaptureGroupId,
    take: CaptureTakeId,
    decisions: [Decision; 1],
    old: [Snapshot; 2],
    extra: [Option<Snapshot>; 4],
    frozen: [Option<Snapshot>; 2],
    last: Option<CaptureIntervalId>,
    done: bool,
}
impl Clock {
    fn freeze(
        &mut self,
        resources: &mut PlannerResourceIo<'_>,
        at: Frame,
        incomplete: Option<Error>,
    ) -> Result<(), Error> {
        assert_eq!(resources.copy_remaining(), 0);
        assert_eq!(
            resources.snapshot(self.old[0].resource, at),
            Err(Error::Capacity)
        );
        let mut occupied = [Some(self.old[0]), None];
        assert_eq!(
            resources
                .capture()
                .freeze(self.take, at, incomplete, &mut occupied),
            Err(Error::InvalidState)
        );
        resources
            .capture()
            .freeze(self.take, at, incomplete, &mut self.frozen)?;
        assert_eq!(
            resources.capture().info(self.group)?.phase,
            CapturePhase::Idle
        );
        assert_eq!(
            resources.snapshot(self.old[0].resource, at),
            Err(Error::Capacity)
        );
        self.done = true;
        self.decisions[0].failed = incomplete.is_some();
        Ok(())
    }
}
impl Planner for Clock {
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
        if context.frame == context.callback_start {
            for _ in 0..2 {
                let mut cursor = context.resources.cursor(self.old[0].id)?;
                assert_eq!(context.resources.read(&mut cursor, &mut [0; 8])?, 8);
            }
            assert_eq!(context.resources.copy_remaining(), 0);
        }
        self.decisions[0].interval = None;
        if !self.done && context.frame == Frame(8) {
            self.freeze(&mut context.resources, context.frame, None)?;
        }
        if !self.done {
            let first = context.frame == Frame(0);
            let reservation = if first {
                context
                    .resources
                    .capture()
                    .reserve_prepared(self.take, context.frame, 1)
            } else {
                context
                    .resources
                    .capture()
                    .reserve(self.take, context.frame, 1)
            };
            match reservation {
                Ok(interval) => {
                    if first {
                        assert_eq!(
                            context.resources.capture().info(self.group)?.phase,
                            CapturePhase::PreparedReserved
                        );
                        for old in self.old {
                            assert_eq!(
                                context.resources.info(old.resource)?.revision,
                                old.revision
                            );
                            assert_eq!(
                                context.resources.info(old.resource)?.description,
                                old.description
                            );
                        }
                        assert_eq!(
                            context
                                .resources
                                .render()
                                .stage_audio(interval, 0, &[0.125, -0.125]),
                            Err(Error::InvalidState)
                        );
                        assert_eq!(
                            context
                                .resources
                                .capture()
                                .activate(self.take, context.frame.checked_add(1)?),
                            Err(Error::InvalidFrame)
                        );
                        context
                            .resources
                            .capture()
                            .activate(self.take, context.frame)?;
                        let info = context.resources.capture().info(self.group)?;
                        assert_eq!(info.phase, CapturePhase::Reserved);
                        assert_eq!(info.interval_sequence, 2);
                    }
                    if let Some(previous) = self.last {
                        assert_ne!(previous, interval);
                        assert_eq!(
                            context
                                .resources
                                .render()
                                .stage_audio(previous, 0, &[0.0; 2]),
                            Err(Error::InvalidId)
                        );
                    }
                    self.decisions[0].interval = Some(interval);
                }
                Err(Error::Capacity) if !first => {
                    assert_eq!(context.frame, Frame(2));
                    for old in self.old {
                        assert_eq!(context.resources.info(old.resource)?.description.frames, 2);
                        assert_eq!(
                            context
                                .resources
                                .info(old.resource)?
                                .capture
                                .unwrap()
                                .next_frame,
                            Frame(2)
                        );
                    }
                    self.freeze(&mut context.resources, context.frame, Some(Error::Capacity))?;
                }
                Err(error) => return Err(error),
            }
        }
        Ok(context.frame.checked_add(1)?.min(context.ceiling))
    }
    fn finish(&mut self, mut context: FinishContext<'_, ()>) -> Result<(), Error> {
        assert!(context.observations.is_empty());
        if let Some(interval) = self.decisions[0].interval.take() {
            context.resources.capture().commit(interval)?;
            assert_eq!(
                context.resources.capture().commit(interval),
                Err(Error::InvalidState)
            );
            for old in self.old {
                let info = context.resources.info(old.resource)?;
                assert_eq!(info.description.frames, context.interval.end.0);
                assert_eq!(info.capture.unwrap().next_frame, context.interval.end);
            }
            self.last = Some(interval);
        }
        Ok(())
    }
}
struct Capture {
    resources: [ResourceId; 2],
    packets: MidiBuffer,
    over_bound: MidiBuffer,
}
impl Processor<Clock> for Capture {
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
    fn process(&mut self, block: &mut ProcessBlock<'_, Clock>) -> Result<(), Error> {
        assert_eq!(block.frames, 1);
        let decision = *block.decision.as_ref().ok_or(Error::InvalidState)?.value;
        let sample = {
            let (input, output) = block.audio.pair(0, 0).ok_or(Error::InvalidShape)?;
            output[0] = input[0] * if decision.failed { 2.0 } else { 1.0 };
            output[0]
        };
        block.audio.output(1).ok_or(Error::InvalidShape)?.fill(7.0);
        if let Some(interval) = decision.interval {
            let before = [
                block.resources.info(self.resources[0])?,
                block.resources.info(self.resources[1])?,
            ];
            let [audio, midi] = decision.members;
            block
                .resources
                .render()
                .stage_audio(interval, audio, &[sample, -sample])?;
            if block.frame == Frame(0) {
                assert_eq!(
                    block
                        .resources
                        .render()
                        .stage_audio(interval, audio, &[sample, -sample]),
                    Err(Error::InvalidState)
                );
                assert_eq!(
                    block
                        .resources
                        .render()
                        .stage_midi(interval, midi, &self.over_bound),
                    Err(Error::Capacity)
                );
            }
            self.packets.clear();
            if block.frame.0 % 2 == 0 {
                self.packets.push(
                    0,
                    1,
                    &[
                        0x90,
                        60 + (block.frame.0 / 2) as u8,
                        64 + block.frame.0 as u8,
                    ],
                )?;
            }
            block
                .resources
                .render()
                .stage_midi(interval, midi, &self.packets)?;
            assert_eq!(
                [
                    block.resources.info(self.resources[0])?,
                    block.resources.info(self.resources[1])?
                ],
                before
            );
        }
        Ok(())
    }
}

fn old_media(
    store: &mut ResourceStore,
    resources: [ResourceId; 2],
) -> Result<[Snapshot; 2], Error> {
    let audio = store.begin_upload(
        resources[0],
        Revision(0),
        Description {
            format: Format::Audio { channels: 2 },
            frames: 1,
            bytes: 8,
        },
    )?;
    let mut samples = [0; 8];
    samples[..4].copy_from_slice(&0.5f32.to_le_bytes());
    samples[4..].copy_from_slice(&(-0.5f32).to_le_bytes());
    store.begin_callback();
    store.write_upload(audio, 0, &samples)?;
    store.finish_upload(audio)?;
    store.commit(audio)?;
    let midi = store.begin_indexed_midi_upload(
        resources[1],
        Revision(0),
        Description {
            format: Format::Midi,
            frames: 1,
            bytes: 15,
        },
        &MidiState::default(),
    )?;
    let mut record = [0; 15];
    encode_midi_record(0, &[0x90, 55, 70], &mut record)?;
    store.begin_callback();
    store.write_upload(midi, 0, &record)?;
    store.finish_upload(midi)?;
    store.commit(midi)?;
    Ok([
        store.snapshot(resources[0], Frame(0))?,
        store.snapshot(resources[1], Frame(0))?,
    ])
}
fn bytes(store: &mut ResourceStore, snapshot: Snapshot) -> Result<[u8; 64], Error> {
    let mut output = [0; 64];
    assert!(snapshot.description.bytes <= output.len());
    let index = store.read_index(snapshot.id)?;
    let mut cursor = store.render_io().cursor_at(index, 0)?;
    assert_no_heap(|| {
        while cursor.position() < snapshot.description.bytes {
            let position = cursor.position();
            assert!(
                store.render_io().read(
                    &mut cursor,
                    &mut output[position..snapshot.description.bytes]
                )? > 0
            );
        }
        Ok::<_, Error>(())
    })?;
    Ok(output)
}
fn render(
    runtime: &mut GraphRuntime<Capture, Clock>,
    frames: usize,
) -> Result<[[f32; 4]; 2], Error> {
    let frame = runtime.next_frame();
    let input: [f32; 4] = std::array::from_fn(|index| (frame.0 + index as u64 + 1) as f32 / 8.0);
    let mut output = [[-99.0; 4]; 2];
    let [left, right] = &mut output;
    assert_no_heap(|| {
        runtime.process(
            AudioBlock {
                frame,
                frames,
                inputs: &[&input],
                outputs: &mut [left, right],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
        )
    })?;
    for channel in &output {
        assert_eq!(&channel[frames..], &[-99.0; 4][frames..]);
    }
    Ok(output)
}

fn run(pages: usize, partition: &[usize]) -> Result<(), Error> {
    let config = Config {
        max_frames: 4,
        max_nodes: 1,
        max_ports: 8,
        max_edges: 8,
        audio_inputs: 1,
        audio_outputs: 2,
        midi_inputs: 0,
        midi_outputs: 0,
        midi_events_per_port: 2,
        midi_bytes_per_port: 6,
        resource_slots: 8,
        resource_pages: pages,
        page_bytes: 16,
        budgets: Budgets {
            transfer_bytes: 16,
            validation_steps: 128,
            subranges: 0,
            ..Budgets::default()
        },
        ..Config::default()
    }
    .prepare()?;
    let mut store = ResourceStore::with_capture_groups(
        config,
        Some(MidiIndexConfig {
            versions: 4,
            checkpoints_per_version: 4,
            replay_events: 2,
        }),
        CaptureGroupConfig {
            groups: 1,
            members_per_group: 2,
            staging_bytes_per_group: 64,
        },
    )?;
    let resources = [store.create()?, store.create()?];
    let old = old_media(&mut store, resources)?;
    let group = store.bind_capture_group(&[
        CaptureMember {
            resource: resources[0],
            format: Format::Audio { channels: 2 },
            midi_events: 0,
            midi_bytes: 0,
        },
        CaptureMember {
            resource: resources[1],
            format: Format::Midi,
            midi_events: 2,
            midi_bytes: 6,
        },
    ])?;
    let before = store.usage();
    let cancelled = store.prepare_capture_take(group, &[initial()])?;
    if pages == 4 {
        assert_eq!(
            store
                .planner_io(Frame(0), Frame(1))
                .capture()
                .reserve_prepared(cancelled, Frame(0), 1),
            Err(Error::Capacity)
        );
        assert_eq!(
            store.capture_group_info(group)?.phase,
            CapturePhase::Prepared
        );
        assert_eq!(store.usage().reserved_pages, 0);
        for snapshot in old {
            assert_eq!(store.info(snapshot.resource)?.revision, snapshot.revision);
            assert_eq!(
                store.info(snapshot.resource)?.description,
                snapshot.description
            );
        }
        store.cancel_capture_take(cancelled)?;
        store.release_capture_group(group)?;
        for snapshot in old {
            store.release_snapshot(snapshot.id)?;
        }
        for resource in resources {
            store.remove(resource)?;
        }
        store.begin_callback();
        store.reclaim();
        recovered(&store, pages);
        return Ok(());
    }
    let interval = store
        .planner_io(Frame(0), Frame(1))
        .capture()
        .reserve_prepared(cancelled, Frame(0), 1)?;
    assert_eq!(store.usage().reserved_pages, 3);
    assert_eq!(
        store.cancel_capture_take(cancelled),
        Err(Error::InvalidState)
    );
    store.cancel_capture_interval(interval)?;
    assert_eq!(
        store.capture_group_info(group)?.phase,
        CapturePhase::Prepared
    );
    store.cancel_capture_take(cancelled)?;
    store.begin_callback();
    store.reclaim();
    assert_eq!(store.usage(), before);
    let take = store.prepare_capture_take(group, &[initial()])?;
    assert_ne!(take, cancelled);
    assert_eq!(
        store.reserve_prepared_capture_interval(cancelled, Frame(0), 1),
        Err(Error::InvalidId)
    );
    for snapshot in old {
        assert_eq!(store.info(snapshot.resource)?.revision, snapshot.revision);
    }
    let mut extra = [None; 4];
    for lease in &mut extra {
        *lease = Some(store.snapshot(resources[0], Frame(0))?);
    }
    assert_eq!(store.usage().free_snapshots, 0);
    let clock = Clock {
        group,
        take,
        decisions: [Decision {
            members: [0, 1],
            ..Decision::default()
        }],
        old,
        extra,
        frozen: [None; 2],
        last: None,
        done: false,
    };
    let mut over_bound = MidiBuffer::new(3, 9)?;
    for note in 0..3 {
        over_bound.push(0, 1, &[0x90, note, 1])?;
    }
    let capture = Capture {
        resources,
        packets: MidiBuffer::new(2, 6)?,
        over_bound,
    };
    let graph = GraphDescription {
        nodes: vec![node()],
        connections: vec![
            Connection {
                source: Endpoint::host(PortType::Audio, 0),
                destination: Endpoint::node(node(), PortType::Audio, 0),
            },
            Connection {
                source: Endpoint::node(node(), PortType::Audio, 0),
                destination: Endpoint::host(PortType::Audio, 0),
            },
            Connection {
                source: Endpoint::node(node(), PortType::Audio, 1),
                destination: Endpoint::host(PortType::Audio, 1),
            },
        ],
    };
    let (control, mut runtime) = GraphRuntime::prepare_with_initial_graph(
        config,
        vec![PreparedNode::new(node(), capture)],
        clock,
        store,
        &graph,
        &[DecisionBinding {
            planner: planner(),
            consumer: node(),
            decision_slot: 0,
        }],
    )?;
    let prefix = if pages == 6 { 2 } else { 8 };
    for &frames in partition {
        let start = runtime.next_frame();
        if start == Frame(2) {
            let before = runtime.resource_usage();
            render(&mut runtime, 0)?;
            assert_eq!(runtime.resource_usage(), before);
        }
        let output = render(&mut runtime, frames)?;
        for offset in 0..frames {
            let frame = start.0 + offset as u64;
            let gain = if pages == 6 && frame >= 2 { 2.0 } else { 1.0 };
            assert_eq!(output[0][offset], (frame + 1) as f32 / 8.0 * gain);
            assert_eq!(output[1][offset], 7.0);
        }
        assert_eq!(runtime.resource_usage().reserved_pages, 0);
    }
    assert_eq!(runtime.next_frame(), Frame(8));
    let before = runtime.resource_usage();
    render(&mut runtime, 0)?;
    assert_eq!(runtime.resource_usage(), before);
    render(&mut runtime, 1)?;
    assert_eq!(control.outstanding(), 0);
    let (mut clock, mut store) = runtime.into_planner_and_resources();
    assert!(clock.done && clock.decisions[0].interval.is_none());
    let [Some(audio), Some(midi)] = clock.frozen else {
        panic!("missing promised final leases")
    };
    let incomplete = if pages == 6 {
        Some(Error::Capacity)
    } else {
        None
    };
    for snapshot in [audio, midi] {
        assert_eq!(snapshot.description.frames, prefix);
        assert_eq!(snapshot.cut_frame, Frame(prefix));
        assert_eq!(snapshot.capture.unwrap().incomplete, incomplete);
        assert!(!snapshot.capture.unwrap().active);
    }
    assert_eq!(store.capture_group_info(group)?.phase, CapturePhase::Idle);
    assert_eq!(store.usage().reserved_pages, 0);
    let used = 2 + (prefix as usize * 8).div_ceil(16) + (prefix as usize / 2 * 15).div_ceil(16);
    assert_eq!(store.usage().free_pages, pages - used);
    let samples = bytes(&mut store, audio)?;
    assert_eq!(audio.description.bytes, prefix as usize * 8);
    for frame in 0..prefix as usize {
        let sample = (frame + 1) as f32 / 8.0;
        assert_eq!(&samples[frame * 8..frame * 8 + 4], &sample.to_le_bytes());
        assert_eq!(
            &samples[frame * 8 + 4..frame * 8 + 8],
            &(-sample).to_le_bytes()
        );
    }
    let packets = bytes(&mut store, midi)?;
    let expected_packets = [
        [0x90, 60, 64],
        [0x90, 61, 66],
        [0x90, 62, 68],
        [0x90, 63, 70],
    ];
    assert_eq!(midi.description.bytes, prefix as usize / 2 * 15);
    for event in 0..prefix as usize / 2 {
        let start = event * 15;
        assert_eq!(
            &packets[start..start + 8],
            &(event as u64 * 2).to_le_bytes()
        );
        assert_eq!(&packets[start + 8..start + 12], &3u32.to_le_bytes());
        assert_eq!(&packets[start + 12..start + 15], &expected_packets[event]);
    }
    let index = store.read_index(midi.id)?;
    assert_eq!(store.render_io().midi_seek(index, 0)?.state, initial());
    let state = store.render_io().midi_seek(index, prefix)?.state;
    for packet in expected_packets.iter().take(prefix as usize / 2) {
        assert_eq!(
            state
                .channel(0)
                .unwrap()
                .note(packet[1])
                .unwrap()
                .velocity(),
            packet[2]
        );
    }
    for old in clock.old {
        assert_eq!(store.currentness(old.id)?, Currentness::Stale);
    }
    let old_audio = bytes(&mut store, clock.old[0])?;
    assert_eq!(&old_audio[..4], &0.5f32.to_le_bytes());
    assert_eq!(&old_audio[4..8], &(-0.5f32).to_le_bytes());
    let old_midi = store.read_index(clock.old[1].id)?;
    assert_eq!(
        store
            .render_io()
            .midi_seek(old_midi, 1)?
            .state
            .channel(0)
            .unwrap()
            .note(55)
            .unwrap()
            .velocity(),
        70
    );
    assert_eq!(store.remove(resources[0]), Err(Error::InvalidState));
    store.release_capture_group(group)?;
    for lease in clock.frozen.iter_mut().chain(clock.extra.iter_mut()) {
        store.release_snapshot(lease.take().unwrap().id)?;
    }
    for snapshot in clock.old {
        store.release_snapshot(snapshot.id)?;
    }
    for resource in resources {
        store.remove(resource)?;
    }
    store.begin_callback();
    store.reclaim();
    recovered(&store, pages);
    drop((clock, store, control));
    Ok(())
}
fn recovered(store: &ResourceStore, pages: usize) {
    assert_eq!(store.usage().free_pages, pages);
    assert_eq!(store.usage().reserved_pages, 0);
    assert_eq!(store.usage().free_versions, 8);
    assert_eq!(store.usage().free_snapshots, 8);
    assert_eq!(store.midi_index_usage().free_versions, 4);
    assert_eq!(store.capture_group_usage().free_groups, 1);
}
// Coordinated capture: record audio and MIDI together in one capture group, across intervals.
fn main() -> Result<(), Error> {
    run(4, &[])?;
    for pages in [24, 6] {
        run(pages, &[4, 4])?;
        run(pages, &[1; 8])?;
        run(pages, &[3, 1, 2, 2])?;
    }
    println!("coordinated capture: reserved first cut, exact stereo/MIDI prefixes, pre-render refusal and complete ownership cleanup verified");
    Ok(())
}
