//! GraphNode trait and wrapper implementation.
//!
//! This module provides Rust equivalents for the C++ GraphNode class hierarchy.
//! Uses trait objects to emulate C++ virtual methods.
//!
//! # Architecture
//! - `GraphNodeVirtual`: Trait with all virtual methods from C++ GraphNode
//! - `GraphNodeWrapper`: Concrete wrapper that stores a boxed virtual trait object
//! - Timing wrappers `PROC_process()` and `PROC_co_process()` measure execution time

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

/// Type alias for weak references to GraphNode (similar to std::weak_ptr)
pub type WeakGraphNode = Rc<RefCell<dyn GraphNodeVirtual>>;

/// Type alias for strong references to GraphNode (similar to std::shared_ptr)
pub type SharedGraphNode = Arc<RefCell<dyn GraphNodeVirtual>>;

/// Wrapper for weak graph node references that can be stored in collections
#[derive(Clone)]
pub struct WeakGraphNodeKey {
    inner: WeakGraphNode,
}

impl WeakGraphNodeKey {
    pub fn new(inner: WeakGraphNode) -> Self {
        WeakGraphNodeKey { inner }
    }

    pub fn as_ptr(&self) -> *const () {
        self.inner.as_ptr() as *const ()
    }
}

impl PartialEq for WeakGraphNodeKey {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Eq for WeakGraphNodeKey {}

impl Ord for WeakGraphNodeKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_ptr().cmp(&other.as_ptr())
    }
}

impl PartialOrd for WeakGraphNodeKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl std::hash::Hash for WeakGraphNodeKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_ptr().hash(state);
    }
}

/// Wrapper for shared graph node references that can be stored in collections
#[derive(Clone)]
pub struct SharedGraphNodeKey {
    inner: SharedGraphNode,
}

impl SharedGraphNodeKey {
    pub fn new(inner: SharedGraphNode) -> Self {
        SharedGraphNodeKey { inner }
    }

    pub fn as_ptr(&self) -> *const () {
        self.inner.as_ptr() as *const ()
    }
}

impl PartialEq for SharedGraphNodeKey {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Eq for SharedGraphNodeKey {}

impl Ord for SharedGraphNodeKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_ptr().cmp(&other.as_ptr())
    }
}

impl PartialOrd for SharedGraphNodeKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl std::hash::Hash for SharedGraphNodeKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_ptr().hash(state);
    }
}

impl std::borrow::Borrow<SharedGraphNode> for SharedGraphNodeKey {
    fn borrow(&self) -> &SharedGraphNode {
        &self.inner
    }
}

impl AsRef<SharedGraphNode> for SharedGraphNodeKey {
    fn as_ref(&self) -> &SharedGraphNode {
        &self.inner
    }
}

/// Type alias for a set of weak GraphNode references
/// Using Vec for simplicity
pub type WeakGraphNodeSet = Vec<WeakGraphNode>;

/// Type alias for a set of shared GraphNode references
/// Using Vec since it's easier to work with and we track uniqueness by pointer
pub type SharedGraphNodeSet = Vec<SharedGraphNode>;

/// Callback type for profiling (called with process time in microseconds)
pub type ProcessedCallback = Box<dyn Fn(u32) + Send + Sync>;

/// Core trait defining the GraphNode interface.
/// Corresponds to the C++ GraphNode class with virtual methods.
pub trait GraphNodeVirtual: Send + Sync {
    /// Return outgoing edges from this node (edges to nodes that depend on this node)
    fn graph_node_outgoing_edges(&self) -> WeakGraphNodeSet {
        WeakGraphNodeSet::new()
    }

    /// Return incoming edges to this node (edges from nodes that this node depends on)
    fn graph_node_incoming_edges(&self) -> WeakGraphNodeSet {
        WeakGraphNodeSet::new()
    }

    /// Return co-process nodes - nodes that should be processed together with this one
    fn graph_node_co_process_nodes(&self) -> WeakGraphNodeSet {
        WeakGraphNodeSet::new()
    }

    /// Return the name of this node (for debugging)
    fn graph_node_name(&self) -> String {
        "GraphNode".to_string()
    }

