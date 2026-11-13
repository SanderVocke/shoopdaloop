//! Dev-only assembly of the loop library into one runtime with optional effects.
//!
//! Tests and test hosts use this to drive the loop parts the way an
//! application would. Production crates assemble their own runtime and node type.
#![forbid(unsafe_code)]

use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    executor::{self, Command, GraphRuntime, PreparedNode},
    graph::{Catalog, GraphDescription, PortCounts, PreparedPlan},
    processor::{OperationContext, ProcessBlock, Processor, ProcessorOperation},
    resource::{self, PoolUsage, ResourceStore},
    schedule,
    transport::{self, Lane},
    Epoch, Error, Frame, NodeId, PreparedConfig,
};
use shoop_loops::{
    media,
    runtime::{
        Binding, BoundaryObserver, Failure, LoopClient, LoopNode, LoopPlanner, NodePairSettings,
        Operation, Options, Prepared, PreparedMedia, Reply, Settings, TopologySettings,
    },
    PreparedTopology,
};

/// A loop node or an independently controlled processor.
pub enum TestNode<P> {
    Loop(LoopNode),
    Effect(P),
}
impl<P: Processor<LoopPlanner, Q>, Q: ProcessorOperation> Processor<LoopPlanner, Q>
    for TestNode<P>
{
    fn admit(&self, id: NodeId, domain: &LoopPlanner) -> Result<(), Error> {
        match self {
            Self::Loop(node) => node.admit(id, domain),
            Self::Effect(node) => node.admit(id, domain),
        }
    }

    fn operation_bytes(&self) -> usize {
        match self {
            Self::Loop(_) => 0,
            Self::Effect(node) => node.operation_bytes(),
        }
    }
    fn operation(
        &mut self,
        operation: Q,
        context: OperationContext<'_>,
    ) -> Result<Q::Result, Error> {
        match self {
            Self::Loop(_) => Err(Error::InvalidEvent),
            Self::Effect(node) => node.operation(operation, context),
        }
    }
    fn ports(&self) -> PortCounts {
        match self {
            Self::Loop(node) => node.ports(),
            Self::Effect(node) => node.ports(),
        }
    }
    fn set_parameter(&mut self, parameter: u32, value: f32) -> Result<(), Error> {
        match self {
            Self::Loop(_) => Err(Error::InvalidParameter),
            Self::Effect(node) => node.set_parameter(parameter, value),
        }
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        match self {
            Self::Loop(node) => node.process(block),
            Self::Effect(node) => node.process(block),
        }
    }
}

pub type TestCommand<P, Q = ()> = Command<TestNode<P>, LoopPlanner, Q>;
pub type TestSubmit<P, Q = ()> =
    Result<transport::Ticket, transport::SubmitError<TestCommand<P, Q>>>;

/// The render-side owner of one assembled runtime.
pub struct LoopRuntime<P: Processor<LoopPlanner, Q>, Q: ProcessorOperation = ()> {
    runtime: GraphRuntime<TestNode<P>, LoopPlanner, Q>,
}
/// The control side of an assembled runtime.
pub struct LoopControl<P: Processor<LoopPlanner, Q>, Q: ProcessorOperation = ()> {
    control: executor::Control<TestNode<P>, LoopPlanner, Q>,
    client: LoopClient,
    catalog: Catalog,
}
/// A runtime's planner and store after callbacks stopped.
pub struct StoppedLoops {
    planner: LoopPlanner,
    resources: ResourceStore,
}

