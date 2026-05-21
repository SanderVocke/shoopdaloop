//! CXX bridge for GraphNode to expose to C++.
//!
//! This module provides the FFI interface for C++ code to interact with
//! the Rust GraphNode implementation.
//!
//! Since CXX requires concrete types, we create a GraphNodeWrapperCxx
//! that wraps the full GraphNodeWrapper implementation.

#![allow(dead_code)]

use crate::graph_node::{GraphNodeVirtual, GraphNodeWrapper};
use std::cell::RefCell;
use std::collections::HashMap;

// ============================================================================
// GraphNodeWrapperCxx - CXX-compatible wrapper
// ============================================================================

/// CXX-compatible wrapper around GraphNodeWrapper.
/// This type can be exported via CXX and delegates to the actual implementation.
pub struct GraphNodeWrapperCxx {
    inner: GraphNodeWrapper,
    self_ptr: usize,
}

impl GraphNodeWrapperCxx {
    pub fn new() -> Self {
        GraphNodeWrapperCxx {
            inner: GraphNodeWrapper::new(Box::new(EmptyGraphNode)),
            self_ptr: 0,
        }
    }

    pub fn new_named(name: &str) -> Self {
        GraphNodeWrapperCxx {
            inner: GraphNodeWrapper::new(Box::new(NamedGraphNode(name.to_string()))),
            self_ptr: 0,
        }
    }

    pub fn init_self_ptr(&mut self) {
        self.self_ptr = self as *mut GraphNodeWrapperCxx as usize;
    }

    pub fn as_usize(&self) -> usize {
        self.self_ptr
    }

    pub fn name(&self) -> String {
        self.inner.graph_node_name()
    }

    pub fn outgoing_count(&self) -> usize {
        self.inner.graph_node_outgoing_edges().len()
    }

    pub fn incoming_count(&self) -> usize {
        self.inner.graph_node_incoming_edges().len()
    }

    pub fn coprocess_count(&self) -> usize {
        self.inner.graph_node_co_process_nodes().len()
    }

    pub fn process(&mut self, nframes: u32) {
        self.inner.PROC_process(nframes);
    }
}

