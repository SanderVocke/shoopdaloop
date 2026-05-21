//! CXX bridge for GraphNode to expose to C++.
//!
//! This module provides the FFI interface for C++ code to interact with
//! the Rust GraphNode implementation.
//!
//! Since CXX requires concrete types, we create a GraphNodeWrapperCxx
//! that wraps the full GraphNodeWrapper implementation.

#![allow(dead_code)]

use crate::graph_node::{
    GraphNodeVirtual, GraphNodeWrapper,
};
use std::cell::RefCell;
use std::collections::HashMap;

/// CXX-compatible wrapper around GraphNodeWrapper.
/// This type can be exported via CXX and delegates to the actual implementation.
pub struct GraphNodeWrapperCxx {
    inner: GraphNodeWrapper,
    // Pointer to raw self for C++ interop
    self_ptr: *mut GraphNodeWrapperCxx,
}

impl GraphNodeWrapperCxx {
    pub fn new() -> Self {
        let inner = GraphNodeWrapper::new(Box::new(EmptyGraphNode));
        let self_ptr = std::ptr::null_mut(); // Will be set after construction
        GraphNodeWrapperCxx { inner, self_ptr }
    }

    pub fn new_named(name: &str) -> Self {
        let inner = GraphNodeWrapper::new(Box::new(NamedGraphNode(name.to_string())));
        let self_ptr = std::ptr::null_mut();
        GraphNodeWrapperCxx { inner, self_ptr }
    }

    /// Initialize the self pointer after construction
    pub fn init_self_ptr(&mut self) {
        self.self_ptr = self as *mut GraphNodeWrapperCxx;
    }

    /// Get the raw pointer for C++ interop
    pub fn as_usize(&self) -> usize {
        self.self_ptr as usize
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

    pub fn add_outgoing_edge(&mut self, target: usize) {
        // target is a raw pointer as usize, we need to convert to WeakGraphNode
        // Since we can't easily create a WeakGraphNode from a raw pointer in CXX,
        // we store the raw pointers and convert later
        self.inner.add_outgoing_by_ptr(target);
    }

    pub fn add_incoming_edge(&mut self, source: usize) {
        self.inner.add_incoming_by_ptr(source);
    }

    pub fn get_outgoing_ptrs(&self) -> Vec<usize> {
        self.inner.get_outgoing_ptrs()
    }

    pub fn get_incoming_ptrs(&self) -> Vec<usize> {
        self.inner.get_incoming_ptrs()
    }
}

impl Default for GraphNodeWrapperCxx {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for GraphNodeWrapperCxx {
    fn drop(&mut self) {
        // Clean up any registered references in the global registry
        GRAPH_NODE_REGISTRY.with(|registry| {
            let mut reg = registry.borrow_mut();
            reg.remove(&self.as_usize());
        });
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

/// Thread-local registry to map raw pointers back to Rust GraphNode instances
thread_local! {
    static GRAPH_NODE_REGISTRY: RefCell<HashMap<usize, ()>> = RefCell::new(HashMap::new());
}

/// Register a node in the registry (called from C++ after construction)
#[allow(dead_code)]
pub fn register_graph_node(ptr: usize) {
    GRAPH_NODE_REGISTRY.with(|registry| {
        registry.borrow_mut().insert(ptr, ());
    });
}

/// Get a node from the registry by pointer
#[allow(dead_code)]
pub fn is_graph_node_registered(ptr: usize) -> bool {
    GRAPH_NODE_REGISTRY.with(|registry| {
        registry.borrow().contains_key(&ptr)
    })
}

/// Clear all registered nodes
#[allow(dead_code)]
pub fn clear_graph_node_registry() {
    GRAPH_NODE_REGISTRY.with(|registry| {
        registry.borrow_mut().clear();
    });
}

#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    extern "Rust" {
        type GraphNodeWrapperCxx;

        fn new_graph_node_wrapper_cxx() -> Box<GraphNodeWrapperCxx>;
        fn new_named_graph_node_wrapper_cxx(name: &str) -> Box<GraphNodeWrapperCxx>;

        fn graph_node_wrapper_cxx_name(wrapper: &GraphNodeWrapperCxx) -> String;
        fn graph_node_wrapper_cxx_outgoing_count(wrapper: &GraphNodeWrapperCxx) -> usize;
        fn graph_node_wrapper_cxx_incoming_count(wrapper: &GraphNodeWrapperCxx) -> usize;
        fn graph_node_wrapper_cxx_coprocess_count(wrapper: &GraphNodeWrapperCxx) -> usize;

        #[allow(non_snake_case)]
        fn PROC_process_graph_node(wrapper: &mut GraphNodeWrapperCxx, nframes: u32);
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

/// Bridge for graph_processing_order - takes raw node pointers as input
/// Returns vector of groups, where each group contains raw node pointers
#[allow(dead_code)]
pub mod graph_processing_cxx {
    /// Compute processing order for given node pointers
    /// Input: list of raw node pointers (as usize)
    /// Output: (all_node_ptrs, processing_order) where processing_order is groups of indices into all_node_ptrs
    pub fn compute_processing_order(input_ptrs: &[usize]) -> Vec<Vec<usize>> {
        // For now, just return input as a single group since we don't have full SharedGraphNode setup
        // The actual implementation needs the full node refs
        if input_ptrs.is_empty() {
            vec![]
        } else {
            vec![input_ptrs.to_vec()]
        }
    }
}

// Export for CXX
pub use graph_processing_cxx::compute_processing_order;