impl<P: Processor<LoopPlanner, Q>, Q: ProcessorOperation> LoopRuntime<P, Q> {
    /// Every primitive requires playback and capture nodes in `graph`.
    /// Effect processors retain their own typed operations.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        config: PreparedConfig,
        topology: PreparedTopology,
        resources: ResourceStore,
        settings: Settings,
        bindings: Vec<Binding>,
        imports: Vec<media::Input>,
        effects: Vec<(NodeId, P)>,
        graph: GraphDescription,
    ) -> Result<(LoopControl<P, Q>, Self), Failure> {
        Self::prepare_with_options(
            config,
            topology,
            resources,
            settings,
            bindings,
            imports,
            effects,
            graph,
            Options::default(),
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_with_observer(
        config: PreparedConfig,
        topology: PreparedTopology,
        resources: ResourceStore,
        settings: Settings,
        bindings: Vec<Binding>,
        imports: Vec<media::Input>,
        effects: Vec<(NodeId, P)>,
        graph: GraphDescription,
        observer: Box<dyn BoundaryObserver>,
    ) -> Result<(LoopControl<P, Q>, Self), Failure> {
        Self::prepare_with_staging(
            config,
            topology,
            resources,
            settings,
            bindings,
            imports,
            effects,
            graph,
            TopologySettings::default(),
            Some(observer),
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_with_staging(
        config: PreparedConfig,
        topology: PreparedTopology,
        resources: ResourceStore,
        settings: Settings,
        bindings: Vec<Binding>,
        imports: Vec<media::Input>,
        effects: Vec<(NodeId, P)>,
        graph: GraphDescription,
        staging: TopologySettings,
        observer: Option<Box<dyn BoundaryObserver>>,
    ) -> Result<(LoopControl<P, Q>, Self), Failure> {
        Self::prepare_with_configuration_staging(
            config,
            topology,
            resources,
            settings,
            bindings,
            imports,
            effects,
            graph,
            staging,
            NodePairSettings::default(),
            observer,
        )
    }
    /// Unassigned node-pair templates stay outside the initial graph.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_with_configuration_staging(
        config: PreparedConfig,
        topology: PreparedTopology,
        resources: ResourceStore,
        settings: Settings,
        bindings: Vec<Binding>,
        imports: Vec<media::Input>,
        effects: Vec<(NodeId, P)>,
        graph: GraphDescription,
        staging: TopologySettings,
        node_pairs: NodePairSettings,
        observer: Option<Box<dyn BoundaryObserver>>,
    ) -> Result<(LoopControl<P, Q>, Self), Failure> {
        Self::prepare_with_options(
            config,
            topology,
            resources,
            settings,
            bindings,
            imports,
            effects,
            graph,
            Options {
                staging,
                node_pairs,
                observer,
            },
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn prepare_with_options(
        config: PreparedConfig,
        topology: PreparedTopology,
        mut resources: ResourceStore,
        settings: Settings,
        bindings: Vec<Binding>,
        imports: Vec<media::Input>,
        effects: Vec<(NodeId, P)>,
        graph: GraphDescription,
        options: Options,
    ) -> Result<(LoopControl<P, Q>, Self), Failure> {
        for binding in &bindings {
            let assigned = topology.node(binding.target).is_ok();
            if graph.nodes.contains(&binding.playback) != assigned
                || graph.nodes.contains(&binding.capture) != assigned
            {
                return Err(Error::InvalidId.into());
            }
        }
        let templates = options.node_pairs.candidates;
        let quantum = resources.quantum();
        let Prepared {
            planner,
            nodes: loop_nodes,
            client,
        } = LoopPlanner::prepare(
            config,
            quantum,
            topology,
            &mut resources,
            settings,
            bindings,
            imports,
            options,
        )?;
        let mut nodes: Vec<_> = loop_nodes
            .into_iter()
            .map(|(id, node)| PreparedNode::new(id, TestNode::Loop(node)))
            .collect();
        for (id, effect) in effects {
            if !graph.nodes.contains(&id) {
                return Err(Error::InvalidShape.into());
            }
            nodes.push(PreparedNode::new(id, TestNode::Effect(effect)));
        }
        if templates == 0 && graph.nodes.len() != nodes.len() {
            return Err(Error::InvalidShape.into());
        }
        let (mut control, runtime) = GraphRuntime::prepare_with_initial_graph(
            config, quantum, nodes, planner, resources, &graph,
        )?;
        control.prepare_submissions()?;
        let catalog = runtime.catalog().clone();
        Ok((
            LoopControl {
                control,
                client,
                catalog,
            },
            Self { runtime },
        ))
    }
    /// Allocating preparation before the first nonempty callback.
    pub fn prepare_graph_staging(&mut self, candidates: usize) -> Result<(), Error> {
        self.runtime.prepare_graph_staging(candidates)
    }
    /// Zero frames are inert. Never replay a callback after an error.
    pub fn process(&mut self, audio: AudioBlock<'_, '_>, midi: MidiBlock<'_>) -> Result<(), Error> {
        self.runtime.process(audio, midi)
    }
    pub fn next_frame(&self) -> Frame {
        self.runtime.next_frame()
    }
    pub fn epoch(&self) -> Epoch {
        self.runtime.epoch()
    }
    pub fn resource_usage(&self) -> PoolUsage {
        self.runtime.resource_usage()
    }
    pub fn diagnostics(&self) -> executor::Diagnostics {
        self.runtime.diagnostics()
    }
    pub fn drain_midi_recovery(
        &mut self,
        port: usize,
        sink: &mut MidiBuffer,
        frames: usize,
    ) -> Result<usize, Error> {
        self.runtime.drain_midi_recovery(port, sink, frames)
    }
    pub fn midi_recovery_pending(&self, port: usize) -> Result<bool, Error> {
        self.runtime.midi_recovery_pending(port)
    }
    /// Consume off-render after callbacks stop, keeping the planner's ledger with its store.
    pub fn into_stopped(self) -> StoppedLoops {
        let (planner, resources) = self.runtime.into_planner_and_resources();
        StoppedLoops { planner, resources }
    }
}

impl<P: Processor<LoopPlanner, Q>, Q: ProcessorOperation> LoopControl<P, Q> {
    pub fn client(&self) -> &LoopClient {
        &self.client
    }
    pub fn media(&self) -> &[PreparedMedia] {
        self.client.media()
    }
    /// Control-side copy of the runtime catalog, for compiling generic graph edits.
    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }
    pub fn compile_graph(&self, graph: &GraphDescription) -> Result<PreparedPlan, Error> {
        self.client.compile_graph(&self.catalog, graph)
    }
    pub fn stage_graph(
        &mut self,
        slot: usize,
        expected_epoch: Epoch,
        plan: PreparedPlan,
    ) -> TestSubmit<P, Q> {
        self.control.submit_prepared(
            Lane::Normal,
            Command::stage_graph(slot, expected_epoch, plan),
        )
    }
    pub fn release_graph(&mut self, id: shoop_loops::runtime::GraphStageId) -> TestSubmit<P, Q> {
        self.control
            .submit_prepared(Lane::Normal, Command::ReleaseGraph { id })
    }
    pub fn submit(&mut self, lane: Lane, at: Frame, operation: Operation) -> TestSubmit<P, Q> {
        self.control
            .submit_prepared(lane, self.client.command(at, operation))
    }
    pub fn submit_payload(
        &mut self,
        lane: Lane,
        at: Frame,
        operation: Operation,
        payload: Box<[u8]>,
    ) -> TestSubmit<P, Q> {
        self.control.submit_prepared(
            lane,
            self.client.command_with_payload(at, operation, payload),
        )
    }
    /// Submit a generic runtime command, bypassing the loop vocabulary.
    pub fn submit_command(&mut self, lane: Lane, command: TestCommand<P, Q>) -> TestSubmit<P, Q> {
        self.control.submit_prepared(lane, command)
    }
    pub fn resource(&mut self, lane: Lane, request: resource::Request) -> TestSubmit<P, Q> {
        self.control
            .submit_prepared(lane, Command::resource(request))
    }
    pub fn read(&mut self, cursor: resource::Cursor, bytes: Box<[u8]>) -> TestSubmit<P, Q> {
        self.control.submit_prepared(
            Lane::Normal,
            Command::resource(resource::Request::Read { cursor, bytes }),
        )
    }
    pub fn parameter(
        &mut self,
        at: Frame,
        node: NodeId,
        parameter: u32,
        value: f32,
    ) -> TestSubmit<P, Q> {
        self.control.submit_prepared(
            Lane::Normal,
            Command::Schedule {
                at,
                action: schedule::Action::Parameter {
                    node,
                    parameter,
                    value,
                },
                payload: Box::new([]),
            },
        )
    }
    pub fn try_completion(
        &mut self,
    ) -> Option<transport::Completion<TestCommand<P, Q>, executor::Retired<TestNode<P>>>> {
        self.control.try_completion()
    }
    pub fn try_execution(&mut self) -> Option<schedule::Execution<Operation, Reply, Q, Q::Result>> {
        self.control.try_execution()
    }
    pub fn outstanding(&self) -> usize {
        self.control.outstanding()
    }
    pub fn connected(&self) -> bool {
        self.control.connected()
    }
}

impl StoppedLoops {
    pub fn observer(&self) -> &dyn BoundaryObserver {
        self.planner.observer()
    }
    pub fn resource_usage(&self) -> PoolUsage {
        self.resources.usage()
    }
    pub fn media_usage(&self) -> media::Usage {
        self.planner.media_usage()
    }
    pub fn resources(&mut self) -> &mut ResourceStore {
        &mut self.resources
    }
    /// Permanently quiesce and release domain owners; repeatable.
    pub fn cleanup(&mut self) -> Result<(), Error> {
        self.planner.cleanup(&mut self.resources)
    }
}