impl Default for GraphNodeWrapperCxx {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for GraphNodeWrapperCxx {
    fn drop(&mut self) {
        unregister_graph_node(self.self_ptr);
    }
}

/// Empty graph node implementation for CXX bridge
struct EmptyGraphNode;

impl GraphNodeVirtual for EmptyGraphNode {}

/// Named graph node for CXX bridge
struct NamedGraphNode(String);

impl GraphNodeVirtual for NamedGraphNode {
    fn graph_node_name(&self) -> String {
        self.0.clone()
    }
}

// ============================================================================
// Node Registry - stores node data for graph processing
// ============================================================================

// Thread-local registry to map raw pointers to full node data
thread_local! {
    static GRAPH_NODE_REGISTRY: RefCell<HashMap<usize, NodeRegistryEntry>> = RefCell::new(HashMap::new());
}

/// Entry stored in the registry for each registered node
#[derive(Clone)]
pub struct NodeRegistryEntry {
    pub outgoing_ptrs: Vec<usize>,
    pub incoming_ptrs: Vec<usize>,
    pub coprocess_ptrs: Vec<usize>,
    pub name: String,
}

impl Default for NodeRegistryEntry {
    fn default() -> Self {
        NodeRegistryEntry {
            outgoing_ptrs: Vec::new(),
            incoming_ptrs: Vec::new(),
            coprocess_ptrs: Vec::new(),
            name: String::new(),
        }
    }
}

/// Register a node in the registry (CXX bridge compatible signature)
fn register_graph_node(
    ptr: usize,
    name: &str,
    outgoing_ptrs: &Vec<usize>,
    incoming_ptrs: &Vec<usize>,
    coprocess_ptrs: &Vec<usize>,
) {
    GRAPH_NODE_REGISTRY.with(|registry| {
        registry.borrow_mut().insert(
            ptr,
            NodeRegistryEntry {
                outgoing_ptrs: outgoing_ptrs.clone(),
                incoming_ptrs: incoming_ptrs.clone(),
                coprocess_ptrs: coprocess_ptrs.clone(),
                name: name.to_string(),
            },
        );
    });
}

/// Update the outgoing edges for a registered node
fn update_graph_node_outgoing(ptr: usize, outgoing_ptrs: &Vec<usize>) {
    GRAPH_NODE_REGISTRY.with(|registry| {
        if let Some(entry) = registry.borrow_mut().get_mut(&ptr) {
            entry.outgoing_ptrs = outgoing_ptrs.clone();
        }
    });
}

/// Update the incoming edges for a registered node
fn update_graph_node_incoming(ptr: usize, incoming_ptrs: &Vec<usize>) {
    GRAPH_NODE_REGISTRY.with(|registry| {
        if let Some(entry) = registry.borrow_mut().get_mut(&ptr) {
            entry.incoming_ptrs = incoming_ptrs.clone();
        }
    });
}

/// Update the co-process nodes for a registered node
fn update_graph_node_coprocess(ptr: usize, coprocess_ptrs: &Vec<usize>) {
    GRAPH_NODE_REGISTRY.with(|registry| {
        if let Some(entry) = registry.borrow_mut().get_mut(&ptr) {
            entry.coprocess_ptrs = coprocess_ptrs.clone();
        }
    });
}

/// Get a node entry from the registry by pointer
fn get_graph_node_entry(ptr: usize) -> Option<NodeRegistryEntry> {
    GRAPH_NODE_REGISTRY.with(|registry| registry.borrow().get(&ptr).cloned())
}

/// Unregister a node from the registry
fn unregister_graph_node(ptr: usize) {
    GRAPH_NODE_REGISTRY.with(|registry| {
        registry.borrow_mut().remove(&ptr);
    });
}

/// Clear all registered nodes
fn clear_graph_node_registry() {
    GRAPH_NODE_REGISTRY.with(|registry| {
        registry.borrow_mut().clear();
    });
}

// ============================================================================
// CXX Bridge
// ============================================================================

#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    extern "Rust" {
        type GraphNodeWrapperCxx;

        // Node creation
        fn new_graph_node_wrapper_cxx() -> Box<GraphNodeWrapperCxx>;
        fn new_named_graph_node_wrapper_cxx(name: &str) -> Box<GraphNodeWrapperCxx>;

        // Node properties
        fn graph_node_wrapper_cxx_name(wrapper: &GraphNodeWrapperCxx) -> String;
        fn graph_node_wrapper_cxx_outgoing_count(wrapper: &GraphNodeWrapperCxx) -> usize;
        fn graph_node_wrapper_cxx_incoming_count(wrapper: &GraphNodeWrapperCxx) -> usize;
        fn graph_node_wrapper_cxx_coprocess_count(wrapper: &GraphNodeWrapperCxx) -> usize;

        // Processing
        #[allow(non_snake_case)]
        fn PROC_process_graph_node(wrapper: &mut GraphNodeWrapperCxx, nframes: u32);

        // Node registration and edge management
        fn register_graph_node(ptr: usize, name: &str, outgoing_ptrs: &Vec<usize>, incoming_ptrs: &Vec<usize>, coprocess_ptrs: &Vec<usize>);
        fn unregister_graph_node(ptr: usize);
        fn update_graph_node_outgoing(ptr: usize, outgoing_ptrs: &Vec<usize>);
        fn update_graph_node_incoming(ptr: usize, incoming_ptrs: &Vec<usize>);
        fn update_graph_node_coprocess(ptr: usize, coprocess_ptrs: &Vec<usize>);
        fn clear_graph_node_registry();

        // Graph processing order computation
        // Returns flat Vec<usize> where usize::MAX acts as group separator
        fn compute_graph_processing_order(input_ptrs: &Vec<usize>) -> Vec<usize>;
    }
}

