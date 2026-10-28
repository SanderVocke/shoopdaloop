//! Test-only planner lifecycle fault injection around the real loop resolver.
use crate::{allocation_guard, journal::Journal, wire, HEAP_FAILURE};
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock},
    executor::{Control, GraphRuntime, PreparedNode},
    graph::{Connection, Endpoint, GraphDescription, PortCounts, PortType},
    planner::{ConsumerAccess, DecisionBinding, FinishContext, PlanContext, Planner, PlannerSpec},
    processor::{ProcessBlock, Processor},
    resource::ResourceStore,
    Budgets, Config, Error, Frame, NodeId, PlannerId,
};
use shoop_loop_engine::{
    compile, runtime::BoundaryObserver, Decision, Limits, LoopId, LoopKind, LoopSpec, Mode,
    Request, Resolver, Sequence, Timing, TopologyDescription, Transition,
};
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[derive(Default)]
struct Counts {
    plans: AtomicU32,
    finishes: AtomicU32,
    witness: AtomicU32,
}
struct Clock {
    resolver: Resolver,
    decisions: [Decision; 1],
    journal: Journal,
    fault: u8,
    counts: Arc<Counts>,
}
fn planner() -> PlannerId {
    PlannerId::from_parts(0, 1).unwrap()
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
    fn plan(&mut self, context: PlanContext<'_>) -> Result<Frame, Error> {
        self.counts.plans.store(
            self.counts.plans.load(Ordering::Relaxed).saturating_add(1),
            Ordering::Relaxed,
        );
        if context.frame != self.resolver.frame() {
            return Err(Error::InvalidFrame);
        }
        let interval = self
            .resolver
            .preview(context.ceiling, &[])
            .map_err(|_| Error::InvalidState)?;
        if self.fault == 1 {
            self.counts.witness.store(3, Ordering::Relaxed);
            return Err(Error::InvalidState);
        }
        if self.fault == 2 && self.counts.plans.load(Ordering::Relaxed) == 1 {
            self.counts.witness.store(4, Ordering::Relaxed);
            let mut bytes = Vec::with_capacity(1);
            for index in 0..128 {
                bytes.push(std::hint::black_box(index as u8));
            }
            drop(std::hint::black_box(bytes));
        }
        self.decisions
            .copy_from_slice(self.resolver.decisions().map_err(|_| Error::InvalidState)?);
        self.resolver.commit().map_err(|_| Error::InvalidState)?;
        self.journal.boundary(
            context.frame,
            &self.decisions,
            self.resolver.events(),
            self.resolver.dropped_events(),
        );
        Ok(interval.end)
    }
    fn finish(&mut self, context: FinishContext<'_, ()>) -> Result<(), Error> {
        if !context.observations.is_empty() {
            return Err(Error::InvalidState);
        }
        self.resolver
            .finish(context.interval.end)
            .map_err(|_| Error::InvalidState)?;
        self.counts.finishes.store(
            self.counts
                .finishes
                .load(Ordering::Relaxed)
                .saturating_add(1),
            Ordering::Relaxed,
        );
        Ok(())
    }
}
impl Drop for Clock {
    fn drop(&mut self) {
        assert!(!allocation_guard::is_rendering());
    }
}
struct Half;
impl Drop for Half {
    fn drop(&mut self) {
        assert!(!allocation_guard::is_rendering());
    }
}
impl Processor<Clock> for Half {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 2,
            audio_outputs: 2,
            ..PortCounts::default()
        }
    }
    fn planning(&self) -> ConsumerAccess {
        ConsumerAccess {
            decision_required: true,
            ..ConsumerAccess::default()
        }
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, Clock>) -> Result<(), Error> {
        let view = block.decision.ok_or(Error::InvalidState)?;
        if view.planner != planner()
            || view.slot != 0
            || view.interval.start != block.frame
            || view.value.state.mode != Mode::Playing
            || view.value.state.position != 0
            || view.value.state.cycles != block.frame.0
            || block.frames != 1
        {
            return Err(Error::InvalidState);
        }
        for channel in 0..2 {
            let (input, output) = block
                .audio
                .pair(channel, channel)
                .ok_or(Error::InvalidShape)?;
            output[0] = input[0] * 0.5;
        }
        Ok(())
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct BrowserPlannerCanary {
    runtime: Option<GraphRuntime<Half, Clock>>,
    control: Option<Control<Half, Clock>>,
    counts: Arc<Counts>,
    input: [[f32; 128]; 2],
    output: [[f32; 128]; 2],
    journal: Box<[u8]>,
    journal_len: usize,
    heap: allocation_guard::Counts,
    frame: u64,
    callbacks: u64,
    faulted: bool,
    recovered: [u32; 3],
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl BrowserPlannerCanary {
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(constructor))]
    pub fn new(fault: u8) -> Result<Self, u32> {
        if fault > 2 {
            return Err(5);
        }
        let config = Config {
            max_nodes: 1,
            max_ports: 8,
            max_edges: 4,
            midi_inputs: 0,
            midi_outputs: 0,
            midi_events_per_port: 1,
            midi_bytes_per_port: 4,
            resource_slots: 4,
            resource_pages: 8,
            page_bytes: 64,
            command_slots: 1,
            scheduled_events: 1,
            budgets: Budgets {
                commands: 1,
                events: 1,
                subranges: 0,
                transfer_bytes: 64,
                validation_steps: 1,
                publications: 1,
            },
            ..Config::default()
        }
        .prepare()
        .map_err(wire::runtime_error)?;
        let target = LoopId::from_parts(0, 1).unwrap();
        let topology = compile(
            Limits {
                loop_slots: 1,
                pending_requests: 1,
                journal_events: 4,
                ..Limits::default()
            },
            &TopologyDescription {
                loops: vec![LoopSpec {
                    id: target,
                    kind: LoopKind::Primitive,
                    length: 1,
                    sync_source: None,
                }],
                ..TopologyDescription::default()
            },
        )
        .unwrap();
        let mut resolver = Resolver::prepare(topology).unwrap();
        let decisions = [Decision {
            state: resolver.state(target).unwrap(),
            playback_discontinuity: false,
            origin: None,
        }];
        resolver
            .submit(Request {
                target,
                sequence: Sequence(1),
                at: Frame(0),
                timing: Timing::At,
                transition: Transition::Play {
                    position: 0,
                    retrigger: false,
                },
            })
            .unwrap();
        let counts = Arc::new(Counts::default());
        let clock = Clock {
            resolver,
            decisions,
            journal: Journal::new(512),
            fault,
            counts: counts.clone(),
        };
        let node = NodeId::from_parts(0, 1).unwrap();
        let mut graph = GraphDescription {
            nodes: vec![node],
            connections: vec![],
        };
        for channel in 0..2 {
            graph.connections.push(Connection {
                source: Endpoint::host(PortType::Audio, channel),
                destination: Endpoint::node(node, PortType::Audio, channel),
            });
            graph.connections.push(Connection {
                source: Endpoint::node(node, PortType::Audio, channel),
                destination: Endpoint::host(PortType::Audio, channel),
            });
        }
        let (control, runtime) = GraphRuntime::prepare_with_initial_graph(
            config,
            vec![PreparedNode::new(node, Half)],
            clock,
            ResourceStore::new(config).map_err(wire::runtime_error)?,
            &graph,
            &[DecisionBinding {
                planner: planner(),
                consumer: node,
                decision_slot: 0,
            }],
        )
        .map_err(wire::runtime_error)?;
        Ok(Self {
            runtime: Some(runtime),
            control: Some(control),
            counts,
            input: [[0.; 128]; 2],
            output: [[0.; 128]; 2],
            journal: vec![0; crate::journal::HEADER + 512 * crate::journal::STRIDE]
                .into_boxed_slice(),
            journal_len: 0,
            heap: allocation_guard::Counts::default(),
            frame: 0,
            callbacks: 0,
            faulted: false,
            recovered: [0; 3],
        })
    }
    pub fn render(&mut self, frames: u32) -> u32 {
        if self.faulted || self.runtime.is_none() {
            return 10;
        }
        if frames > 128 {
            return 4;
        }
        let (result, heap) = allocation_guard::measure(|| {
            self.output.iter_mut().for_each(|channel| channel.fill(0.));
            for offset in 0..frames as usize {
                let Some(at) = self
                    .frame
                    .checked_add(offset as u64)
                    .and_then(|v| v.checked_add(1))
                else {
                    return wire::runtime_error(Error::InvalidFrame);
                };
                let value = 2. * at as f32;
                self.input[0][offset] = value;
                self.input[1][offset] = value * 10.;
            }
            let [left, right] = &mut self.output;
            let runtime = self.runtime.as_mut().unwrap();
            let result = runtime.process(
                AudioBlock {
                    frame: Frame(self.frame),
                    frames: frames as usize,
                    inputs: &[
                        &self.input[0][..frames as usize],
                        &self.input[1][..frames as usize],
                    ],
                    outputs: &mut [&mut left[..frames as usize], &mut right[..frames as usize]],
                },
                MidiBlock {
                    inputs: &[],
                    outputs: &mut [],
                },
            );
            self.frame = runtime.next_frame().0;
            self.callbacks = self.callbacks.saturating_add(u64::from(frames != 0));
            result.map_or_else(wire::runtime_error, |_| 0)
        });
        self.heap = heap;
        self.faulted = result != 0 || heap != allocation_guard::Counts::default();
        if heap != allocation_guard::Counts::default() {
            HEAP_FAILURE
        } else {
            result
        }
    }
    pub fn output_ptr(&self, channel: u32) -> usize {
        self.output
            .get(channel as usize)
            .map_or(0, |v| v.as_ptr() as usize)
    }
    pub fn marker(&self) -> u32 {
        crate::browser::instance_marker()
    }
    pub fn frame(&self) -> u64 {
        self.frame
    }
    pub fn callbacks(&self) -> u64 {
        self.callbacks
    }
    pub fn plans(&self) -> u32 {
        self.counts.plans.load(Ordering::Relaxed)
    }
    pub fn finishes(&self) -> u32 {
        self.counts.finishes.load(Ordering::Relaxed)
    }
    pub fn fault_witness(&self) -> u32 {
        self.counts.witness.load(Ordering::Relaxed)
    }
    pub fn heap_allocations(&self) -> u32 {
        self.heap.allocations as u32
    }
    pub fn heap_reallocations(&self) -> u32 {
        self.heap.reallocations as u32
    }
    pub fn heap_deallocations(&self) -> u32 {
        self.heap.deallocations as u32
    }
    pub fn semantic_ptr(&self) -> usize {
        self.journal.as_ptr() as usize
    }
    pub fn semantic_len(&self) -> u32 {
        self.journal_len as u32
    }
    pub fn recovered_pages(&self) -> u32 {
        self.recovered[0]
    }
    pub fn recovered_versions(&self) -> u32 {
        self.recovered[1]
    }
    pub fn recovered_snapshots(&self) -> u32 {
        self.recovered[2]
    }
    pub fn stop(&mut self) -> u32 {
        let Some(runtime) = self.runtime.take() else {
            return 10;
        };
        drop(self.control.take());
        let (planner, resources) = runtime.into_planner_and_resources();
        let bytes = planner.journal.bytes();
        self.journal[..bytes.len()].copy_from_slice(bytes);
        self.journal_len = bytes.len();
        let usage = resources.usage();
        self.recovered = [
            usage.free_pages as u32,
            usage.free_versions as u32,
            usage.free_snapshots as u32,
        ];
        0
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn report() -> String {
    let mut host = BrowserPlannerCanary::new(0).unwrap();
    assert_eq!(host.render(128), 0);
    let samples: Vec<_> = (0..128)
        .map(|i| [host.output[0][i], host.output[1][i]])
        .collect();
    assert_eq!(host.stop(), 0);
    assert_eq!(&host.journal[..host.journal_len], expected_semantic());
    assert_eq!(
        samples,
        (1..=128)
            .map(|v| [v as f32, v as f32 * 10.])
            .collect::<Vec<_>>()
    );
    format!("{{\"samples\":{:?},\"semanticBytes\":{:?},\"recovered\":{:?},\"frames\":{},\"callbacks\":{},\"plans\":{},\"finishes\":{}}}", samples, &host.journal[..host.journal_len], host.recovered, host.frame, host.callbacks, host.plans(), host.finishes())
}
#[cfg(any(test, not(target_arch = "wasm32")))]
fn expected_semantic() -> Vec<u8> {
    let mut bytes = vec![0; 40];
    bytes[..8].copy_from_slice(b"LSJ1\x01\0\xa0\0");
    crate::export_wire::put32(&mut bytes, 8, 512);
    for frame in 0..128 {
        let mut boundary = [0; 160];
        boundary[0] = 1;
        crate::export_wire::put64(&mut boundary, 8, frame);
        crate::export_wire::put32(&mut boundary, 16, 1);
        crate::export_wire::put32(&mut boundary, 20, 2);
        bytes.extend(boundary);
        let mut decision = [0; 160];
        decision[..3].copy_from_slice(&[2, 1, if frame == 0 { 73 } else { 9 }]);
        for (offset, value) in [(8, frame), (24, 1), (48, frame), (72, frame)] {
            crate::export_wire::put64(&mut decision, offset, value);
        }
        crate::export_wire::put32(&mut decision, 20, 1);
        bytes.extend(decision);
        for code in [if frame == 0 { 2 } else { 9 }, 10] {
            let mut event = [0; 160];
            event[0] = 3;
            event[1] = code;
            crate::export_wire::put64(&mut event, 8, frame);
            crate::export_wire::put32(&mut event, 20, 1);
            bytes.extend(event);
        }
    }
    bytes
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resolver_planner_control_is_partition_invariant_with_complete_boundary_tables() {
        let mut reference = None;
        for parts in [vec![128], vec![1; 128], vec![0, 3, 0, 62, 17, 0, 46, 0]] {
            let mut host = BrowserPlannerCanary::new(0).unwrap();
            assert_eq!(host.plans(), 0);
            let mut samples = vec![];
            for frames in parts {
                assert_eq!(host.render(frames), 0);
                for i in 0..frames as usize {
                    samples.push([host.output[0][i], host.output[1][i]]);
                }
            }
            assert_eq!(host.plans(), 128);
            assert_eq!(host.finishes(), 128);
            assert_eq!(
                samples,
                (1..=128)
                    .map(|v| [v as f32, v as f32 * 10.])
                    .collect::<Vec<_>>()
            );
            assert_eq!(host.stop(), 0);
            assert_eq!(host.recovered, [8, 4, 4]);
            let bytes = &host.journal[..host.journal_len];
            assert_eq!(bytes, expected_semantic());
            assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), 512);
            if let Some(original) = &reference {
                assert_eq!(bytes, original);
            } else {
                reference = Some(bytes.to_vec());
            }
        }
    }
    #[test]
    fn planner_errors_and_heap_faults_have_exact_lifecycle_origins_and_are_not_successful_cleanup()
    {
        for fault in [1, 2] {
            let mut host = BrowserPlannerCanary::new(fault).unwrap();
            assert_eq!(host.render(128), if fault == 1 { 10 } else { HEAP_FAILURE });
            assert_eq!(host.fault_witness(), if fault == 1 { 3 } else { 4 });
            assert_eq!(host.frame(), 128);
            assert_eq!(host.callbacks(), 1);
            if fault == 1 {
                assert!(host.output.iter().flatten().all(|v| *v == 0.));
            }
            assert_eq!(host.plans(), if fault == 1 { 1 } else { 128 });
            assert_eq!(host.finishes(), if fault == 1 { 0 } else { 128 });
            if fault == 1 {
                assert_eq!(host.heap, allocation_guard::Counts::default());
            } else {
                assert!(
                    host.heap.allocations > 0
                        && host.heap.reallocations > 0
                        && host.heap.deallocations > 0
                );
            }
            assert_eq!(host.render(1), 10);
            assert_eq!(host.stop(), 0);
            assert_eq!(host.recovered, [8, 4, 4]);
        }
    }
}
