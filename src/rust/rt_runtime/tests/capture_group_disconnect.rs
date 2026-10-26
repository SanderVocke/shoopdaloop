mod support;
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    executor::{Command, GraphRuntime, PreparedNode},
    graph::PortCounts,
    midi_state::MidiState,
    planner::*,
    processor::{ProcessBlock, Processor},
    resource::*,
    schedule::Action,
    transport::Lane,
    Budgets, Config, Error, Frame, NodeId, PlannerId,
};
use shoop_wasm_test_support::shoop_test;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use support::assert_no_heap;
#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
fn pid() -> PlannerId {
    PlannerId::from_parts(0, 1).unwrap()
}
const CUTS: [&str; 17] = [
    "unbound",
    "bound",
    "prepared",
    "activated",
    "reserved",
    "audio-staged",
    "all-staged",
    "committed",
    "frozen",
    "first-lease-released",
    "both-leases-released",
    "group-released",
    "prepared-cancelled",
    "interval-cancelled",
    "prepared-interval-reserved",
    "prepared-interval-cancelled",
    "prepared-interval-activated",
];
struct Owner {
    cut: usize,
    stopped: bool,
    group: Option<CaptureGroupId>,
    take: Option<CaptureTakeId>,
    interval: Option<CaptureIntervalId>,
    snapshots: [Option<Snapshot>; 2],
}
impl Planner for Owner {
    type Decision = ();
    type Observation = ();
    type Operation = bool;
    type Result = ();
    fn spec(&self) -> Option<PlannerSpec> {
        Some(PlannerSpec {
            id: pid(),
            decision_slots: 0,
            observation_capacity: 0,
            operation_bytes: 1,
        })
    }
    fn decisions(&self) -> &[()] {
        &[]
    }
    fn recovery_safe(stop: bool) -> bool {
        stop
    }
    fn operation(
        &mut self,
        stop: bool,
        context: OperationContext<'_, bool, ()>,
    ) -> Result<Disposition<()>, Error> {
        assert!(stop);
        assert!(context.recovery);
        assert!(context.payload.is_empty());
        self.stopped = true;
        Ok(Disposition::Complete(()))
    }
    fn plan(&mut self, mut context: PlanContext<'_, bool, ()>) -> Result<Frame, Error> {
        if self.stopped {
            if let Some(interval) = self.interval.take() {
                context.resources.capture().cancel_interval(interval)?;
            }
            if let Some(group) = self.group {
                match context.resources.capture().info(group)?.phase {
                    CapturePhase::Prepared => context
                        .resources
                        .capture()
                        .cancel_take(self.take.take().unwrap())?,
                    CapturePhase::Recording => context.resources.capture().freeze(
                        self.take.take().unwrap(),
                        context.frame,
                        None,
                        &mut self.snapshots,
                    )?,
                    CapturePhase::Idle => {}
                    CapturePhase::Reserved | CapturePhase::PreparedReserved => {
                        return Err(Error::InvalidState)
                    }
                }
            }
            for snapshot in &mut self.snapshots {
                if let Some(snapshot) = snapshot.take() {
                    assert_eq!(
                        snapshot.description.frames,
                        if (7..=11).contains(&self.cut) { 1 } else { 0 }
                    );
                    context.resources.release_snapshot(snapshot.id)?;
                }
            }
        }
        context.frame.checked_add(1)
    }
    fn finish(&mut self, mut context: FinishContext<'_, (), bool, ()>) -> Result<(), Error> {
        if !self.stopped && context.interval.end == Frame(1) {
            assert!((7..=11).contains(&self.cut));
            context
                .resources
                .capture()
                .commit(self.interval.take().unwrap())?;
            if self.cut >= 8 {
                context.resources.capture().freeze(
                    self.take.take().unwrap(),
                    Frame(1),
                    None,
                    &mut self.snapshots,
                )?;
            }
            if self.cut >= 9 {
                context
                    .resources
                    .release_snapshot(self.snapshots[0].take().unwrap().id)?;
            }
            if self.cut >= 10 {
                context
                    .resources
                    .release_snapshot(self.snapshots[1].take().unwrap().id)?;
            }
            if self.cut >= 11 {
                context
                    .resources
                    .capture()
                    .release_group(self.group.take().unwrap())?;
            }
        }
        Ok(())
    }
}
#[derive(Default)]
struct Drops {
    render: AtomicUsize,
    control: AtomicUsize,
}
struct NeverRun(Arc<Drops>);
impl Processor<Owner> for NeverRun {
    fn ports(&self) -> PortCounts {
        PortCounts::default()
    }
    fn process(&mut self, _: &mut ProcessBlock<'_, Owner>) -> Result<(), Error> {
        panic!("unbound processor executed");
    }
}
impl Drop for NeverRun {
    fn drop(&mut self) {
        if support::is_rendering() {
            self.0.render.fetch_add(1, Ordering::Relaxed);
        } else {
            self.0.control.fetch_add(1, Ordering::Relaxed);
        }
    }
}
fn step(runtime: &mut GraphRuntime<NeverRun, Owner>) {
    let frame = runtime.next_frame();
    assert_no_heap(|| {
        runtime.process(
            AudioBlock {
                frame,
                frames: 1,
                inputs: &[],
                outputs: &mut [],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
        )
    })
    .unwrap();
}
#[shoop_test]
fn disconnected_control_preserves_capture_owners_at_every_lifecycle_cut() {
    for (cut, name) in CUTS.into_iter().enumerate() {
        let cfg = Config {
            max_frames: 4,
            max_nodes: 1,
            max_ports: 2,
            max_edges: 2,
            audio_inputs: 0,
            audio_outputs: 0,
            midi_inputs: 0,
            midi_outputs: 0,
            midi_events_per_port: 2,
            midi_bytes_per_port: 6,
            resource_slots: 8,
            resource_pages: 64,
            page_bytes: 16,
            budgets: Budgets {
                transfer_bytes: 16,
                validation_steps: 64,
                ..Budgets::default()
            },
            ..Config::default()
        }
        .prepare()
        .unwrap();
        let mut store = ResourceStore::with_capture_groups(
            cfg,
            Some(MidiIndexConfig {
                versions: 2,
                checkpoints_per_version: 4,
                replay_events: 4,
            }),
            CaptureGroupConfig {
                groups: 1,
                members_per_group: 2,
                staging_bytes_per_group: 62,
            },
        )
        .unwrap();
        let resources = [store.create().unwrap(), store.create().unwrap()];
        let mut owner = Owner {
            cut,
            stopped: false,
            group: None,
            take: None,
            interval: None,
            snapshots: [None; 2],
        };
        let mut midi = MidiBuffer::new(2, 6).unwrap();
        midi.push(0, 1, &[0x90, 60, 99]).unwrap();
        assert_no_heap(|| {
            if cut >= 1 {
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
                            midi_events: 2,
                            midi_bytes: 6,
                        },
                    ])
                    .unwrap();
                owner.group = Some(group);
                if cut >= 2 {
                    let take = store
                        .prepare_capture_take(group, &[MidiState::default()])
                        .unwrap();
                    owner.take = Some(take);
                    if cut == 12 {
                        store.cancel_capture_take(take).unwrap();
                        owner.take = None;
                    } else if cut >= 14 {
                        let interval = store
                            .reserve_prepared_capture_interval(take, Frame(0), 1)
                            .unwrap();
                        owner.interval = Some(interval);
                        if cut == 15 {
                            store.cancel_capture_interval(interval).unwrap();
                            owner.interval = None;
                        } else if cut == 16 {
                            store.activate_capture_take(take, Frame(0)).unwrap();
                        }
                    } else {
                        if cut >= 3 {
                            store.activate_capture_take(take, Frame(0)).unwrap();
                        }
                        if cut >= 4 {
                            let interval =
                                store.reserve_capture_interval(take, Frame(0), 1).unwrap();
                            owner.interval = Some(interval);
                            if cut >= 5 {
                                store
                                    .stage_capture_audio(interval, 0, &[0.5, -0.5])
                                    .unwrap();
                            }
                            if cut >= 6 {
                                store.stage_capture_midi(interval, 1, &midi).unwrap();
                            }
                            if cut == 13 {
                                store.cancel_capture_interval(interval).unwrap();
                                owner.interval = None;
                            }
                        }
                    }
                }
            }
        });
        let group = owner.group;
        let drops = Arc::new(Drops::default());
        let (mut control, mut runtime) = GraphRuntime::prepare_with_planner_and_resources(
            cfg,
            vec![PreparedNode::new(
                NodeId::from_parts(0, 1).unwrap(),
                NeverRun(drops.clone()),
            )],
            owner,
            store,
        )
        .unwrap();
        if (7..=11).contains(&cut) {
            step(&mut runtime);
        }
        control
            .submit(
                Lane::Recovery,
                Command::Schedule {
                    at: runtime.next_frame(),
                    action: Action::Planner {
                        planner: pid(),
                        operation: true,
                    },
                    payload: Box::new([]),
                },
            )
            .unwrap_or_else(|e| panic!("{name}: {:?}", e.error));
        drop(control);
        step(&mut runtime);
        assert_eq!(runtime.resource_usage().free_snapshots, 8, "{name}");
        assert_eq!(runtime.resource_usage().reserved_pages, 0, "{name}");
        assert_eq!(drops.render.load(Ordering::Relaxed), 0);
        assert_eq!(drops.control.load(Ordering::Relaxed), 0);
        let mut store = runtime.into_resources();
        assert_eq!(drops.render.load(Ordering::Relaxed), 0);
        assert_eq!(drops.control.load(Ordering::Relaxed), 1);
        assert_no_heap(|| {
            if let Some(group) = group {
                if cut == 11 {
                    assert_eq!(store.capture_group_info(group), Err(Error::InvalidId));
                } else {
                    assert_eq!(
                        store.capture_group_info(group).unwrap().phase,
                        CapturePhase::Idle
                    );
                    store.release_capture_group(group).unwrap();
                }
            }
            for resource in resources {
                store.remove(resource).unwrap();
            }
            for _ in 0..4 {
                store.begin_callback();
                store.reclaim();
            }
            assert_eq!(store.usage().free_pages, 64, "{name}");
            assert_eq!(store.usage().free_versions, 8, "{name}");
            assert_eq!(store.usage().free_snapshots, 8, "{name}");
            assert_eq!(store.usage().reserved_pages, 0, "{name}");
            assert_eq!(store.midi_index_usage().free_versions, 2, "{name}");
            assert_eq!(store.capture_group_usage().free_groups, 1, "{name}");
        });
    }
}