    /// Process audio/MIDI for this node
    fn graph_node_process(&mut self, _nframes: u32) {}

    /// Process this node with its co-process nodes
    fn graph_node_co_process(&mut self, _nodes: &SharedGraphNodeSet, _nframes: u32) {}

    /// Optional: Get the inner object if this wraps a specific type
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        None
    }

    /// Add an outgoing edge by raw pointer (for CXX bridge compatibility)
    fn add_outgoing_ptr(&mut self, _ptr: usize) {}

    /// Add an incoming edge by raw pointer (for CXX bridge compatibility)
    fn add_incoming_ptr(&mut self, _ptr: usize) {}

    /// Get raw outgoing edge pointers
    fn get_outgoing_ptrs(&self) -> Vec<usize> {
        vec![]
    }

    /// Get raw incoming edge pointers
    fn get_incoming_ptrs(&self) -> Vec<usize> {
        vec![]
    }
}

/// Wrapper struct that provides timing callbacks and owns a boxed virtual trait object.
/// This is the main implementation that C++ code will interact with through the CXX bridge.
pub struct GraphNodeWrapper {
    /// The underlying virtual node implementation
    virtual_node: Box<dyn GraphNodeVirtual>,
    /// Callback for profiling (called with process time in microseconds)
    processed_cb: Option<ProcessedCallback>,
}

impl GraphNodeWrapper {
    /// Create a new GraphNodeWrapper wrapping the given virtual node
    pub fn new(virtual_node: Box<dyn GraphNodeVirtual>) -> Self {
        GraphNodeWrapper {
            virtual_node,
            processed_cb: None,
        }
    }

    /// Create a new boxed GraphNodeWrapper wrapped in Arc<RefCell<...>>
    pub fn new_shared(virtual_node: Box<dyn GraphNodeVirtual>) -> SharedGraphNode {
        Arc::new(RefCell::new(Self::new(virtual_node)))
    }

    /// Set the callback to be called after processing with the time taken in microseconds
    pub fn set_processed_cb(&mut self, cb: Option<ProcessedCallback>) {
        self.processed_cb = cb;
    }

    /// Add an outgoing edge by raw pointer (stored as usize)
    /// The pointer is converted back to a WeakGraphNode when needed
    pub fn add_outgoing_by_ptr(&mut self, target_ptr: usize) {
        // For now, store the raw pointer in the virtual_node
        // The actual node resolution happens at graph_processing time
        self.virtual_node.add_outgoing_ptr(target_ptr);
    }

    /// Add an incoming edge by raw pointer
    pub fn add_incoming_by_ptr(&mut self, source_ptr: usize) {
        self.virtual_node.add_incoming_ptr(source_ptr);
    }

    /// Get raw outgoing edge pointers
    pub fn get_outgoing_ptrs(&self) -> Vec<usize> {
        self.virtual_node.get_outgoing_ptrs()
    }

    /// Get raw incoming edge pointers
    pub fn get_incoming_ptrs(&self) -> Vec<usize> {
        self.virtual_node.get_incoming_ptrs()
    }

    /// Get outgoing edges
    pub fn graph_node_outgoing_edges(&self) -> WeakGraphNodeSet {
        self.virtual_node.graph_node_outgoing_edges()
    }

    /// Get incoming edges
    pub fn graph_node_incoming_edges(&self) -> WeakGraphNodeSet {
        self.virtual_node.graph_node_incoming_edges()
    }

    /// Get co-process nodes
    pub fn graph_node_co_process_nodes(&self) -> WeakGraphNodeSet {
        self.virtual_node.graph_node_co_process_nodes()
    }

    /// Get node name
    pub fn graph_node_name(&self) -> String {
        self.virtual_node.graph_node_name()
    }

    /// Process with timing measurement
    #[allow(non_snake_case)]
    pub fn PROC_process(&mut self, nframes: u32) {
        match &self.processed_cb {
            Some(cb) => {
                let start = Instant::now();
                self.virtual_node.graph_node_process(nframes);
                let duration = start.elapsed();
                let micros = (duration.as_nanos() / 1000) as u32;
                cb(micros);
            }
            None => {
                self.virtual_node.graph_node_process(nframes);
            }
        }
    }

