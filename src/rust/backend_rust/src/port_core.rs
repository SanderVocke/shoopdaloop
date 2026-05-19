//! PortCore - General port metadata struct.
//!
//! Provides name, direction, and driver handle storage.
//! External connection status queries are handled on the C++ side.
//!
//! Designed to be composed into any port type (dummy, JACK, etc.).

use crate::port_direction::PortDirection;

pub struct PortCore {
    name: String,
    direction: PortDirection,
    driver_handle: usize,
}

impl PortCore {
    pub fn new(name: &str, direction: PortDirection, driver_handle: usize) -> Self {
        PortCore {
            name: name.to_string(),
            direction,
            driver_handle,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn direction(&self) -> PortDirection {
        self.direction
    }

    pub fn is_output(&self) -> bool {
        self.direction == PortDirection::Output
    }

    pub fn maybe_driver_handle(&self) -> usize {
        self.driver_handle
    }
}
