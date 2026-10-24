//! Control-side DAG compilation and incremental render-side descriptor validation.
//!
//! Descriptors contain logical typed endpoints, never pointers or caller-chosen
//! scratch offsets. Prepared plans own immutable descriptors; activation/retirement
//! transfers that ownership without rebuilding or freeing on the render thread.

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
        Ok(Self { config, slots })
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
    membership: Vec<Option<NodeId>>,
    config: PreparedConfig,
}

impl PreparedPlan {
    /// Control-side envelope for an untrusted descriptor. Only storage bounds are
    /// checked here. This deliberately does not certify graph validity.
    pub fn from_descriptor(
        config: PreparedConfig,
        descriptor: PlanDescriptor,
    ) -> Result<Self, Error> {
        let limits = config.limits();
        if descriptor.steps.len() > limits.max_nodes
            || descriptor.connections.len() > limits.max_edges
        {
            return Err(Error::Capacity);
        }
        let mut membership = vec![None; limits.max_nodes];
        for step in &descriptor.steps {
            if let Some(slot) = membership.get_mut(step.node.slot() as usize) {
                *slot = Some(step.node);
            }
        }
        Ok(Self {
            descriptor,
            membership,
            config,
        })
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
            .is_some_and(|slot| *slot == Some(id))
    }
}

/// Allocating control-side compiler. Equivalent descriptions compile identically
/// regardless of declaration order; ties break by stable node ID and source port.
pub fn compile(catalog: &Catalog, graph: &GraphDescription) -> Result<PreparedPlan, Error> {
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
}

impl PlanValidator {
    pub fn new(config: PreparedConfig) -> Self {
        Self {
            config,
            seen: vec![None; config.limits().max_nodes],
            phase: Phase::Idle,
            edge: 0,
            previous: None,
        }
    }

    pub fn begin(&mut self) {
        self.phase = Phase::Header;
        self.edge = 0;
        self.previous = None;
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
                    self.phase = Phase::Done;
                } else {
                    let edge = *descriptor
                        .connections
                        .get(self.edge)
                        .ok_or(Error::InvalidState)?;
                    self.validate_edge(edge, None, catalog)?;
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