    /// Process with co-process nodes and timing measurement
    #[allow(non_snake_case)]
    pub fn PROC_co_process(&mut self, nodes: &SharedGraphNodeSet, nframes: u32) {
        match &self.processed_cb {
            Some(cb) => {
                let start = Instant::now();
                self.virtual_node.graph_node_co_process(nodes, nframes);
                let duration = start.elapsed();
                let micros = (duration.as_nanos() / 1000) as u32;
                cb(micros);
            }
            None => {
                self.virtual_node.graph_node_co_process(nodes, nframes);
            }
        }
    }

    /// Get mutable reference to the underlying virtual node
    pub fn as_mut(&mut self) -> &mut dyn GraphNodeVirtual {
        &mut *self.virtual_node
    }
}

/// Blanket implementation that allows SharedGraphNode to be used where GraphNodeVirtual is expected
impl GraphNodeVirtual for GraphNodeWrapper {
    fn graph_node_outgoing_edges(&self) -> WeakGraphNodeSet {
        self.graph_node_outgoing_edges()
    }

    fn graph_node_incoming_edges(&self) -> WeakGraphNodeSet {
        self.graph_node_incoming_edges()
    }

    fn graph_node_co_process_nodes(&self) -> WeakGraphNodeSet {
        self.graph_node_co_process_nodes()
    }

    fn graph_node_name(&self) -> String {
        self.graph_node_name()
    }

    fn graph_node_process(&mut self, nframes: u32) {
        self.virtual_node.graph_node_process(nframes);
    }

    fn graph_node_co_process(&mut self, nodes: &SharedGraphNodeSet, nframes: u32) {
        self.virtual_node.graph_node_co_process(nodes, nframes);
    }
}

/// Helper trait for nodes that hold a weak reference to a parent.
/// Corresponds to the C++ NodeWithParent template.
pub trait NodeWithParent<Parent>: GraphNodeVirtual {
    /// Get the parent reference
    fn parent(&self) -> Option<std::rc::Rc<Parent>>;
}

/// Helper function to extract parent from a node.
/// Corresponds to the C++ graph_node_parent_as function.
pub fn graph_node_parent_as<T: 'static>(node: &dyn GraphNodeVirtual) -> Option<Rc<T>> {
    node.as_any()
        .and_then(|any| any.downcast_ref::<Rc<T>>())
        .cloned()
}

/// Interface for objects that need to be notified of buffer size/sample rate changes.
/// Corresponds to the C++ NotifyProcessParametersInterface class.
pub trait NotifyProcessParametersInterface: Send + Sync {
    /// Called when the buffer size changes
    fn proc_notify_changed_buffer_size(&mut self, _buffer_size: u32) {}

    /// Called when the sample rate changes
    fn proc_notify_changed_sample_rate(&mut self, _sample_rate: u32) {}
}

/// Base interface for objects that contain graph nodes.
/// Corresponds to the C++ HasGraphNodesInterface class.
pub trait HasGraphNodesInterface: NotifyProcessParametersInterface {
    /// Return all graph nodes contained in this object
    fn all_graph_nodes(&self) -> SharedGraphNodeSet {
        SharedGraphNodeSet::new()
    }
}

/// Trait for objects that have a single graph node, with lazy creation.
/// Corresponds to the C++ HasGraphNode class.
///
/// Implementors should override the virtual methods for graph node behavior.
///
/// Note: Actual implementations will typically use interior mutability
/// (e.g., Arc<Mutex<T>>) to allow the graph node wrappers to call back
/// into the parent's processing methods.
pub trait HasGraphNode: HasGraphNodesInterface + Send + Sync + 'static {
    /// Return the name of this object's graph node
    fn graph_node_name(&self) -> String {
        "GraphNode".to_string()
    }

    /// Return outgoing edges for this object's graph node
    fn graph_node_outgoing_edges(&self) -> WeakGraphNodeSet {
        WeakGraphNodeSet::new()
    }

    /// Return incoming edges for this object's graph node
    fn graph_node_incoming_edges(&self) -> WeakGraphNodeSet {
        WeakGraphNodeSet::new()
    }

    /// Return co-process nodes for this object's graph node
    fn graph_node_co_process_nodes(&self) -> WeakGraphNodeSet {
        WeakGraphNodeSet::new()
    }

    /// Process the graph node - implementors should use interior mutability
    fn graph_node_process(&self, _nframes: u32) {}

    /// Co-process the graph node with other nodes - implementors should use interior mutability
    fn graph_node_co_process(&self, _nodes: &SharedGraphNodeSet, _nframes: u32) {}
}

