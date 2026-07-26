//! Deterministic driver for tests: decides how many frames each cycle processes.
//!
//! Two modes. In `Automatic` it hands out a full buffer every cycle, like a real
//! driver would. In `Controlled` it hands out only what a test has explicitly
//! requested, in buffer-sized chunks, so a test can advance the engine by an
//! exact number of samples and assert on the result.
//!
//! The C++ driver owned a thread that called the engine on a timer. Here the
//! caller drives the loop and this type only decides chunk sizes, which is the
//! part that carries behaviour. Threading belongs with a real driver.
//!
//! No `Driver` trait yet: JACK pushes from its own callback while this pulls, and
//! inventing an abstraction that fits both before either exists would be a guess.

use crate::dummy_port::{DummyExternalConnections, ExternalPortDescriptor, PortId};
use crate::port::{PortDataType, PortDirection};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriverSettings {
    pub sample_rate: u32,
    pub buffer_size: u32,
    pub client_name: String,
}

impl Default for DriverSettings {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            buffer_size: 256,
            client_name: "dummy".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverMode {
    /// Hand out a full buffer every cycle.
    Automatic,
    /// Hand out only explicitly requested samples.
    Controlled,
}

#[derive(Debug)]
pub struct DummyDriver {
    settings: DriverSettings,
    mode: DriverMode,
    samples_to_process: u32,
    active: bool,
    paused: bool,
    xruns: u32,
    dsp_load: f32,
    external: DummyExternalConnections,
}

impl Default for DummyDriver {
    fn default() -> Self {
        Self {
            settings: DriverSettings::default(),
            mode: DriverMode::Automatic,
            samples_to_process: 0,
            active: false,
            paused: false,
            xruns: 0,
            dsp_load: 0.0,
            external: DummyExternalConnections::default(),
        }
    }
}

impl DummyDriver {
    pub fn start(&mut self, settings: DriverSettings) {
        self.settings = settings;
        self.dsp_load = 0.0;
        self.active = true;
    }

    pub fn close(&mut self) {
        self.active = false;
        self.samples_to_process = 0;
    }

    pub fn active(&self) -> bool {
        self.active
    }
    pub fn sample_rate(&self) -> u32 {
        self.settings.sample_rate
    }
    pub fn buffer_size(&self) -> u32 {
        self.settings.buffer_size
    }
    pub fn client_name(&self) -> &str {
        &self.settings.client_name
    }
    pub fn xruns(&self) -> u32 {
        self.xruns
    }
    pub fn reset_xruns(&mut self) {
        self.xruns = 0;
    }
    pub fn report_xrun(&mut self) {
        self.xruns += 1;
    }
    pub fn dsp_load(&self) -> f32 {
        self.dsp_load
    }

    pub fn mode(&self) -> DriverMode {
        self.mode
    }

    /// Switches mode, discarding any outstanding request.
    ///
    /// A pending request belongs to the mode it was made in, so carrying it across
    /// a switch would advance the engine unexpectedly.
    pub fn enter_mode(&mut self, mode: DriverMode) {
        if self.mode != mode {
            self.mode = mode;
            self.samples_to_process = 0;
        }
    }

    pub fn pause(&mut self) {
        self.paused = true;
    }
    pub fn resume(&mut self) {
        self.paused = false;
    }
    pub fn paused(&self) -> bool {
        self.paused
    }

    /// Asks for `samples` more frames to be processed, in controlled mode.
    pub fn request_samples(&mut self, samples: u32) {
        self.samples_to_process += samples;
    }
    pub fn samples_to_process(&self) -> u32 {
        self.samples_to_process
    }

    /// Frames the next cycle should process, consuming them from any request.
    ///
    /// Zero means there is nothing to do: the driver is inactive, paused, or in
    /// controlled mode with nothing requested.
    pub fn next_chunk(&mut self) -> u32 {
        if !self.active || self.paused {
            return 0;
        }
        match self.mode {
            DriverMode::Automatic => self.settings.buffer_size,
            DriverMode::Controlled => {
                let n = self.samples_to_process.min(self.settings.buffer_size);
                self.samples_to_process -= n;
                n
            }
        }
    }

