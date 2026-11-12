//! Common production preparation and queue producer for the primitive loop profile.
use crate::SimResult;
use rt_audio_math::Q824;
use rt_fpga_adapter::looping::{Controller, Operation, Prepared, Processor, Request, Startup};
use rt_runtime::{
    Budgets, Config, Epoch, Frame, NodeId, PlannerId, PreparedConfig, Quantum,
    buffer::{AudioBlock, MidiBlock},
    executor::GraphRuntime,
    graph::{self, Catalog, Connection, Endpoint, GraphDescription, PortType},
    resource::ResourceStore,
    transport::Lane,
};
use shoop_loops::{
    self as loops,
    audio::Smoothing,
    runtime::{self as domain, LoopClient, LoopPlanner},
};
use shoop_rt_engine::dsp::GainProcessor;
pub type Runtime = GraphRuntime<Processor, LoopPlanner<Q824>, Operation, Q824>;
pub fn config() -> PreparedConfig {
    Config {
        max_frames: 32,
        audio_inputs: 1,
        audio_outputs: 1,
        midi_inputs: 0,
        midi_outputs: 0,
        max_nodes: 4,
        max_ports: 16,
        max_edges: 16,
        command_slots: 32,
        scheduled_events: 16,
        page_bytes: 128,
        resource_pages: 1024,
        resource_slots: 64,
        budgets: Budgets {
            commands: 32,
            events: 32,
            publications: 32,
            validation_steps: 4096,
            ..Default::default()
        },
        ..Default::default()
    }
    .prepare()
    .unwrap()
}
pub fn loop_id() -> loops::LoopId {
    loops::LoopId::from_parts(0, 1).unwrap()
}
pub fn node_id(slot: u32) -> NodeId {
    NodeId::from_parts(slot, 1).unwrap()
}
pub fn topology() -> SimResult<loops::PreparedTopology> {
    Ok(loops::compile(
        loops::Limits {
            loop_slots: 1,
            pending_requests: 8,
            journal_events: 10,
            relations: 1,
            program_entries: 1,
            program_segments: 1,
        },
        &loops::TopologyDescription {
            loops: vec![loops::LoopSpec {
                id: loop_id(),
                kind: loops::LoopKind::Primitive,
                length: 0,
                sync_source: None,
            }],
            ..Default::default()
        },
    )
    .map_err(|e| format!("{e:?}"))?)
}
pub fn graph() -> GraphDescription {
    let host = |p| Endpoint::host(PortType::Audio, p);
    let node = |s| Endpoint::node(node_id(s), PortType::Audio, 0);
    GraphDescription {
        nodes: vec![node_id(0), node_id(1), node_id(2)],
        connections: vec![
            Connection {
                source: host(0),
                destination: node(1),
            },
            Connection {
                source: node(0),
                destination: node(2),
            },
            Connection {
                source: node(2),
                destination: host(0),
            },
        ],
    }
}
pub struct Preparation {
    pub startup: Startup,
    pub nodes: Vec<Prepared>,
    pub planner: LoopPlanner<Q824>,
    pub client: LoopClient,
    pub resources: ResourceStore<Q824>,
}
pub fn prepare() -> SimResult<Preparation> {
    prepare_with(config())
}
pub fn prepare_with(config: PreparedConfig) -> SimResult<Preparation> {
    let quantum = Quantum::new(32)?;
    let mut resources = ResourceStore::<Q824>::with_capture_groups(
        config,
        quantum,
        None,
        rt_runtime::resource::CaptureGroupConfig {
            groups: 2,
            members_per_group: 1,
            staging_bytes_per_group: 128,
        },
    )?;
    let binding = domain::Binding {
        target: loop_id(),
        playback: node_id(0),
        capture: node_id(1),
        channels: 1,
        auxiliary_channels: 0,
        dry_wet: false,
        midi: false,
        gain: Q824(1 << 24),
        smoothing: Smoothing::default(),
        midi_events: 1,
        midi_bytes: 4,
        packet_bytes: 4,
    };
    let domain::Prepared {
        planner,
        nodes,
        client,
    } = LoopPlanner::prepare(
        config,
        quantum,
        topology()?,
        &mut resources,
        domain::Settings {
            planner: PlannerId::from_parts(0, 1)?,
            normal_requests: 4,
            media_slots: 8,
            export_slots: 4,
        },
        vec![binding],
        vec![],
        domain::Options::default(),
    )
    .map_err(|e| format!("{e:?}"))?;
    let mut nodes = nodes
        .into_iter()
        .map(|(id, node)| Prepared::new(id, Processor::Loop(node)))
        .collect::<Vec<_>>();
    nodes.push(Prepared::new(
        node_id(2),
        Processor::Gain(GainProcessor::new(1, 32, Q824(1 << 24))),
    ));
    Ok(Preparation {
        startup: Startup {
            planner: client.planner(),
            topology: topology()?,
            binding,
            max_chunks: 128,
            exports: 4,
        },
        nodes,
        planner,
        client,
        resources,
    })
}
pub fn commit(control: &mut Controller, nodes: &[Prepared]) -> SimResult<()> {
    let catalog = Catalog::new(
        config(),
        &nodes.iter().map(Prepared::spec).collect::<Vec<_>>(),
    )?;
    let plan = graph::compile(&catalog, &graph())?;
    control
        .submit(Lane::Normal, Request::commit(Epoch(0), plan))
        .map_err(|e| e.error)?;
    Ok(())
}
pub struct Native {
    pub control: Controller,
    pub runtime: Runtime,
    pub client: LoopClient,
}
impl Native {
    pub fn new() -> SimResult<Self> {
        Self::with_config(config())
    }
    pub fn with_config(config: PreparedConfig) -> SimResult<Self> {
        let Preparation {
            nodes,
            planner,
            client,
            resources,
            ..
        } = prepare_with(config)?;
        let plan = graph::compile(
            &Catalog::new(
                config,
                &nodes.iter().map(Prepared::spec).collect::<Vec<_>>(),
            )?,
            &graph(),
        )?;
        let (mut control, runtime) = Runtime::prepare_with_planner_and_resources(
            config,
            Quantum::new(32)?,
            nodes,
            planner,
            resources,
        )?;
        control
            .submit(Lane::Normal, Request::commit(Epoch(0), plan))
            .map_err(|e| e.error)?;
        Ok(Self {
            control,
            runtime,
            client,
        })
    }
    pub fn send(
        &mut self,
        operation: domain::Operation<Q824>,
        at: u64,
    ) -> SimResult<rt_runtime::Sequence> {
        Ok(self
            .control
            .submit(Lane::Normal, self.client.command(Frame(at), operation))
            .map_err(|e| e.error)?
            .sequence())
    }
    pub fn step(&mut self, input: &[i32]) -> SimResult<Vec<i32>> {
        let inputs = input.iter().copied().map(Q824).collect::<Vec<_>>();
        let mut outputs = vec![Q824(0); 32];
        self.runtime.process(
            AudioBlock {
                frame: self.runtime.next_frame(),
                frames: 32,
                inputs: &[&inputs],
                outputs: &mut [&mut outputs],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
        )?;
        Ok(outputs.into_iter().map(|v| v.0).collect())
    }
}