/// Internal wrapper node for HasGraphNode implementation.
/// Stores an Arc reference to the parent and delegates to its virtual methods.
/// Uses Arc::upgrade for weak reference handling.
struct HasGraphNodeWrapper<Parent: HasGraphNode> {
    parent: std::sync::Arc<Parent>,
}

impl<Parent: HasGraphNode> HasGraphNodeWrapper<Parent> {
    fn new(parent: std::sync::Arc<Parent>) -> Self {
        HasGraphNodeWrapper { parent }
    }
}

impl<Parent: HasGraphNode> GraphNodeVirtual for HasGraphNodeWrapper<Parent> {
    fn graph_node_name(&self) -> String {
        self.parent.graph_node_name()
    }

    fn graph_node_outgoing_edges(&self) -> WeakGraphNodeSet {
        self.parent.graph_node_outgoing_edges()
    }

    fn graph_node_incoming_edges(&self) -> WeakGraphNodeSet {
        self.parent.graph_node_incoming_edges()
    }

    fn graph_node_co_process_nodes(&self) -> WeakGraphNodeSet {
        self.parent.graph_node_co_process_nodes()
    }

    fn graph_node_process(&mut self, nframes: u32) {
        self.parent.graph_node_process(nframes);
    }

    fn graph_node_co_process(&mut self, nodes: &SharedGraphNodeSet, nframes: u32) {
        self.parent.graph_node_co_process(nodes, nframes);
    }
}

/// Trait for objects that have two graph nodes, with lazy creation.
/// Corresponds to the C++ HasTwoGraphNodes class.
pub trait HasTwoGraphNodes: HasGraphNodesInterface + Send + Sync + 'static {
    // First node virtual methods
    fn graph_node_0_name(&self) -> String {
        "GraphNode".to_string()
    }
    fn graph_node_0_outgoing_edges(&self) -> WeakGraphNodeSet {
        WeakGraphNodeSet::new()
    }
    fn graph_node_0_incoming_edges(&self) -> WeakGraphNodeSet {
        WeakGraphNodeSet::new()
    }
    fn graph_node_0_co_process_nodes(&self) -> WeakGraphNodeSet {
        WeakGraphNodeSet::new()
    }
    fn graph_node_0_process(&self, _nframes: u32) {}
    fn graph_node_0_co_process(&self, _nodes: &SharedGraphNodeSet, _nframes: u32) {}

    // Second node virtual methods
    fn graph_node_1_name(&self) -> String {
        "GraphNode".to_string()
    }
    fn graph_node_1_outgoing_edges(&self) -> WeakGraphNodeSet {
        WeakGraphNodeSet::new()
    }
    fn graph_node_1_incoming_edges(&self) -> WeakGraphNodeSet {
        WeakGraphNodeSet::new()
    }
    fn graph_node_1_co_process_nodes(&self) -> WeakGraphNodeSet {
        WeakGraphNodeSet::new()
    }
    fn graph_node_1_process(&self, _nframes: u32) {}
    fn graph_node_1_co_process(&self, _nodes: &SharedGraphNodeSet, _nframes: u32) {}
}

/// Internal wrapper for the first graph node of HasTwoGraphNodes
struct FirstNodeWrapper<Parent: HasTwoGraphNodes> {
    parent: std::sync::Arc<Parent>,
}

