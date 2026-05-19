//! CXX bridge for PortCore to expose to C++.

#![allow(dead_code)]

use crate::port_core::PortCore;

#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    extern "Rust" {
        type PortCore;

        fn new_port_core(name: &str, is_output: bool, driver_handle: usize) -> Box<PortCore>;

        fn name(self: &PortCore) -> &str;
        fn is_output(self: &PortCore) -> bool;
        fn maybe_driver_handle(self: &PortCore) -> usize;
    }
}

fn new_port_core(name: &str, is_output: bool, driver_handle: usize) -> Box<PortCore> {
    use crate::port_direction::PortDirection;
    let direction = if is_output {
        PortDirection::Output
    } else {
        PortDirection::Input
    };
    Box::new(PortCore::new(name, direction, driver_handle))
}

fn name(port: &PortCore) -> &str {
    port.name()
}

fn is_output(port: &PortCore) -> bool {
    port.is_output()
}

fn maybe_driver_handle(port: &PortCore) -> usize {
    port.maybe_driver_handle()
}
