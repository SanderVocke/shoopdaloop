//! Control-side DAG compilation and incremental render-side descriptor validation.
//!
//! Descriptors contain logical typed endpoints, never pointers or caller-chosen
//! scratch offsets. Prepared plans own immutable descriptors; activation/retirement
//! transfers that ownership without rebuilding or freeing on the render thread.

pub mod activation;

use crate::planner::{
    ConsumerAccess, ConsumerSpec, DecisionBinding, PlannerSpec, PlanningDescriptor, PlanningStep,
};
use crate::{Error, NodeId, PreparedConfig};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PortType {
    Audio,
    Midi,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PortCounts {
    pub audio_inputs: usize,
    pub audio_outputs: usize,
    pub midi_inputs: usize,
    pub midi_outputs: usize,
}

impl PortCounts {
    pub fn total(self) -> Result<usize, Error> {
        self.audio_inputs
            .checked_add(self.audio_outputs)
            .and_then(|n| n.checked_add(self.midi_inputs))
            .and_then(|n| n.checked_add(self.midi_outputs))
            .ok_or(Error::Capacity)
    }

    pub fn inputs(self, kind: PortType) -> usize {
        match kind {
            PortType::Audio => self.audio_inputs,
            PortType::Midi => self.midi_inputs,
        }
    }

    pub fn outputs(self, kind: PortType) -> usize {
        match kind {
            PortType::Audio => self.audio_outputs,
            PortType::Midi => self.midi_outputs,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NodeSpec {
    pub id: NodeId,
    pub ports: PortCounts,
}

/// `None` identifies the host: host inputs are sources, host outputs destinations.
/// Node endpoints instead select outputs as sources and inputs as destinations.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Endpoint {
    pub node: Option<NodeId>,
    pub kind: PortType,
    pub port: u32,
}

impl Endpoint {
    pub fn host(kind: PortType, port: u32) -> Self {
        Self {
            node: None,
            kind,
            port,
        }
    }
    pub fn node(node: NodeId, kind: PortType, port: u32) -> Self {
        Self {
            node: Some(node),
            kind,
            port,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Connection {
    pub source: Endpoint,
    pub destination: Endpoint,
}

/// Stable processor signatures; constructed outside rendering. A plan need not use
/// every registered processor. The runtime owns the corresponding processor state.
pub struct Catalog {
    config: PreparedConfig,
    slots: Vec<Option<NodeSpec>>,
    planner: Option<PlannerSpec>,
    consumers: Option<Vec<Option<ConsumerAccess>>>,
}

impl Catalog {
    pub fn new(config: PreparedConfig, nodes: &[NodeSpec]) -> Result<Self, Error> {
        let limits = config.limits();
        if nodes.len() > limits.max_nodes {
            return Err(Error::Capacity);
        }
        let mut slots = vec![None; limits.max_nodes];
        let mut total = limits
            .audio_inputs
            .checked_add(limits.audio_outputs)
            .and_then(|n| n.checked_add(limits.midi_inputs))
            .and_then(|n| n.checked_add(limits.midi_outputs))
            .ok_or(Error::Capacity)?;
        for &node in nodes {
            let slot = slots
                .get_mut(node.id.slot() as usize)
                .ok_or(Error::InvalidId)?;
            if slot.is_some() {
                return Err(Error::InvalidId);
            }
            total = total
                .checked_add(node.ports.total()?)
                .ok_or(Error::Capacity)?;
            if total > limits.max_ports {
                return Err(Error::Capacity);
            }
            *slot = Some(node);
        }
        if total > limits.max_ports {
            return Err(Error::Capacity);
        }
        Ok(Self {
            config,
            slots,
            planner: None,
            consumers: None,
        })
    }

    pub fn with_planner(
        config: PreparedConfig,
        nodes: &[NodeSpec],
        planner: PlannerSpec,
        consumers: &[ConsumerSpec],
    ) -> Result<Self, Error> {
        planner.validate()?;
        if consumers.len() > config.limits().max_nodes
            || config
                .limits()
                .max_nodes
                .checked_mul(size_of::<Option<ConsumerAccess>>())
                .is_none_or(|bytes| bytes > i32::MAX as usize)
        {
            return Err(Error::Capacity);
        }
        let mut catalog = Self::new(config, nodes)?;
        let mut access = vec![None; config.limits().max_nodes];
        for consumer in consumers {
            catalog.node(consumer.node)?;
            if access[consumer.node.slot() as usize].is_some() {
                return Err(Error::InvalidGraph);
            }
            if consumer.access.observations > planner.observation_capacity
                || consumer.access.operation_bytes > i32::MAX as usize
            {
                return Err(Error::Capacity);
            }
            if consumer.access.decision_required && planner.decision_slots == 0 {
                return Err(Error::InvalidGraph);
            }
            access[consumer.node.slot() as usize] = Some(consumer.access);
        }
        catalog.planner = Some(planner);
        catalog.consumers = Some(access);
        Ok(catalog)
    }

    pub fn planner(&self) -> Option<PlannerSpec> {
        self.planner
    }

    pub fn consumer(&self, node: NodeId) -> Result<ConsumerAccess, Error> {
        self.node(node)?;
        Ok(self
            .consumers
            .as_ref()
            .and_then(|access| access[node.slot() as usize])
            .unwrap_or_default())
    }

    pub fn config(&self) -> PreparedConfig {
        self.config
    }
    pub fn node(&self, id: NodeId) -> Result<NodeSpec, Error> {
        self.slots
            .get(id.slot() as usize)
            .and_then(|slot| *slot)
            .filter(|spec| spec.id == id)
            .ok_or(Error::InvalidId)
    }

    pub(crate) fn remove(&mut self, id: NodeId) -> Result<(), Error> {
        self.node(id)?;
        self.slots[id.slot() as usize] = None;
        Ok(())
    }

    pub(crate) fn install(&mut self, spec: NodeSpec) -> Result<(), Error> {
        let slot = self
            .slots
            .get_mut(spec.id.slot() as usize)
            .ok_or(Error::InvalidId)?;
        if slot.is_some() {
            return Err(Error::InvalidState);
        }
        *slot = Some(spec);
        Ok(())
    }

    fn endpoint(&self, endpoint: Endpoint, source: bool) -> Result<(), Error> {
        let count = if let Some(id) = endpoint.node {
            let ports = self.node(id)?.ports;
            if source {
                ports.outputs(endpoint.kind)
            } else {
                ports.inputs(endpoint.kind)
            }
        } else {
            let config = self.config.limits();
            match (endpoint.kind, source) {
                (PortType::Audio, true) => config.audio_inputs,
                (PortType::Audio, false) => config.audio_outputs,
                (PortType::Midi, true) => config.midi_inputs,
                (PortType::Midi, false) => config.midi_outputs,
            }
        };
        if endpoint.port as usize >= count {
            return Err(Error::InvalidGraph);
        }
        Ok(())
    }

    fn connection(&self, connection: Connection) -> Result<(), Error> {
        if connection.source.kind != connection.destination.kind {
            return Err(Error::PortType);
        }
        self.endpoint(connection.source, true)?;
        self.endpoint(connection.destination, false)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GraphDescription {
    pub nodes: Vec<NodeId>,
    pub connections: Vec<Connection>,
}

/// Incoming connections for each node occupy the range between the preceding
/// step's `input_end` and this step's `input_end`. Remaining connections route to
/// the host. The receiver validates all ranges and dependency ordering.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NodeStep {
    pub node: NodeId,
    pub input_end: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PlanDescriptor {
    pub steps: Vec<NodeStep>,
    pub connections: Vec<Connection>,
}

/// Prepared ownership, not yet permission to execute. Even compiler-generated
/// plans must pass receiver validation against the live catalog before activation.
pub struct PreparedPlan {
    descriptor: PlanDescriptor,
    planning: PlanningDescriptor,
    membership: Vec<Membership>,
    membership_stamp: u64,
    config: PreparedConfig,
}
#[derive(Clone, Copy, Default)]
struct Membership {
    stamp: u64,
    node: Option<NodeId>,
}

impl PreparedPlan {
    /// Retained metadata bytes, including spare vector capacity but not allocator overhead.
    pub fn storage_bytes(&self) -> Result<usize, Error> {
        let mut bytes = size_of::<Self>();
        for (count, size) in [
            (self.descriptor.steps.capacity(), size_of::<NodeStep>()),
            (
                self.descriptor.connections.capacity(),
                size_of::<Connection>(),
            ),
            (self.membership.capacity(), size_of::<Membership>()),
            (
                self.planning.bindings.capacity(),
                size_of::<DecisionBinding>(),
            ),
        ] {
            bytes = count
                .checked_mul(size)
                .and_then(|n| bytes.checked_add(n))
                .ok_or(Error::Capacity)?;
        }
        if bytes > i32::MAX as usize {
            return Err(Error::Capacity);
        }
        Ok(bytes)
    }

    /// Control-side envelope for an untrusted descriptor. Only storage bounds are
    /// checked here. This deliberately does not certify graph validity.
    pub fn from_descriptor(
        config: PreparedConfig,
        descriptor: PlanDescriptor,
    ) -> Result<Self, Error> {
        Self::from_planned_descriptor(config, descriptor, PlanningDescriptor::default())
    }

    pub fn from_planned_descriptor(
        config: PreparedConfig,
        descriptor: PlanDescriptor,
        planning: PlanningDescriptor,
    ) -> Result<Self, Error> {
        let limits = config.limits();
        if descriptor.steps.len() > limits.max_nodes
            || descriptor.connections.len() > limits.max_edges
            || planning.bindings.len() > limits.max_nodes
        {
            return Err(Error::Capacity);
        }
        let mut membership = vec![Membership::default(); limits.max_nodes];
        for slot in &mut membership {
            *std::hint::black_box(slot) = Membership::default();
        }
        for step in &descriptor.steps {
            if let Some(slot) = membership.get_mut(step.node.slot() as usize) {
                *slot = Membership {
                    stamp: 1,
                    node: Some(step.node),
                };
            }
        }
        Ok(Self {
            descriptor,
            planning,
            membership,
            membership_stamp: 1,
            config,
        })
    }

    /// Prepare full-capacity, pre-touched descriptor storage for bounded receiver
    /// materialization. Descriptor edits do not certify execution validity.
    pub fn with_capacity(config: PreparedConfig) -> Result<Self, Error> {
        let limits = config.limits();
        for (count, size) in [
            (limits.max_nodes, size_of::<NodeStep>()),
            (limits.max_edges, size_of::<Connection>()),
            (limits.max_nodes, size_of::<Membership>()),
            (limits.max_nodes, size_of::<DecisionBinding>()),
        ] {
            if count
                .checked_mul(size)
                .is_none_or(|bytes| bytes > i32::MAX as usize)
            {
                return Err(Error::Capacity);
            }
        }
        let step = NodeStep {
            node: NodeId::from_parts(0, 1)?,
            input_end: 0,
        };
        let endpoint = Endpoint {
            node: None,
            kind: PortType::Audio,
            port: 0,
        };
        let edge = Connection {
            source: endpoint,
            destination: endpoint,
        };
        let mut descriptor = PlanDescriptor {
            steps: vec![step; limits.max_nodes],
            connections: vec![edge; limits.max_edges],
        };
        for value in &mut descriptor.steps {
            *std::hint::black_box(value) = step;
        }
        for value in &mut descriptor.connections {
            *std::hint::black_box(value) = edge;
        }
        descriptor.steps.clear();
        descriptor.connections.clear();
        let binding = DecisionBinding {
            planner: crate::PlannerId::from_parts(0, 1)?,
            consumer: step.node,
            decision_slot: 0,
        };
        let mut bindings = vec![binding; limits.max_nodes];
        for value in &mut bindings {
            *std::hint::black_box(value) = binding;
        }
        bindings.clear();
        Self::from_planned_descriptor(
            config,
            descriptor,
            PlanningDescriptor {
                step: None,
                bindings,
            },
        )
    }
    /// Constant-work reset. Stamped membership avoids clearing every node slot in
    /// a callback. Only invoke while exclusively owning an inactive plan.
    pub fn begin_descriptor(&mut self) -> Result<(), Error> {
        let stamp = self
            .membership_stamp
            .checked_add(1)
            .ok_or(Error::Exhausted)?;
        self.descriptor.steps.clear();
        self.descriptor.connections.clear();
        self.planning.step = None;
        self.planning.bindings.clear();
        self.membership_stamp = stamp;
        Ok(())
    }
    pub fn push_step(&mut self, step: NodeStep) -> Result<(), Error> {
        if self.descriptor.steps.len() == self.descriptor.steps.capacity()
            || self.descriptor.steps.len() == self.config.limits().max_nodes
        {
            return Err(Error::Capacity);
        }
        if let Some(slot) = self.membership.get_mut(step.node.slot() as usize) {
            *slot = Membership {
                stamp: self.membership_stamp,
                node: Some(step.node),
            };
        }
        self.descriptor.steps.push(step);
        Ok(())
    }
    pub fn push_connection(&mut self, connection: Connection) -> Result<(), Error> {
        if self.descriptor.connections.len() == self.descriptor.connections.capacity()
            || self.descriptor.connections.len() == self.config.limits().max_edges
        {
            return Err(Error::Capacity);
        }
        self.descriptor.connections.push(connection);
        Ok(())
    }
    pub fn set_planning_step(&mut self, step: Option<PlanningStep>) {
        self.planning.step = step;
    }

    pub fn push_binding(&mut self, binding: DecisionBinding) -> Result<(), Error> {
        if self.planning.bindings.len() == self.planning.bindings.capacity()
            || self.planning.bindings.len() == self.config.limits().max_nodes
        {
            return Err(Error::Capacity);
        }
        self.planning.bindings.push(binding);
        Ok(())
    }

    pub fn planning(&self) -> &PlanningDescriptor {
        &self.planning
    }

    pub fn descriptor(&self) -> &PlanDescriptor {
        &self.descriptor
    }
    pub fn config(&self) -> PreparedConfig {
        self.config
    }
    pub fn contains(&self, id: NodeId) -> bool {
        self.membership
            .get(id.slot() as usize)
            .is_some_and(|slot| slot.stamp == self.membership_stamp && slot.node == Some(id))
    }
}

/// Allocating control-side compiler. Equivalent descriptions compile identically
/// regardless of declaration order; ties break by stable node ID and source port.
pub fn compile(catalog: &Catalog, graph: &GraphDescription) -> Result<PreparedPlan, Error> {
    if catalog.planner.is_none() {
        compile_signal(catalog, graph)
    } else {
        compile_planned(catalog, graph, &[])
    }
}

/// Compile explicit planning dependencies separately from signal connections.
pub fn compile_planned(
    catalog: &Catalog,
    graph: &GraphDescription,
    bindings: &[DecisionBinding],
) -> Result<PreparedPlan, Error> {
    if bindings.len() > catalog.config.limits().max_nodes {
        return Err(Error::Capacity);
    }
    let mut plan = compile_signal(catalog, graph)?;
    plan.planning.step = catalog.planner.map(|planner| PlanningStep {
        planner,
        before_step: 0,
    });
    plan.planning.bindings = bindings.to_vec();
    plan.planning
        .bindings
        .sort_unstable_by_key(|binding| binding.consumer);
    let mut validator = PlanValidator::new(catalog.config);
    validator.begin();
    match validator.advance(&plan, catalog, usize::MAX).status {
        ValidationStatus::Valid => Ok(plan),
        ValidationStatus::Invalid(error) => Err(error),
        ValidationStatus::Pending => Err(Error::InvalidState),
    }
}

fn compile_signal(catalog: &Catalog, graph: &GraphDescription) -> Result<PreparedPlan, Error> {
    let limits = catalog.config.limits();
    if graph.nodes.len() > limits.max_nodes || graph.connections.len() > limits.max_edges {
        return Err(Error::Capacity);
    }
    let mut nodes = BTreeSet::new();
    for &id in &graph.nodes {
        catalog.node(id)?;
        if !nodes.insert(id) {
            return Err(Error::InvalidGraph);
        }
    }
    let mut connections = BTreeSet::new();
    for &connection in &graph.connections {
        catalog.connection(connection)?;
        for id in [connection.source.node, connection.destination.node]
            .into_iter()
            .flatten()
        {
            if !nodes.contains(&id) {
                return Err(Error::InvalidId);
            }
        }
        if !connections.insert(connection) {
            return Err(Error::InvalidGraph);
        }
    }
    let mut ordered = Vec::with_capacity(nodes.len());
    let mut seen = BTreeSet::new();
    while seen.len() != nodes.len() {
        let candidate = nodes
            .iter()
            .copied()
            .find(|id| {
                !seen.contains(id)
                    && connections.iter().all(|edge| {
                        edge.destination.node != Some(*id)
                            || edge.source.node.is_none_or(|source| seen.contains(&source))
                    })
            })
            .ok_or(Error::Cycle)?;
        ordered.push(candidate);
        seen.insert(candidate);
    }
    let mut descriptor = PlanDescriptor {
        steps: Vec::with_capacity(ordered.len()),
        connections: Vec::with_capacity(connections.len()),
    };
    for id in ordered {
        descriptor.connections.extend(
            connections
                .iter()
                .copied()
                .filter(|edge| edge.destination.node == Some(id)),
        );
        descriptor.steps.push(NodeStep {
            node: id,
            input_end: descriptor.connections.len(),
        });
    }
    descriptor.connections.extend(
        connections
            .iter()
            .copied()
            .filter(|edge| edge.destination.node.is_none()),
    );
    PreparedPlan::from_descriptor(catalog.config, descriptor)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValidationStatus {
    Pending,
    Valid,
    Invalid(Error),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidationProgress {
    pub status: ValidationStatus,
    pub steps: usize,
}

#[derive(Clone, Copy, Debug)]
enum Phase {
    Idle,
    Header,
    Clear(usize),
    Node(usize),
    Inputs { node_index: usize, end: usize },
    Host,
    Bindings(usize),
    Required(usize),
    Done,
    Failed(Error),
}

/// Preallocated incremental validator. `begin` must be called for each new plan;
/// the caller must keep the plan and catalog unchanged until validation finishes.
/// A validation step has constant work independent of graph size; the caller
/// supplies a bounded step budget each callback.
pub struct PlanValidator {
    config: PreparedConfig,
    seen: Vec<Option<NodeId>>,
    phase: Phase,
    edge: usize,
    previous: Option<Connection>,
    previous_consumer: Option<NodeId>,
    observations: usize,
}

impl PlanValidator {
    pub fn new(config: PreparedConfig) -> Self {
        Self {
            config,
            seen: vec![None; config.limits().max_nodes],
            phase: Phase::Idle,
            edge: 0,
            previous: None,
            previous_consumer: None,
            observations: 0,
        }
    }

    pub fn begin(&mut self) {
        self.phase = Phase::Header;
        self.edge = 0;
        self.previous = None;
        self.previous_consumer = None;
        self.observations = 0;
    }

    pub fn advance(
        &mut self,
        plan: &PreparedPlan,
        catalog: &Catalog,
        budget: usize,
    ) -> ValidationProgress {
        let mut steps = 0;
        while steps < budget {
            match self.phase {
                Phase::Done | Phase::Failed(_) | Phase::Idle => break,
                _ => {}
            }
            if let Err(error) = self.step(plan, catalog) {
                self.phase = Phase::Failed(error);
            }
            steps += 1;
        }
        let status = match self.phase {
            Phase::Done => ValidationStatus::Valid,
            Phase::Failed(error) => ValidationStatus::Invalid(error),
            Phase::Idle => ValidationStatus::Invalid(Error::InvalidState),
            _ => ValidationStatus::Pending,
        };
        ValidationProgress { status, steps }
    }

    fn step(&mut self, plan: &PreparedPlan, catalog: &Catalog) -> Result<(), Error> {
        let descriptor = &plan.descriptor;
        match self.phase {
            Phase::Header => {
                if plan.config != self.config || catalog.config != self.config {
                    return Err(Error::InvalidConfig);
                }
                match (catalog.planner, plan.planning.step) {
                    (None, None) if plan.planning.bindings.is_empty() => {}
                    (Some(expected), Some(step)) => {
                        if step.planner.id != expected.id {
                            return Err(Error::InvalidId);
                        }
                        if step.planner != expected || step.before_step != 0 {
                            return Err(Error::InvalidGraph);
                        }
                    }
                    _ => return Err(Error::InvalidGraph),
                }
                self.phase = Phase::Clear(0);
            }
            Phase::Clear(index) => {
                if let Some(slot) = self.seen.get_mut(index) {
                    *slot = None;
                    self.phase = Phase::Clear(index + 1);
                } else {
                    self.phase = Phase::Node(0);
                }
            }
            Phase::Node(index) => {
                self.previous = None;
                if let Some(step) = descriptor.steps.get(index) {
                    catalog.node(step.node)?;
                    let access = catalog.consumer(step.node)?;
                    self.observations = self
                        .observations
                        .checked_add(access.observations)
                        .ok_or(Error::Capacity)?;
                    if self.observations
                        > catalog
                            .planner
                            .map_or(0, |planner| planner.observation_capacity)
                    {
                        return Err(Error::Capacity);
                    }
                    let seen = self
                        .seen
                        .get(step.node.slot() as usize)
                        .ok_or(Error::InvalidId)?;
                    if seen.is_some()
                        || step.input_end < self.edge
                        || step.input_end > descriptor.connections.len()
                    {
                        return Err(Error::InvalidGraph);
                    }
                    self.phase = Phase::Inputs {
                        node_index: index,
                        end: step.input_end,
                    };
                } else {
                    self.phase = Phase::Host;
                }
            }
            Phase::Inputs { node_index, end } => {
                let node = descriptor
                    .steps
                    .get(node_index)
                    .ok_or(Error::InvalidState)?
                    .node;
                if self.edge == end {
                    *self
                        .seen
                        .get_mut(node.slot() as usize)
                        .ok_or(Error::InvalidId)? = Some(node);
                    self.phase = Phase::Node(node_index + 1);
                } else {
                    let edge = *descriptor
                        .connections
                        .get(self.edge)
                        .ok_or(Error::InvalidState)?;
                    self.validate_edge(edge, Some(node), catalog)?;
                }
            }
            Phase::Host => {
                if self.edge == descriptor.connections.len() {
                    self.phase = if catalog.planner.is_some() {
                        Phase::Bindings(0)
                    } else {
                        Phase::Done
                    };
                } else {
                    let edge = *descriptor
                        .connections
                        .get(self.edge)
                        .ok_or(Error::InvalidState)?;
                    self.validate_edge(edge, None, catalog)?;
                }
            }
            Phase::Bindings(index) => {
                if let Some(binding) = plan.planning.bindings.get(index) {
                    let planner = catalog.planner.ok_or(Error::InvalidGraph)?;
                    if binding.planner != planner.id {
                        return Err(Error::InvalidId);
                    }
                    catalog.node(binding.consumer)?;
                    if !plan.contains(binding.consumer) {
                        return Err(Error::InvalidId);
                    }
                    if binding.decision_slot >= planner.decision_slots
                        || self
                            .previous_consumer
                            .is_some_and(|previous| previous >= binding.consumer)
                    {
                        return Err(Error::InvalidGraph);
                    }
                    self.seen[binding.consumer.slot() as usize] = None;
                    self.previous_consumer = Some(binding.consumer);
                    self.phase = Phase::Bindings(index + 1);
                } else {
                    self.phase = Phase::Required(0);
                }
            }
            Phase::Required(index) => {
                if let Some(step) = descriptor.steps.get(index) {
                    if catalog.consumer(step.node)?.decision_required
                        && self.seen[step.node.slot() as usize].is_some()
                    {
                        return Err(Error::InvalidGraph);
                    }
                    self.phase = Phase::Required(index + 1);
                } else {
                    self.phase = Phase::Done;
                }
            }
            Phase::Idle | Phase::Done | Phase::Failed(_) => return Err(Error::InvalidState),
        }
        Ok(())
    }

    fn validate_edge(
        &mut self,
        edge: Connection,
        target: Option<NodeId>,
        catalog: &Catalog,
    ) -> Result<(), Error> {
        if edge.destination.node != target || self.previous.is_some_and(|previous| previous >= edge)
        {
            return Err(Error::InvalidGraph);
        }
        catalog.connection(edge)?;
        if let Some(source) = edge.source.node {
            if self.seen.get(source.slot() as usize).copied().flatten() != Some(source) {
                return Err(Error::InvalidGraph);
            }
        }
        self.previous = Some(edge);
        self.edge += 1;
        Ok(())
    }
}

#[cfg(test)]
mod staging_tests {
    use super::*;
    #[shoop_wasm_test_support::shoop_test]
    fn exhausted_membership_stamp_preserves_the_prior_descriptor() {
        let config = crate::Config::default().prepare().unwrap();
        let mut plan = PreparedPlan::with_capacity(config).unwrap();
        plan.membership_stamp = u64::MAX;
        let node = NodeId::from_parts(0, 1).unwrap();
        plan.push_step(NodeStep { node, input_end: 0 }).unwrap();
        assert_eq!(plan.begin_descriptor(), Err(Error::Exhausted));
        assert_eq!(plan.descriptor().steps.len(), 1);
        assert!(plan.contains(node));
    }
}