impl<Parent: HasTwoGraphNodes> FirstNodeWrapper<Parent> {
    fn new(parent: std::sync::Arc<Parent>) -> Self {
        FirstNodeWrapper { parent }
    }
}

impl<Parent: HasTwoGraphNodes> GraphNodeVirtual for FirstNodeWrapper<Parent> {
    fn graph_node_name(&self) -> String {
        self.parent.graph_node_0_name()
    }

    fn graph_node_outgoing_edges(&self) -> WeakGraphNodeSet {
        self.parent.graph_node_0_outgoing_edges()
    }

    fn graph_node_incoming_edges(&self) -> WeakGraphNodeSet {
        self.parent.graph_node_0_incoming_edges()
    }

    fn graph_node_co_process_nodes(&self) -> WeakGraphNodeSet {
        self.parent.graph_node_0_co_process_nodes()
    }

    fn graph_node_process(&mut self, nframes: u32) {
        self.parent.graph_node_0_process(nframes);
    }

    fn graph_node_co_process(&mut self, nodes: &SharedGraphNodeSet, nframes: u32) {
        self.parent.graph_node_0_co_process(nodes, nframes);
    }
}

/// Internal wrapper for the second graph node of HasTwoGraphNodes
struct SecondNodeWrapper<Parent: HasTwoGraphNodes> {
    parent: std::sync::Arc<Parent>,
}

impl<Parent: HasTwoGraphNodes> SecondNodeWrapper<Parent> {
    fn new(parent: std::sync::Arc<Parent>) -> Self {
        SecondNodeWrapper { parent }
    }
}

impl<Parent: HasTwoGraphNodes> GraphNodeVirtual for SecondNodeWrapper<Parent> {
    fn graph_node_name(&self) -> String {
        self.parent.graph_node_1_name()
    }

    fn graph_node_outgoing_edges(&self) -> WeakGraphNodeSet {
        self.parent.graph_node_1_outgoing_edges()
    }

    fn graph_node_incoming_edges(&self) -> WeakGraphNodeSet {
        self.parent.graph_node_1_incoming_edges()
    }

    fn graph_node_co_process_nodes(&self) -> WeakGraphNodeSet {
        self.parent.graph_node_1_co_process_nodes()
    }

    fn graph_node_process(&mut self, nframes: u32) {
        self.parent.graph_node_1_process(nframes);
    }

    fn graph_node_co_process(&mut self, nodes: &SharedGraphNodeSet, nframes: u32) {
        self.parent.graph_node_1_co_process(nodes, nframes);
    }
}

/// Helper function to create a graph node for a HasGraphNode implementor
pub fn create_graph_node<Parent: HasGraphNode>(parent: std::sync::Arc<Parent>) -> SharedGraphNode {
    GraphNodeWrapper::new_shared(Box::new(HasGraphNodeWrapper::new(parent)))
}

/// Helper function to create the first graph node for a HasTwoGraphNodes implementor
pub fn create_first_graph_node<Parent: HasTwoGraphNodes>(
    parent: std::sync::Arc<Parent>,
) -> SharedGraphNode {
    GraphNodeWrapper::new_shared(Box::new(FirstNodeWrapper::new(parent)))
}

/// Helper function to create the second graph node for a HasTwoGraphNodes implementor
pub fn create_second_graph_node<Parent: HasTwoGraphNodes>(
    parent: std::sync::Arc<Parent>,
) -> SharedGraphNode {
    GraphNodeWrapper::new_shared(Box::new(SecondNodeWrapper::new(parent)))
}

/// Represents one or more nodes that should be co-processed together.
/// Annotated with incoming and outgoing edges for topological sorting.
#[derive(Clone)]
#[allow(dead_code)]
struct AnnotatedGraphNode {
    /// The actual node pointers (as indices into all_nodes)
    nodes: Vec<usize>,
    /// Annotated node indices this node depends on (incoming edges)
    incoming: Vec<usize>,
    /// Annotated node indices that depend on this node (outgoing edges)
    outgoing: Vec<usize>,
}