fn new_graph_node_wrapper_cxx() -> Box<GraphNodeWrapperCxx> {
    Box::new(GraphNodeWrapperCxx::new())
}

fn new_named_graph_node_wrapper_cxx(name: &str) -> Box<GraphNodeWrapperCxx> {
    Box::new(GraphNodeWrapperCxx::new_named(name))
}

fn graph_node_wrapper_cxx_name(wrapper: &GraphNodeWrapperCxx) -> String {
    wrapper.name()
}

fn graph_node_wrapper_cxx_outgoing_count(wrapper: &GraphNodeWrapperCxx) -> usize {
    wrapper.outgoing_count()
}

fn graph_node_wrapper_cxx_incoming_count(wrapper: &GraphNodeWrapperCxx) -> usize {
    wrapper.incoming_count()
}

fn graph_node_wrapper_cxx_coprocess_count(wrapper: &GraphNodeWrapperCxx) -> usize {
    wrapper.coprocess_count()
}

#[allow(non_snake_case)]
fn PROC_process_graph_node(wrapper: &mut GraphNodeWrapperCxx, nframes: u32) {
    wrapper.process(nframes);
}

// ============================================================================
// Graph Processing Order Algorithm
// ============================================================================

/// Helper struct for graph processing
#[derive(Clone)]
struct AnnotatedProcessingNode {
    nodes: Vec<usize>,
    incoming: Vec<usize>,
    outgoing: Vec<usize>,
}

