//! CXX bridge for GraphNode to expose to C++.
//!
//! This module provides the FFI interface for C++ code to interact with
//! the Rust GraphNode implementation.
//!
//! Since CXX requires concrete types, we create a GraphNodeWrapperCxx
//! that wraps the full GraphNodeWrapper implementation.

#![allow(dead_code)]

use crate::graph_node::GraphNodeWrapper;

/// CXX-compatible wrapper around GraphNodeWrapper.
/// This type can be exported via CXX and delegates to the actual implementation.
pub struct GraphNodeWrapperCxx {
    inner: GraphNodeWrapper,
}

impl GraphNodeWrapperCxx {
    pub fn new() -> Self {
        GraphNodeWrapperCxx {
            inner: GraphNodeWrapper::new(Box::new(EmptyGraphNode)),
        }
    }

    pub fn new_named(name: &str) -> Self {
        GraphNodeWrapperCxx {
            inner: GraphNodeWrapper::new(Box::new(NamedGraphNode(name.to_string()))),
        }
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

/// Empty graph node implementation for CXX bridge
struct EmptyGraphNode;

impl crate::graph_node::GraphNodeVirtual for EmptyGraphNode {}

/// Named graph node for CXX bridge
struct NamedGraphNode(String);

impl crate::graph_node::GraphNodeVirtual for NamedGraphNode {
    fn graph_node_name(&self) -> String {
        self.0.clone()
    }
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