/// Compute the processing order for a set of graph nodes.
/// Returns indices into the returned all_nodes Vec, grouped for co-processing.
///
/// This is a variation on Kahn's algorithm for topological sorting.
#[allow(dead_code)]
pub fn graph_processing_order(
    input_nodes: &[SharedGraphNode],
) -> (Vec<usize>, Vec<Vec<usize>>) {
    use std::collections::HashMap;

    // Collect all unique nodes - store both pointer and shared ref
    // The key is the pointer address, the value is the shared reference
    let mut all_node_ptrs: Vec<usize> = Vec::new();
    let mut node_refs: Vec<SharedGraphNode> = Vec::new();
    let mut node_to_idx: HashMap<usize, usize> = HashMap::new();

    // Helper to add a node to our collections
    let add_node = |node: &SharedGraphNode,
                    all_node_ptrs: &mut Vec<usize>,
                    node_refs: &mut Vec<SharedGraphNode>,
                    node_to_idx: &mut HashMap<usize, usize>| {
        let ptr = node.as_ptr() as *const ();
        let ptr_usize = ptr as usize;
        if !node_to_idx.contains_key(&ptr_usize) {
            let idx = all_node_ptrs.len();
            node_to_idx.insert(ptr_usize, idx);
            all_node_ptrs.push(ptr_usize);
            node_refs.push(Arc::clone(node));
        }
    };

    // Add all input nodes and their co-process nodes
    for node in input_nodes {
        add_node(node, &mut all_node_ptrs, &mut node_refs, &mut node_to_idx);

        // Also add co-process nodes
        let borrowed = node.borrow();
        for co_node in borrowed.graph_node_co_process_nodes() {
            // co_node is WeakGraphNode (Rc<RefCell<...>>)
            // We need to get its raw pointer for comparison
            let co_ptr = co_node.as_ptr() as *const ();
            let co_ptr_usize = co_ptr as usize;
            if !node_to_idx.contains_key(&co_ptr_usize) {
                // Add by pointer only
                let idx = all_node_ptrs.len();
                node_to_idx.insert(co_ptr_usize, idx);
                all_node_ptrs.push(co_ptr_usize);
            }
        }
    }

    // Build annotated nodes - groups of co-processed nodes
    let mut annotated_nodes: Vec<AnnotatedGraphNode> = Vec::new();
    let mut node_to_annotated: HashMap<usize, usize> = HashMap::new();

    // First pass: create annotated nodes for each input node
    for node in input_nodes {
        let ptr = node.as_ptr() as *const ();
        let ptr_usize = ptr as usize;
        if !node_to_annotated.contains_key(&ptr_usize) {
            let annotated_idx = annotated_nodes.len();
            node_to_annotated.insert(ptr_usize, annotated_idx);

            // Get the node index
            let node_idx = node_to_idx[&ptr_usize];

            let mut annotated = AnnotatedGraphNode {
                nodes: vec![node_idx],
                incoming: Vec::new(),
                outgoing: Vec::new(),
            };

            // Add co-process nodes to this group
            let borrowed = node.borrow();
            for co_node in borrowed.graph_node_co_process_nodes() {
                let co_ptr = co_node.as_ptr() as *const ();
                let co_ptr_usize = co_ptr as usize;
                if let Some(&co_idx) = node_to_idx.get(&co_ptr_usize) {
                    if !annotated.nodes.contains(&co_idx) {
                        annotated.nodes.push(co_idx);
                    }
                }
                node_to_annotated.insert(co_ptr_usize, annotated_idx);
            }

            annotated_nodes.push(annotated);
        }
    }

    // Second pass: build edges based on outgoing and incoming dependencies
    // We need to find the node refs by pointer
    for annotated_idx in 0..annotated_nodes.len() {
        for &node_idx in &annotated_nodes[annotated_idx].nodes.clone() {
            // Find the actual node reference by pointer
            let node_ptr = all_node_ptrs[node_idx];
            let node_ref = input_nodes
                .iter()
                .find(|n| n.as_ptr() as *const () as usize == node_ptr)
                .cloned();

            if let Some(node_ref) = node_ref {
                let borrowed = node_ref.borrow();

                // Process outgoing edges
                for edge_ref in borrowed.graph_node_outgoing_edges() {
                    let edge_ptr = edge_ref.as_ptr() as *const ();
                    let edge_ptr_usize = edge_ptr as usize;
                    if let Some(&other_annotated_idx) = node_to_annotated.get(&edge_ptr_usize) {
                        if other_annotated_idx != annotated_idx {
                            if !annotated_nodes[annotated_idx]
                                .outgoing
                                .contains(&other_annotated_idx)
                            {
                                annotated_nodes[annotated_idx]
                                    .outgoing
                                    .push(other_annotated_idx);
                            }
                            if !annotated_nodes[other_annotated_idx]
                                .incoming
                                .contains(&annotated_idx)
                            {
                                annotated_nodes[other_annotated_idx]
                                    .incoming
                                    .push(annotated_idx);
                            }
                        }
                    }
                }

                // Process incoming edges
                for edge_ref in borrowed.graph_node_incoming_edges() {
                    let edge_ptr = edge_ref.as_ptr() as *const ();
                    let edge_ptr_usize = edge_ptr as usize;
                    if let Some(&other_annotated_idx) = node_to_annotated.get(&edge_ptr_usize) {
                        if other_annotated_idx != annotated_idx {
                            if !annotated_nodes[annotated_idx]
                                .incoming
                                .contains(&other_annotated_idx)
                            {
                                annotated_nodes[annotated_idx]
                                    .incoming
                                    .push(other_annotated_idx);
                            }
                            if !annotated_nodes[other_annotated_idx]
                                .outgoing
                                .contains(&annotated_idx)
                            {
                                annotated_nodes[other_annotated_idx]
                                    .outgoing
                                    .push(annotated_idx);
                            }
                        }
                    }
                }
            }
        }
    }

    // Kahn's algorithm - topological sort
    let mut scheduled: Vec<Vec<usize>> = Vec::new();
    let mut unscheduled: HashMap<usize, AnnotatedGraphNode> = (0..annotated_nodes.len())
        .map(|i| (i, annotated_nodes[i].clone()))
        .collect();

    while !unscheduled.is_empty() {
        // Find nodes with no incoming edges
        let no_incoming: Vec<usize> = unscheduled
            .iter()
            .filter(|(_, annotated)| annotated.incoming.is_empty())
            .map(|(&idx, _)| idx)
            .collect();

        if no_incoming.is_empty() {
            panic!("Cycle in graph or unsolvable co-processing constraint");
        }

        // Sort by name for deterministic ordering
        let mut to_schedule: Vec<(usize, String)> = Vec::new();
        for &idx in &no_incoming {
            let first_node_idx = unscheduled[&idx].nodes.first().copied().unwrap_or(0);
            let node_ptr = all_node_ptrs[first_node_idx];
            // Find the node name by looking up in input_nodes
            let name = input_nodes
                .iter()
                .find(|n| n.as_ptr() as *const () as usize == node_ptr)
                .map(|n| n.borrow().graph_node_name())
                .unwrap_or_default();
            to_schedule.push((idx, name));
        }
        to_schedule.sort_by(|a, b| a.1.cmp(&b.1));

        // Add to scheduled list - collect node indices for this group
        let mut group_nodes: Vec<usize> = Vec::new();
        for (idx, _) in to_schedule {
            let annotated = unscheduled.remove(&idx).unwrap();
            group_nodes.extend(annotated.nodes);

            // Remove this node from other nodes' incoming edges
            for other_annotated in unscheduled.values_mut() {
                other_annotated.incoming.retain(|&x| x != idx);
            }
        }

        if !group_nodes.is_empty() {
            scheduled.push(group_nodes);
        }
    }

    (all_node_ptrs, scheduled)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Simple test implementation of GraphNodeVirtual without edge sets
    struct TestNode {
        name: String,
    }

    impl TestNode {
        fn new(name: &str) -> Self {
            TestNode {
                name: name.to_string(),
            }
        }
    }

    impl GraphNodeVirtual for TestNode {
        fn graph_node_name(&self) -> String {
            self.name.clone()
        }
    }

    #[test]
    fn test_graph_node_wrapper_basic() {
        let test_node = TestNode::new("TestNode");
        let wrapper = GraphNodeWrapper::new(Box::new(test_node));

        assert_eq!(wrapper.graph_node_name(), "TestNode");
        assert!(wrapper.graph_node_outgoing_edges().is_empty());
        assert!(wrapper.graph_node_incoming_edges().is_empty());
    }

    #[test]
    fn test_graph_node_wrapper_shared() {
        let test_node = TestNode::new("SharedTest");
        let shared = GraphNodeWrapper::new_shared(Box::new(test_node));

        let borrowed = shared.borrow();
        assert_eq!(borrowed.graph_node_name(), "SharedTest");
    }

    #[test]
    fn test_graph_node_wrapper_processing() {
        struct SimpleNode;

        impl GraphNodeVirtual for SimpleNode {
            fn graph_node_process(&mut self, _nframes: u32) {
                // Minimal processing - nothing to do
            }
        }

        let mut wrapper = GraphNodeWrapper::new(Box::new(SimpleNode));

        // Process should work without callback
        wrapper.PROC_process(512);
        // If we get here without panic, the test passes
    }

    #[test]
    fn test_graph_node_wrapper_with_callback() {
        struct SimpleNode;

        impl GraphNodeVirtual for SimpleNode {
            fn graph_node_process(&mut self, _nframes: u32) {
                // Minimal processing
            }
        }

        let mut wrapper = GraphNodeWrapper::new(Box::new(SimpleNode));

        // Set up callback
        let callback_time = Arc::new(std::sync::Mutex::new(None));
        let callback_time_clone = callback_time.clone();

        wrapper.set_processed_cb(Some(Box::new(move |us| {
            *callback_time_clone.lock().unwrap() = Some(us);
        })));

        // Process should trigger callback
        wrapper.PROC_process(256);

        let captured_time = callback_time.lock().unwrap();
        assert!(captured_time.is_some(), "Callback should have been called");
    }

    #[test]
    fn test_graph_node_shared_access() {
        let node_c = GraphNodeWrapper::new_shared(Box::new(TestNode::new("C")));
        let node_b = GraphNodeWrapper::new_shared(Box::new(TestNode::new("B")));
        let node_a = GraphNodeWrapper::new_shared(Box::new(TestNode::new("A")));

        // Verify nodes can be accessed
        assert_eq!(node_a.borrow().graph_node_name(), "A");
        assert_eq!(node_b.borrow().graph_node_name(), "B");
        assert_eq!(node_c.borrow().graph_node_name(), "C");
    }

    #[test]
    fn test_graph_processing_order_single_node() {
        // Single node with no edges should return just that node
        let node = GraphNodeWrapper::new_shared(Box::new(TestNode::new("single")));
        let input_set: Vec<SharedGraphNode> = vec![node];

        let (all_ptrs, order) = graph_processing_order(&input_set);

        // Should have one node
        assert_eq!(all_ptrs.len(), 1);
        // Should have one group in the order
        assert_eq!(order.len(), 1);
        // That group should have one node
        assert_eq!(order[0].len(), 1);
    }

    #[test]
    fn test_graph_processing_order_simple_chain() {
        // Create three independent nodes (no edges between them)
        // Since there are no edges, they should all be processed in one step
        let node_c = GraphNodeWrapper::new_shared(Box::new(TestNode::new("C")));
        let node_b = GraphNodeWrapper::new_shared(Box::new(TestNode::new("B")));
        let node_a = GraphNodeWrapper::new_shared(Box::new(TestNode::new("A")));

        let input_set: Vec<SharedGraphNode> = vec![
            Arc::clone(&node_a),
            Arc::clone(&node_b),
            Arc::clone(&node_c),
        ];

        let (all_ptrs, order) = graph_processing_order(&input_set);

        // Should have 3 nodes
        assert_eq!(all_ptrs.len(), 3);
        // Since no edges, all nodes are independent and can be processed together
        // The result should have all 3 nodes in one step
        assert_eq!(order.len(), 1);
        assert_eq!(order[0].len(), 3);
    }
}
