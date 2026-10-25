//! Preparation-time capacity and work-budget validation.

use crate::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Budgets {
    pub commands: usize,
    pub events: usize,
    pub subranges: usize,
    pub transfer_bytes: usize,
    pub validation_steps: usize,
    pub publications: usize,
}

impl Default for Budgets {
    fn default() -> Self {
        Self {
            commands: 8,
            events: 64,
            subranges: 65,
            transfer_bytes: 4096,
            validation_steps: 32,
            publications: 8,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Config {
    pub sample_rate: u32,
    pub max_frames: usize,
    pub audio_inputs: usize,
    pub audio_outputs: usize,
    pub midi_inputs: usize,
    pub midi_outputs: usize,
    pub max_nodes: usize,
    pub max_ports: usize,
    pub max_edges: usize,
    pub midi_events_per_port: usize,
    pub midi_bytes_per_port: usize,
    pub scheduled_events: usize,
    pub resource_slots: usize,
    pub resource_pages: usize,
    pub page_bytes: usize,
    /// Callback copy/validation headroom reserved for processor resource I/O.
    pub resource_io_bytes: usize,
    pub resource_io_steps: usize,
    pub command_slots: usize,
    pub budgets: Budgets,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            max_frames: 128,
            audio_inputs: 2,
            audio_outputs: 2,
            midi_inputs: 1,
            midi_outputs: 1,
            max_nodes: 64,
            max_ports: 128,
            max_edges: 256,
            midi_events_per_port: 256,
            midi_bytes_per_port: 4096,
            scheduled_events: 256,
            resource_slots: 32,
            resource_pages: 256,
            page_bytes: 4096,
            resource_io_bytes: 0,
            resource_io_steps: 0,
            command_slots: 64,
            budgets: Budgets::default(),
        }
    }
}

/// Immutable validated capacities. Retaining a copy of the source configuration
/// does not permit changing a running runtime's capacities.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreparedConfig {
    config: Config,
    audio_samples: usize,
    midi_bytes: usize,
    resource_bytes: usize,
}

// Use the same indexing ceiling on native and wasm32 hosts.
const MAX_STORAGE: usize = i32::MAX as usize;

fn storage_product(a: usize, b: usize, element_size: usize) -> Result<usize, Error> {
    let count = a.checked_mul(b).ok_or(Error::Capacity)?;
    let bytes = count.checked_mul(element_size).ok_or(Error::Capacity)?;
    if bytes > MAX_STORAGE {
        return Err(Error::Capacity);
    }
    Ok(count)
}

impl Config {
    pub fn prepare(self) -> Result<PreparedConfig, Error> {
        let positive = [
            self.max_frames,
            self.max_nodes,
            self.max_ports,
            self.max_edges,
            self.midi_events_per_port,
            self.midi_bytes_per_port,
            self.scheduled_events,
            self.resource_slots,
            self.resource_pages,
            self.page_bytes,
            self.command_slots,
            self.budgets.commands,
            self.budgets.events,
            self.budgets.subranges,
            self.budgets.transfer_bytes,
            self.budgets.validation_steps,
            self.budgets.publications,
        ];
        if self.sample_rate == 0 || positive.contains(&0) {
            return Err(Error::InvalidConfig);
        }
        if positive.iter().any(|&value| value > MAX_STORAGE)
            || self.audio_inputs > self.max_ports
            || self.audio_outputs > self.max_ports
            || self.midi_inputs > self.max_ports
            || self.midi_outputs > self.max_ports
        {
            return Err(Error::Capacity);
        }
        if self.page_bytes % size_of::<f32>() != 0 || self.budgets.transfer_bytes < self.page_bytes
        {
            return Err(Error::InvalidConfig);
        }
        if self.resource_io_bytes > self.budgets.transfer_bytes - self.page_bytes
            || self.resource_io_steps >= self.budgets.validation_steps
        {
            return Err(Error::InvalidConfig);
        }
        // Routes, injected ports, processors, and scheduled events can each
        // report MIDI overflow; their aggregate counter must also fit wasm32.
        let events = self
            .scheduled_events
            .min(self.budgets.events)
            .min(self.budgets.subranges - 1);
        let ranges = self.max_frames.min(events + 1);
        self.max_edges
            .checked_add(self.max_ports)
            .and_then(|work| work.checked_add(self.max_nodes))
            .and_then(|work| work.checked_mul(ranges))
            .and_then(|work| work.checked_add(events))
            .filter(|&work| work <= MAX_STORAGE)
            .ok_or(Error::Capacity)?;
        storage_product(
            self.max_nodes,
            1,
            size_of::<Option<crate::graph::NodeSpec>>(),
        )?;
        let audio_samples = storage_product(self.max_ports, self.max_frames, size_of::<f32>())?;
        let midi_bytes = storage_product(self.max_ports, self.midi_bytes_per_port, 1)?;
        let resource_bytes = storage_product(self.resource_pages, self.page_bytes, 1)?;
        storage_product(
            self.max_ports,
            self.midi_events_per_port,
            size_of::<crate::buffer::Event>(),
        )?;
        Ok(PreparedConfig {
            config: self,
            audio_samples,
            midi_bytes,
            resource_bytes,
        })
    }
}

impl PreparedConfig {
    pub fn limits(self) -> Config {
        self.config
    }

    pub fn audio_samples(self) -> usize {
        self.audio_samples
    }

    pub fn midi_bytes(self) -> usize {
        self.midi_bytes
    }

    pub fn resource_bytes(self) -> usize {
        self.resource_bytes
    }
}