    /// Chunk sizes that would drain the current request, without running anything.
    ///
    /// Useful for asserting how a request splits across buffers.
    pub fn planned_chunks(&self) -> Vec<u32> {
        if !self.active || self.paused || self.mode == DriverMode::Automatic {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut left = self.samples_to_process;
        while left > 0 {
            let n = left.min(self.settings.buffer_size);
            out.push(n);
            left -= n;
        }
        out
    }

    // --- mock external ports ---

    pub fn external(&self) -> &DummyExternalConnections {
        &self.external
    }
    pub fn external_mut(&mut self) -> &mut DummyExternalConnections {
        &mut self.external
    }

    pub fn add_external_mock_port(
        &mut self,
        name: impl Into<String>,
        direction: PortDirection,
        data_type: PortDataType,
    ) {
        self.external.add_mock_port(name, direction, data_type);
    }
    pub fn remove_external_mock_port(&mut self, name: &str) {
        self.external.remove_mock_port(name);
    }
    pub fn remove_all_external_mock_ports(&mut self) {
        self.external.remove_all_mock_ports();
    }

    pub fn find_external_ports(
        &self,
        name_pattern: Option<&str>,
        direction: PortDirection,
        data_type: PortDataType,
    ) -> Vec<ExternalPortDescriptor> {
        self.external
            .find_external_ports(name_pattern, direction, data_type)
            .unwrap_or_default()
    }

    pub fn connect_external(&mut self, port: PortId, external: &str) -> bool {
        self.external.connect(port, external).is_ok()
    }
    pub fn disconnect_external(&mut self, port: PortId, external: &str) -> bool {
        self.external.disconnect(port, external).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert2::check;

    fn started() -> DummyDriver {
        let mut d = DummyDriver::default();
        d.start(DriverSettings {
            sample_rate: 48000,
            buffer_size: 4,
            client_name: "test".to_string(),
        });
        d
    }

    #[test]
    fn is_inactive_until_started() {
        let mut d = DummyDriver::default();
        check!(!d.active());
        check!(d.next_chunk() == 0);

        d.start(DriverSettings::default());
        check!(d.active());
        check!(d.client_name() == "dummy");
        check!(d.sample_rate() == 48000);
        check!(d.buffer_size() == 256);
    }

    #[test]
    fn automatic_mode_hands_out_full_buffers() {
        let mut d = started();
        check!(d.mode() == DriverMode::Automatic);
        check!(d.next_chunk() == 4);
        check!(d.next_chunk() == 4);
    }

    #[test]
    fn controlled_mode_hands_out_nothing_unrequested() {
        let mut d = started();
        d.enter_mode(DriverMode::Controlled);
        check!(d.next_chunk() == 0);
    }

    #[test]
    fn a_request_is_handed_out_in_buffer_sized_chunks() {
        let mut d = started();
        d.enter_mode(DriverMode::Controlled);
        d.request_samples(10);
        check!(d.planned_chunks() == vec![4, 4, 2]);

        check!(d.next_chunk() == 4);
        check!(d.next_chunk() == 4);
        check!(d.next_chunk() == 2);
        // Request drained.
        check!(d.next_chunk() == 0);
        check!(d.samples_to_process() == 0);
    }

    #[test]
    fn a_request_smaller_than_a_buffer_is_handed_out_exactly() {
        let mut d = started();
        d.enter_mode(DriverMode::Controlled);
        d.request_samples(3);
        check!(d.next_chunk() == 3);
        check!(d.next_chunk() == 0);
    }

    #[test]
    fn requests_accumulate() {
        let mut d = started();
        d.enter_mode(DriverMode::Controlled);
        d.request_samples(2);
        d.request_samples(3);
        check!(d.samples_to_process() == 5);
        check!(d.planned_chunks() == vec![4, 1]);
    }

    #[test]
    fn switching_mode_discards_an_outstanding_request() {
        let mut d = started();
        d.enter_mode(DriverMode::Controlled);
        d.request_samples(8);
        d.enter_mode(DriverMode::Automatic);
        check!(d.samples_to_process() == 0);
        // Re-entering controlled mode starts clean rather than resuming.
        d.enter_mode(DriverMode::Controlled);
        check!(d.next_chunk() == 0);
    }

    #[test]
    fn re_entering_the_same_mode_keeps_the_request() {
        let mut d = started();
        d.enter_mode(DriverMode::Controlled);
        d.request_samples(8);
        d.enter_mode(DriverMode::Controlled);
        check!(d.samples_to_process() == 8);
    }

    #[test]
    fn pausing_stops_handing_out_work() {
        let mut d = started();
        d.enter_mode(DriverMode::Controlled);
        d.request_samples(8);
        d.pause();
        check!(d.paused());
        check!(d.next_chunk() == 0);
        // The request survives the pause.
        check!(d.samples_to_process() == 8);
        d.resume();
        check!(d.next_chunk() == 4);
    }

    #[test]
    fn closing_deactivates_and_drops_the_request() {
        let mut d = started();
        d.enter_mode(DriverMode::Controlled);
        d.request_samples(8);
        d.close();
        check!(!d.active());
        check!(d.samples_to_process() == 0);
        check!(d.next_chunk() == 0);
    }

    #[test]
    fn xruns_are_counted_and_resettable() {
        let mut d = started();
        check!(d.xruns() == 0);
        d.report_xrun();
        d.report_xrun();
        check!(d.xruns() == 2);
        d.reset_xruns();
        check!(d.xruns() == 0);
    }

    #[test]
    fn mock_external_ports_are_exposed_and_filterable() {
        let mut d = started();
        d.add_external_mock_port("sys:in_1", PortDirection::Input, PortDataType::Audio);
        d.add_external_mock_port("sys:out_1", PortDirection::Output, PortDataType::Audio);
        d.add_external_mock_port("sys:midi_1", PortDirection::Input, PortDataType::Midi);

        check!(
            d.find_external_ports(None, PortDirection::Any, PortDataType::Any)
                .len()
                == 3
        );
        check!(
            d.find_external_ports(None, PortDirection::Input, PortDataType::Midi)
                .len()
                == 1
        );
        check!(
            d.find_external_ports(Some("sys:in_.*"), PortDirection::Any, PortDataType::Any)
                .len()
                == 1
        );

        d.remove_external_mock_port("sys:in_1");
        check!(
            d.find_external_ports(None, PortDirection::Any, PortDataType::Any)
                .len()
                == 2
        );
        d.remove_all_external_mock_ports();
        check!(d
            .find_external_ports(None, PortDirection::Any, PortDataType::Any)
            .is_empty());
    }

    #[test]
    fn connecting_to_a_missing_external_port_fails() {
        let mut d = started();
        check!(!d.connect_external(PortId(1), "nope"));
        d.add_external_mock_port("sys:in_1", PortDirection::Input, PortDataType::Audio);
        check!(d.connect_external(PortId(1), "sys:in_1"));
        check!(d.disconnect_external(PortId(1), "sys:in_1"));
    }

    #[test]
    fn planned_chunks_is_empty_in_automatic_mode() {
        let mut d = started();
        d.request_samples(10);
        // Automatic mode does not consume requests, so nothing is planned.
        check!(d.planned_chunks().is_empty());
    }
}
