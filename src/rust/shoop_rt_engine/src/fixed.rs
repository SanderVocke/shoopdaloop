//! Fixed-core composition of the production DSP and direct loop bank.
//!
//! These are prepared parts for engine migration. The protocol endpoint and
//! production host selection remain on the interval engine until parity.
use crate::{
    cells::{mode_code, Cells},
    node::NodeSettings,
    pool::Pool,
    EngineDescription, EngineNode, EngineOp, HostStatus, PrepareError,
};
use alloc::{sync::Arc, vec::Vec};
use core::sync::atomic::Ordering;
use rt_runtime::{
    fixed as rt,
    graph::{GraphDescription, PortCounts},
    resource::ResourceStore,
    Error, Frame, NodeId,
};
use shoop_loops::{audio::Smoothing, fixed as loops, LoopId};
use shoop_rt_protocol::DriverStatus;

pub enum Node {
    Loop(loops::Node),
    Signal(EngineNode),
}
pub struct State {
    pub bank: loops::Bank,
    cells: Arc<Cells>,
    targets: Vec<LoopId>,
}
pub type Runtime = rt::Runtime<Node, State, EngineOp>;
pub struct Prepared {
    pub config: rt::PreparedConfig,
    pub nodes: Vec<(NodeId, Node)>,
    pub state: State,
    pub resources: ResourceStore,
    pub loops: rt::CommandControl<loops::Command, loops::Reply>,
    pub host: HostStatus,
}
pub struct Engine {
    pub runtime: Runtime,
    pub loops: rt::CommandControl<loops::Command, loops::Reply>,
    pub operations: rt::OperationControl<EngineOp>,
    pub graphs: rt::GraphControl,
    pub host: HostStatus,
}