/// CXX bridge function for graph processing order computation
/// Takes input node pointers and returns flat Vec<usize> with usize::MAX as group separator
/// Format: [ptr1, ptr2, ..., usize::MAX, ptr3, ptr4, ..., usize::MAX, ...]
fn compute_graph_processing_order(input_ptrs: &Vec<usize>) -> Vec<usize> {
    if input_ptrs.is_empty() {
        return vec![];
    }

    // Collect all unique nodes and build node-to-idx mapping
    let mut all_node_ptrs: Vec<usize> = Vec::new();
    let mut node_to_idx: HashMap<usize, usize> = HashMap::new();

    for &ptr in input_ptrs {
        if !node_to_idx.contains_key(&ptr) {
            let idx = all_node_ptrs.len();
            node_to_idx.insert(ptr, idx);
            all_node_ptrs.push(ptr);

            // Also add co-process nodes
            if let Some(entry) = get_graph_node_entry(ptr) {
                for &coprocess_ptr in &entry.coprocess_ptrs {
                    if !node_to_idx.contains_key(&coprocess_ptr) {
                        let idx = all_node_ptrs.len();
                        node_to_idx.insert(coprocess_ptr, idx);
                        all_node_ptrs.push(coprocess_ptr);
                    }
                }
            }
        }
    }

    // Build annotated nodes - groups of co-processed nodes
    let mut annotated_nodes: Vec<AnnotatedProcessingNode> = Vec::new();
    let mut node_to_annotated: HashMap<usize, usize> = HashMap::new();

    // Create annotated nodes for each unique node pointer
    for &ptr in &all_node_ptrs {
        if !node_to_annotated.contains_key(&ptr) {
            let annotated_idx = annotated_nodes.len();
            node_to_annotated.insert(ptr, annotated_idx);
            let node_idx = node_to_idx[&ptr];

            let mut annotated = AnnotatedProcessingNode {
                nodes: vec![node_idx],
                incoming: Vec::new(),
                outgoing: Vec::new(),
            };

            // Add co-process nodes to this group
            if let Some(entry) = get_graph_node_entry(ptr) {
                for &coprocess_ptr in &entry.coprocess_ptrs {
                    if let Some(&coprocess_idx) = node_to_idx.get(&coprocess_ptr) {
                        if !annotated.nodes.contains(&coprocess_idx) {
                            annotated.nodes.push(coprocess_idx);
                        }
                        node_to_annotated.insert(coprocess_ptr, annotated_idx);
                    }
                }
            }

            annotated_nodes.push(annotated);
        }
    }

    // Build edges based on outgoing/incoming from registry
    for annotated_idx in 0..annotated_nodes.len() {
        for &node_idx in &annotated_nodes[annotated_idx].nodes.clone() {
            let node_ptr = all_node_ptrs[node_idx];

            if let Some(entry) = get_graph_node_entry(node_ptr) {
                // Process outgoing edges
                for &out_ptr in &entry.outgoing_ptrs {
                    if let Some(&other_annotated_idx) = node_to_annotated.get(&out_ptr) {
                        if other_annotated_idx != annotated_idx {
                            if !annotated_nodes[annotated_idx].outgoing.contains(&other_annotated_idx) {
                                annotated_nodes[annotated_idx].outgoing.push(other_annotated_idx);
                            }
                            if !annotated_nodes[other_annotated_idx].incoming.contains(&annotated_idx) {
                                annotated_nodes[other_annotated_idx].incoming.push(annotated_idx);
                            }
                        }
                    }
                }

                // Process incoming edges
                for &in_ptr in &entry.incoming_ptrs {
                    if let Some(&other_annotated_idx) = node_to_annotated.get(&in_ptr) {
                        if other_annotated_idx != annotated_idx {
                            if !annotated_nodes[annotated_idx].incoming.contains(&other_annotated_idx) {
                                annotated_nodes[annotated_idx].incoming.push(other_annotated_idx);
                            }
                            if !annotated_nodes[other_annotated_idx].outgoing.contains(&annotated_idx) {
                                annotated_nodes[other_annotated_idx].outgoing.push(annotated_idx);
                            }
                        }
                    }
                }
            }
        }
    }

    // Kahn's algorithm - topological sort
    let mut result: Vec<usize> = Vec::new();
    let mut unscheduled: HashMap<usize, AnnotatedProcessingNode> = (0..annotated_nodes.len())
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
            // Cycle detected - return what we have
            break;
        }

        // Sort by name for deterministic ordering
        let mut to_schedule: Vec<(usize, String)> = Vec::new();
        for &idx in &no_incoming {
            let first_node_idx = unscheduled[&idx].nodes.first().copied().unwrap_or(0);
            let node_ptr = all_node_ptrs[first_node_idx];
            let name = get_graph_node_entry(node_ptr)
                .map(|e| e.name)
                .unwrap_or_default();
            to_schedule.push((idx, name));
        }
        to_schedule.sort_by(|a, b| a.1.cmp(&b.1));

        // Collect node indices for this group
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
            // Convert indices to pointers
            for &node_idx in &group_nodes {
                result.push(all_node_ptrs[node_idx]);
            }
            // Add separator
            result.push(usize::MAX);
        }
    }

    // Remove trailing separator if present
    if let Some(&last) = result.last() {
        if last == usize::MAX {
            result.pop();
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_processing_order_empty() {
        clear_graph_node_registry();
        let result = compute_graph_processing_order(&vec![]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_compute_processing_order_single_node() {
        clear_graph_node_registry();
        let ptr = 0x1000usize;
        register_graph_node(ptr, "node1", &vec![], &vec![], &vec![]);
        
        let result = compute_graph_processing_order(&vec![ptr]);
        // Should return [ptr] with no separator (single group)
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], ptr);
    }

    #[test]
    fn test_compute_processing_order_two_nodes() {
        clear_graph_node_registry();
        let ptr1 = 0x1000usize;
        let ptr2 = 0x2000usize;
        // ptr1 depends on ptr2 (ptr1 has incoming from ptr2)
        register_graph_node(ptr1, "a_node", &vec![], &vec![ptr2], &vec![]);
        register_graph_node(ptr2, "b_node", &vec![ptr1], &vec![], &vec![]);
        
        let result = compute_graph_processing_order(&vec![ptr1, ptr2]);
        // ptr2 should come first (no incoming), then ptr1
        // Format: ptr2, SEP, ptr1
        let expected: Vec<usize> = vec![ptr2, usize::MAX, ptr1];
        assert_eq!(result, expected);
    }
}