/// Prepare all physical DSP/loop slots. Initial loop owners use generation one.
pub fn prepare(
    description: &EngineDescription,
    driver: DriverStatus,
    quantum: rt::Quantum,
) -> Result<Prepared, PrepareError> {
    description.validate()?;
    let pool = Pool::new(&description.segments, description.buses as usize);
    let config = rt::Config {
        quantum,
        max_host_frames: description.max_frames,
        capacities: crate::runtime_capacities(description, &pool, quantum.frames()),
    }
    .prepare()?;
    let resources = loops::Bank::resource_store(config, pool.loops())?;
    let smoothing = match description.smoothing_frames {
        0 => Smoothing::None,
        frames => Smoothing::LinearRestart { frames },
    };
    let targets = (0..pool.loops())
        .map(|s| LoopId::from_parts(s as u32, 1))
        .collect::<Result<Vec<_>, _>>()?;
    let bindings = targets
        .iter()
        .enumerate()
        .map(|(s, &target)| loops::Binding {
            target,
            playback: pool.playback(s),
            capture: pool.capture(s),
            channels: description.strip_channels as usize,
            midi: true,
            gain: 1.,
            smoothing,
        })
        .collect();
    let parts = loops::Bank::prepare(config, resources, bindings, Vec::new())?;
    let cells = Cells::new(
        pool.strips(),
        pool.loops(),
        description.buses as usize,
        crate::stream::RING_PAGES * description.page_bytes,
        driver,
    );
    let settings = NodeSettings {
        cells: cells.clone(),
        channels: description.strip_channels as usize,
        master_channels: description.bus_channels as usize,
        frames: quantum.frames(),
        ramp: description.ramp_frames,
    };
    let mut nodes: Vec<_> = parts
        .nodes
        .into_iter()
        .map(|(id, n)| (id, Node::Loop(n)))
        .collect();
    nodes.extend(
        crate::signal_nodes(description, &pool, &settings)
            .into_iter()
            .map(|(id, n)| (id, Node::Signal(n))),
    );
    Ok(Prepared {
        config,
        nodes,
        state: State {
            bank: parts.bank,
            cells: cells.clone(),
            targets,
        },
        resources: parts.resources,
        loops: parts.control,
        host: HostStatus(cells),
    })
}
impl Prepared {
    pub fn runtime(self, graph: &GraphDescription) -> Result<Engine, Error> {
        let budget = self.config.resources().limits().budgets.commands;
        let mut runtime =
            Runtime::prepare(self.config, self.nodes, self.state, self.resources, graph)?;
        let operations = runtime.prepare_operations(
            self.config.resources().limits().command_slots,
            (budget / 2).max(1),
        )?;
        let graphs = runtime.prepare_graph_changes()?;
        Ok(Engine {
            runtime,
            loops: self.loops,
            operations,
            graphs,
            host: self.host,
        })
    }
}
impl rt::State for State {
    fn validate_graph(&self, g: &GraphDescription) -> Result<(), Error> {
        self.bank.validate_graph(g)
    }
    fn validate_activation(&self, g: &GraphDescription) -> Result<(), Error> {
        self.bank.validate_activation(g)
    }
    fn begin_callback_with_budget(
        &mut self,
        frame: Frame,
        resources: &mut ResourceStore,
        commands: usize,
    ) -> Result<(), Error> {
        self.cells.callbacks.fetch_add(1, Ordering::Relaxed);
        self.bank
            .begin_callback_with_budget(frame, resources, commands)
    }
    fn begin_tick(&mut self, tick: rt::Tick, resources: &mut ResourceStore) -> Result<(), Error> {
        self.bank.begin_tick(tick, resources)
    }
    fn end_tick(&mut self, tick: rt::Tick, resources: &mut ResourceStore) -> Result<(), Error> {
        self.bank.end_tick(tick, resources)?;
        for &target in &self.targets {
            let s = self.bank.status(target)?;
            let cell = &self.cells.loops[target.slot() as usize];
            cell.generation
                .store(target.generation(), Ordering::Relaxed);
            cell.mode.store(mode_code(s.mode), Ordering::Relaxed);
            cell.length.store(s.length, Ordering::Relaxed);
            cell.position.store(s.position, Ordering::Relaxed);
            cell.pulses.store(s.cycles, Ordering::Relaxed);
        }
        self.cells
            .next_frame
            .store(tick.end()?.0, Ordering::Relaxed);
        Ok(())
    }
}
impl rt::Processor<State, EngineOp> for Node {
    fn validate_state(&self, id: NodeId, state: &State) -> Result<(), Error> {
        match self {
            Self::Loop(n) => {
                <loops::Node as rt::Processor<loops::Bank>>::validate_state(n, id, &state.bank)
            }
            Self::Signal(n) => {
                <EngineNode as rt::Processor<loops::Bank, EngineOp>>::validate_state(
                    n,
                    id,
                    &state.bank,
                )
            }
        }
    }
    fn ports(&self) -> PortCounts {
        match self {
            Self::Loop(n) => <loops::Node as rt::Processor<loops::Bank>>::ports(n),
            Self::Signal(n) => <EngineNode as rt::Processor<loops::Bank, EngineOp>>::ports(n),
        }
    }
    fn operation(
        &mut self,
        op: EngineOp,
        ctx: rt::OperationContext<'_>,
        state: &mut State,
    ) -> Result<(), Error> {
        match self {
            Self::Loop(_) => Err(Error::InvalidEvent),
            Self::Signal(n) => <EngineNode as rt::Processor<loops::Bank, EngineOp>>::operation(
                n,
                op,
                ctx,
                &mut state.bank,
            ),
        }
    }
    fn process(&mut self, b: &mut rt::Block<'_>, state: &mut State) -> Result<(), Error> {
        match self {
            Self::Loop(n) => {
                <loops::Node as rt::Processor<loops::Bank>>::process(n, b, &mut state.bank)
            }
            Self::Signal(n) => {
                <EngineNode as rt::Processor<loops::Bank, EngineOp>>::process(n, b, &mut state.bank)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use rt_runtime::{
        buffer::{AudioBlock, MidiBlock, MidiBuffer},
        graph::{Connection, Endpoint, PortType},
        Epoch, Revision,
    };
    use shoop_rt_protocol::{DriverKind, DriverRunState};
    use shoop_wasm_test_support::{
        heap::{assert_no_heap, TrackingAllocator},
        shoop_test,
    };
    #[global_allocator]
    static ALLOCATOR: TrackingAllocator = TrackingAllocator;

    fn description(q: usize, loops: u32) -> EngineDescription {
        EngineDescription {
            sample_rate: 48000,
            max_frames: 4 * q,
            host: crate::HostPorts::numbered(2, 2, 1),
            segments: vec![crate::Segment { strips: 1, loops }],
            buses: 1,
            bus_channels: 2,
            resource_pages: 256,
            page_bytes: 256,
            midi_events: 128,
            midi_bytes: 1024,
            ramp_frames: 0,
            ..Default::default()
        }
    }
    fn driver(d: &EngineDescription) -> DriverStatus {
        DriverStatus {
            kind: DriverKind::Dummy,
            state: DriverRunState::Running,
            sample_rate: d.sample_rate,
            buffer_frames: d.max_frames as u32,
        }
    }
    fn graph(pool: &Pool) -> GraphDescription {
        let mut nodes = vec![
            pool.bus(0),
            pool.reader(),
            pool.strip_input(0),
            pool.strip_output(0),
        ];
        let mut connections = vec![];
        let mut edge = |source, destination| {
            connections.push(Connection {
                source,
                destination,
            })
        };
        for c in 0..2 {
            let ep = |node, port| Endpoint::node(node, PortType::Audio, port);
            edge(
                Endpoint::host(PortType::Audio, c),
                ep(pool.strip_input(0), c),
            );
            edge(ep(pool.strip_input(0), c + 2), ep(pool.strip_output(0), c));
            edge(ep(pool.strip_output(0), c), ep(pool.bus(0), c));
            edge(ep(pool.bus(0), c), Endpoint::host(PortType::Audio, c));
            for s in 0..pool.loops() {
                edge(ep(pool.strip_input(0), c), ep(pool.capture(s), c));
                edge(ep(pool.playback(s), c), ep(pool.fader(s), c));
                edge(ep(pool.fader(s), c), ep(pool.strip_output(0), c));
            }
        }
        let ep = |node, port| Endpoint::node(node, PortType::Midi, port);
        edge(
            Endpoint::host(PortType::Midi, 0),
            ep(pool.strip_input(0), 0),
        );
        edge(ep(pool.strip_input(0), 1), ep(pool.strip_output(0), 0));
        edge(
            ep(pool.strip_output(0), 0),
            Endpoint::host(PortType::Midi, 0),
        );
        for s in 0..pool.loops() {
            nodes.extend([pool.playback(s), pool.capture(s), pool.fader(s)]);
            edge(ep(pool.strip_input(0), 0), ep(pool.capture(s), 0));
            edge(ep(pool.playback(s), 0), ep(pool.strip_output(0), 0));
        }
        GraphDescription { nodes, connections }
    }
    fn render(
        engine: &mut Engine,
        input: &[Vec<f32>],
        output: &mut [Vec<f32>],
        mi: &[MidiBuffer],
        mo: &mut [MidiBuffer],
        frames: usize,
    ) {
        let [left, right] = output else {
            panic!("stereo")
        };
        assert_no_heap(|| {
            engine.runtime.process(
                AudioBlock {
                    frame: engine.runtime.next_frame(),
                    frames,
                    inputs: &[&input[0], &input[1]],
                    outputs: &mut [&mut left[..], &mut right[..]],
                },
                MidiBlock {
                    inputs: mi,
                    outputs: mo,
                },
            )
        })
        .unwrap();
    }
    fn cleanup(mut e: Engine, mut imported: impl FnMut(&mut ResourceStore)) {
        e.runtime.shutdown_operations(&mut e.operations).unwrap();
        e.runtime.shutdown_graph_changes(&mut e.graphs).unwrap();
        let at = e.runtime.next_frame();
        let (mut state, mut resources) = e.runtime.into_parts();
        state
            .bank
            .shutdown(&mut e.loops, &mut resources, at)
            .unwrap();
        imported(&mut resources);
        for _ in 0..512 {
            resources.begin_callback();
            resources.reclaim();
        }
        assert_eq!(resources.usage().free_pages, 256);
    }

    #[shoop_test]
    fn production_dsp_records_and_plays_audio_and_midi_on_every_fixed_quantum() {
        for q in [16, 32, 64, 128] {
            let d = description(q, 1);
            let pool = Pool::new(&d.segments, 1);
            let mut e = prepare(&d, driver(&d), rt::Quantum::new(q).unwrap())
                .unwrap()
                .runtime(&graph(&pool))
                .unwrap();
            for (target, op) in [
                (
                    pool.strip_input(0),
                    EngineOp::Reset {
                        inputs: 2,
                        outputs: 0,
                    },
                ),
                (pool.strip_input(0), EngineOp::Monitor(true)),
                (pool.strip_input(0), EngineOp::Gain(0.5)),
                (pool.strip_input(0), EngineOp::Balance(-0.5)),
                (
                    pool.strip_output(0),
                    EngineOp::Reset {
                        inputs: 0,
                        outputs: 2,
                    },
                ),
                (pool.strip_output(0), EngineOp::Gain(0.25)),
                (pool.strip_output(0), EngineOp::Balance(0.25)),
                (
                    pool.bus(0),
                    EngineOp::Reset {
                        inputs: 0,
                        outputs: 2,
                    },
                ),
                (pool.bus(0), EngineOp::Gain(2.)),
                (pool.bus(0), EngineOp::Balance(-0.25)),
            ] {
                e.operations.submit(Frame(0), target, op).unwrap();
            }
            e.operations
                .submit(
                    Frame(2 * q as u64),
                    pool.strip_input(0),
                    EngineOp::Monitor(false),
                )
                .unwrap();
            let target = LoopId::from_parts(0, 1).unwrap();
            e.loops
                .submit(
                    Frame(0),
                    loops::Command::Record {
                        target,
                        monitor_previous: false,
                    },
                )
                .unwrap();
            e.loops
                .submit(
                    Frame(2 * q as u64),
                    loops::Command::Play {
                        target,
                        position: 0,
                    },
                )
                .unwrap();
            let frames = 4 * q;
            let input: Vec<_> = [1., -2.]
                .map(|g| (0..frames).map(|i| (i + 1) as f32 * g / 16.).collect())
                .into();
            let mut output = vec![vec![0.; frames]; 2];
            let mut mi = [MidiBuffer::new(128, 1024).unwrap()];
            let mut mo = [MidiBuffer::new(128, 1024).unwrap()];
            mi[0].push((q - 1) as u32, frames, &[0x90, 60, 90]).unwrap();
            mi[0].push((q + 3) as u32, frames, &[0x80, 60, 0]).unwrap();
            render(&mut e, &input, &mut output, &mi, &mut mo, frames);
            for f in 0..frames {
                let source = f % (2 * q);
                assert_eq!(output[0][f], input[0][source] * 0.5 * 0.25 * 0.75 * 2.);
                assert_eq!(
                    output[1][f],
                    input[1][source] * 0.5 * 0.5 * 0.25 * 2. * 0.75
                );
            }
            let packets: Vec<_> = (0..mo[0].len())
                .filter_map(|i| {
                    let (at, p) = mo[0].get(i)?;
                    (p == [0x90, 60, 90] || p == [0x80, 60, 0]).then_some((at, p.to_vec()))
                })
                .collect();
            assert_eq!(
                packets,
                vec![
                    ((q - 1) as u32, vec![0x90, 60, 90]),
                    ((q + 3) as u32, vec![0x80, 60, 0]),
                    ((3 * q - 1) as u32, vec![0x90, 60, 90]),
                    ((3 * q + 3) as u32, vec![0x80, 60, 0])
                ]
            );
            let state = e.runtime.state();
            let status = state.bank.status(target).unwrap();
            assert_eq!(status.length, 2 * q as u64);
            assert_eq!(status.position, 0);
            assert_eq!(status.cycles, 1);
            assert_eq!(
                state.cells.loops[0].length.load(Ordering::Relaxed),
                2 * q as u64
            );
            assert_eq!(state.cells.callbacks.load(Ordering::Relaxed), 1);
            let peaks = state.cells.buses[0].take(2);
            for c in 0..2 {
                assert_eq!(
                    peaks.as_slice()[c],
                    output[c].iter().fold(0f32, |p, x| p.max(x.abs()))
                );
            }
            while let Some(r) = e.operations.try_receipt() {
                assert_eq!(r.result, Ok(()));
            }
            while let Some(r) = e.loops.try_receipt() {
                assert_eq!(r.result, Ok(loops::Reply::Done));
            }
            cleanup(e, |_| {});
        }
    }

    #[shoop_test]
    fn production_configuration_activates_with_routes_and_ramps_continue_across_ticks() {
        for q in [16, 32, 64, 128] {
            let mut d = description(q, 0);
            d.ramp_frames = (q + 3) as u32;
            let pool = Pool::new(&d.segments, 1);
            let g = graph(&pool);
            let mut e = prepare(&d, driver(&d), rt::Quantum::new(q).unwrap())
                .unwrap()
                .runtime(&g)
                .unwrap();
            for (target, gain) in [(pool.strip_input(0), 0.5), (pool.strip_output(0), 1.)] {
                e.operations
                    .submit(
                        Frame(0),
                        target,
                        EngineOp::Configure(crate::NodeConfiguration {
                            token: 1,
                            midi_inputs: 0,
                            epoch: Epoch(1),
                            inputs: 2,
                            outputs: 2,
                            bank: 0,
                            gain,
                            balance: 0.,
                            monitoring: true,
                            muted: false,
                        }),
                    )
                    .unwrap();
            }
            e.graphs
                .submit(
                    Frame(q as u64),
                    Epoch(0),
                    e.graphs.compiler().compile(&g).unwrap(),
                )
                .unwrap();
            e.operations
                .submit(
                    Frame(2 * q as u64),
                    pool.strip_output(0),
                    EngineOp::Gain(0.),
                )
                .unwrap();
            let input = vec![vec![1.; 4 * q]; 2];
            let mut output = vec![vec![0.; 4 * q]; 2];
            let mi = [MidiBuffer::new(128, 1024).unwrap()];
            let mut mo = [MidiBuffer::new(128, 1024).unwrap()];
            render(&mut e, &input, &mut output, &mi, &mut mo, 4 * q);
            for c in 0..2 {
                assert_eq!(&output[c][..q], vec![0.; q]);
                assert_eq!(&output[c][q..2 * q], vec![0.5; q]);
                for i in 0..2 * q {
                    let expected = 0.5 * (1. - ((i + 1) as f32 / (q + 3) as f32).min(1.));
                    assert!((output[c][2 * q + i] - expected).abs() < 1e-6);
                }
            }
            assert_eq!(e.graphs.try_receipt().unwrap().result, Ok(Epoch(1)));
            while let Some(r) = e.operations.try_receipt() {
                assert_eq!(r.result, Ok(()));
            }
            cleanup(e, |_| {});
        }
    }

    #[shoop_test]
    fn production_bulk_reader_spends_one_transfer_allowance_for_the_entire_callback() {
        use rt_runtime::resource::{Description, Format};
        for q in [16, 32, 64, 128] {
            let d = description(32, 0);
            let pool = Pool::new(&d.segments, 1);
            let mut p = prepare(&d, driver(&d), rt::Quantum::new(q).unwrap()).unwrap();
            let bytes: Vec<_> = (0..4096).map(|i| (i % 251) as u8).collect();
            let resource = p.resources.create().unwrap();
            let u = p
                .resources
                .begin_upload(
                    resource,
                    Revision(0),
                    Description {
                        format: Format::Audio { channels: 1 },
                        frames: 1024,
                        bytes: bytes.len(),
                    },
                )
                .unwrap();
            for (i, data) in bytes.chunks(256).enumerate() {
                p.resources.begin_callback();
                p.resources.write_upload(u, i * 256, data).unwrap();
            }
            p.resources.finish_upload(u).unwrap();
            p.resources.commit(u).unwrap();
            let snapshot = p.resources.snapshot(resource, Frame(0)).unwrap();
            let cursor = p.resources.cursor(snapshot.id).unwrap();
            let mut e = p.runtime(&graph(&pool)).unwrap();
            e.operations
                .submit(
                    Frame(0),
                    pool.reader(),
                    EngineOp::ReadStart {
                        cursor,
                        bytes: bytes.len() as u64,
                    },
                )
                .unwrap();
            let input = vec![vec![0.; 128]; 2];
            let mut output = vec![vec![0.; 128]; 2];
            let mi = [MidiBuffer::new(128, 1024).unwrap()];
            let mut mo = [MidiBuffer::new(128, 1024).unwrap()];
            let count = crate::stream::READ_PAGES * d.page_bytes;
            let mut actual = vec![0; count];
            for offset in [0, count] {
                render(&mut e, &input, &mut output, &mi, &mut mo, 128);
                let ring = &e.runtime.state().cells.bulk;
                assert_eq!(ring.available(), count);
                assert_eq!(ring.take(&mut actual), count);
                assert_eq!(&actual, &bytes[offset..offset + count]);
            }
            cleanup(e, |resources| {
                resources.release_snapshot(snapshot.id).unwrap();
                resources.remove(resource).unwrap();
            });
        }
    }
}
