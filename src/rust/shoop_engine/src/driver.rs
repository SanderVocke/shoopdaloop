//! What the drivers have in common, and a dummy driver that owns an engine.
//!
//! The trait is deliberately narrow. Extracting it waited until three drivers existed,
//! and having them made clear what they do *not* share: how cycles arrive. The dummy
//! driver is pulled by its caller, JACK pushes from one callback covering both
//! directions, and cpal pushes from two callbacks that have to be bridged through a
//! ring. A trait with a `process` method would fit two of the three and misfit the one
//! whose shape is hardest.
//!
//! So [`Driver`] covers only what is genuinely common: what the backend settled on, how
//! to reach the engine, and where the counters are. Driving cycles stays with each
//! driver, because that is exactly where they differ.

use crate::dummy_driver::{DriverMode, DriverSettings, DummyDriver};
use crate::engine::{Engine, EngineHandle, Stats};
use crate::session::Session;

use std::sync::Arc;

use enum_iterator::Sequence;
use num_enum::{IntoPrimitive, TryFromPrimitive};

#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Sequence)]
#[repr(i32)]
pub enum AudioDriverType {
    Jack = 0,
    JackTest = 1,
    Dummy = 2,
    Cpal = 3,
}

#[derive(Clone, Debug)]
pub struct AudioDriverState {
    pub dsp_load_percent: f32,
    pub xruns_since_last: u32,
    pub maybe_instance_name: String,
    pub sample_rate: u32,
    pub buffer_size: u32,
    pub active: u32,
    pub last_processed: u32,
}

#[derive(Debug, Clone)]
pub struct BackendSessionState {
    pub audio_driver: *mut (),
    pub n_audio_buffers_created: u32,
    pub n_audio_buffers_available: u32,
}

pub fn driver_type_supported(driver_type: AudioDriverType) -> bool {
    matches!(
        driver_type,
        AudioDriverType::Dummy
            | AudioDriverType::Jack
            | AudioDriverType::JackTest
            | AudioDriverType::Cpal
    )
}

#[cfg(feature = "cpal")]
fn cpal_device_label(device: &cpal::Device) -> String {
    use cpal::traits::DeviceTrait;
    device.name().unwrap_or_else(|_| "cpal".to_string())
}

#[cfg(feature = "cpal")]
fn cpal_host_label(id: cpal::HostId) -> String {
    format!("{id:?}").to_lowercase()
}

#[cfg(feature = "cpal")]
pub fn cpal_host_names() -> Vec<String> {
    cpal::available_hosts()
        .into_iter()
        .map(cpal_host_label)
        .collect()
}

#[cfg(not(feature = "cpal"))]
pub fn cpal_host_names() -> Vec<String> {
    Vec::new()
}

#[cfg(feature = "cpal")]
pub fn cpal_output_device_names_for_host(host: &str) -> Vec<String> {
    use cpal::traits::HostTrait;
    let selected = if host == "default" || host.is_empty() {
        Ok(cpal::default_host())
    } else {
        cpal::available_hosts()
            .into_iter()
            .find(|id| cpal_host_label(*id) == host.to_lowercase())
            .ok_or(())
            .and_then(|id| cpal::host_from_id(id).map_err(|_| ()))
    };
    selected
        .and_then(|h| h.output_devices().map_err(|_| ()))
        .map(|devices| devices.map(|d| cpal_device_label(&d)).collect())
        .unwrap_or_default()
}

#[cfg(not(feature = "cpal"))]
pub fn cpal_output_device_names_for_host(_host: &str) -> Vec<String> {
    Vec::new()
}

#[cfg(feature = "cpal")]
pub fn cpal_input_device_names_for_host(host: &str) -> Vec<String> {
    use cpal::traits::HostTrait;
    let selected = if host == "default" || host.is_empty() {
        Ok(cpal::default_host())
    } else {
        cpal::available_hosts()
            .into_iter()
            .find(|id| cpal_host_label(*id) == host.to_lowercase())
            .ok_or(())
            .and_then(|id| cpal::host_from_id(id).map_err(|_| ()))
    };
    selected
        .and_then(|h| h.input_devices().map_err(|_| ()))
        .map(|devices| devices.map(|d| cpal_device_label(&d)).collect())
        .unwrap_or_default()
}

#[cfg(not(feature = "cpal"))]
pub fn cpal_input_device_names_for_host(_host: &str) -> Vec<String> {
    Vec::new()
}

pub fn cpal_output_device_names() -> Vec<String> {
    cpal_output_device_names_for_host("default")
}

pub fn cpal_input_device_names() -> Vec<String> {
    cpal_input_device_names_for_host("default")
}

#[cfg(feature = "midir")]
pub fn midir_input_port_names() -> Vec<String> {
    midir::MidiInput::new("ShoopDaLoop-list")
        .map(|m| {
            m.ports()
                .iter()
                .filter_map(|p| m.port_name(p).ok())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(not(feature = "midir"))]
pub fn midir_input_port_names() -> Vec<String> {
    Vec::new()
}

#[cfg(feature = "midir")]
pub fn midir_output_port_names() -> Vec<String> {
    midir::MidiOutput::new("ShoopDaLoop-list")
        .map(|m| {
            m.ports()
                .iter()
                .filter_map(|p| m.port_name(p).ok())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(not(feature = "midir"))]
pub fn midir_output_port_names() -> Vec<String> {
    Vec::new()
}

/// Common surface of a running driver.
pub trait Driver {
    /// Rate the backend is running at, which may not be what was asked for.
    fn sample_rate(&self) -> u32;
    /// Frames per cycle. Some backends vary this; treat it as the latest value.
    fn buffer_size(&self) -> u32;
    /// Name the backend knows this client by.
    fn client_name(&self) -> &str;
    /// Counters the engine and driver publish.
    fn stats(&self) -> &Arc<Stats>;
    /// The only way to reach the engine while it is running.
    fn handle(&mut self) -> &mut EngineHandle;
}

/// A driver with no backend, pulled by its caller.
///
/// For tests and for headless runs -- a self-test wants the engine to advance by an
/// exact number of frames, not by whatever an audio device happened to ask for. Cycle
/// sizes come from [`DummyDriver`], so its chunking behaviour is shared rather than
/// duplicated here.
pub struct DummyEngineDriver {
    engine: Engine,
    handle: EngineHandle,
    clock: DummyDriver,
}

impl DummyEngineDriver {
    pub fn start(
        session: Session,
        command_queue_capacity: usize,
        settings: DriverSettings,
    ) -> Self {
        let mut clock = DummyDriver::default();
        // Controlled: nothing runs until asked, which is what makes a test able to
        // advance by an exact number of frames.
        clock.enter_mode(DriverMode::Controlled);
        clock.start(settings);

        let (engine, handle) = crate::engine::split(session, command_queue_capacity);
        Self {
            engine,
            handle,
            clock,
        }
    }

    /// Runs cycles until `n_frames` have been processed, and reports how many were.
    ///
    /// Split into buffer-sized cycles by the same rule a real driver would use, so a
    /// request larger than one buffer exercises the same path as several callbacks.
    pub fn request_frames(&mut self, n_frames: u32) -> u32 {
        self.clock.request_samples(n_frames);
        let mut done = 0;
        loop {
            let chunk = self.clock.next_chunk();
            if chunk == 0 {
                break;
            }
            self.engine.process(chunk as usize);
            done += chunk;
        }
        done
    }

    /// The engine, for a caller that owns this driver outright.
    ///
    /// Sound here and not on a real driver: nothing else holds the engine, so there is
    /// no other thread to race with.
    pub fn engine(&self) -> &Engine {
        &self.engine
    }
    pub fn engine_mut(&mut self) -> &mut Engine {
        &mut self.engine
    }

    pub fn close(&mut self) {
        self.clock.close();
    }
}

impl Driver for DummyEngineDriver {
    fn sample_rate(&self) -> u32 {
        self.clock.sample_rate()
    }
    fn buffer_size(&self) -> u32 {
        self.clock.buffer_size()
    }
    fn client_name(&self) -> &str {
        self.clock.client_name()
    }
    fn stats(&self) -> &Arc<Stats> {
        self.engine.stats()
    }
    fn handle(&mut self) -> &mut EngineHandle {
        &mut self.handle
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loop_mode::LoopMode;
    use assert2::{check, let_assert};

    fn settings(buffer_size: u32) -> DriverSettings {
        DriverSettings {
            sample_rate: 48000,
            buffer_size,
            client_name: "dummy-engine".to_string(),
        }
    }

    fn driver(buffer_size: u32) -> DummyEngineDriver {
        let mut s = Session::default();
        s.apply_graph_changes().expect("schedule");
        DummyEngineDriver::start(s, 16, settings(buffer_size))
    }

    #[test]
    fn nothing_runs_until_frames_are_requested() {
        let d = driver(64);
        check!(d.stats().cycles.load(std::sync::atomic::Ordering::Relaxed) == 0);
        check!(d.sample_rate() == 48000);
        check!(d.buffer_size() == 64);
        check!(d.client_name() == "dummy-engine");
    }

    #[test]
    fn a_request_is_split_into_buffer_sized_cycles() {
        let mut d = driver(64);

        check!(d.request_frames(160) == 160);
        // 64 + 64 + 32, so three cycles rather than one oversized one.
        check!(d.stats().cycles.load(std::sync::atomic::Ordering::Relaxed) == 3);
        check!(d.stats().frames.load(std::sync::atomic::Ordering::Relaxed) == 160);
    }

    #[test]
    fn control_work_and_cycles_meet() {
        let mut d = driver(64);
        let_assert!(
            Ok(()) = d.handle().send(Box::new(|s: &mut Session| {
                let l = s.create_loop();
                s.loop_mut(l).expect("loop").set_length(1000);
                let _ = s.set_loop_mode(l, LoopMode::Playing);
                let _ = s.apply_graph_changes();
            }))
        );

        d.request_frames(128);

        let_assert!(Some(snap) = d.handle().poll());
        check!(snap.loops.len() == 1);
        check!(snap.loops[0].mode == LoopMode::Playing);
        check!(snap.loops[0].position == 128);
    }

    #[test]
    fn driver_state_is_derived_from_the_trait() {
        let mut d = driver(64);

        let before = driver_state(&d);
        check!(before.instance_name == "dummy-engine");
        check!(before.sample_rate == 48000);
        check!(before.buffer_size == 64);
        // Nothing has run, so there is no evidence the driver is live.
        check!(!before.active);
        check!(before.last_processed == 0);
        check!(before.xruns == 0);

        d.request_frames(128);

        let after = driver_state(&d);
        check!(after.active);
        check!(after.last_processed == 2);
    }

    /// The point of the trait: a caller can report on any driver without knowing which.
    #[test]
    fn the_trait_is_usable_without_knowing_the_driver() {
        fn describe(d: &dyn Driver) -> String {
            format!(
                "{} at {} Hz, {} frames",
                d.client_name(),
                d.sample_rate(),
                d.buffer_size()
            )
        }

        let d = driver(128);
        check!(describe(&d) == "dummy-engine at 48000 Hz, 128 frames");
    }
}

/// State over whichever driver is running.
///
/// `active` is a bool where the C struct used `u32` to cross a C boundary, and
/// `last_processed` is the cycle count rather than a timestamp -- what a caller wants
/// from it is "is this thing still running", and a counter answers that without needing
/// a clock the two sides agree on.
#[derive(Debug, Clone, PartialEq)]
pub struct DriverState {
    pub dsp_load_percent: f32,
    pub xruns: u32,
    pub instance_name: String,
    pub sample_rate: u32,
    pub buffer_size: u32,
    pub active: bool,
    pub last_processed: u32,
}

/// State of any driver, for the control side to report.
///
/// A free function rather than a trait method: it is derived entirely from the trait's
/// own accessors, so an implementation cannot get it wrong or forget to update it.
pub fn driver_state(d: &dyn Driver) -> DriverState {
    use std::sync::atomic::Ordering;
    let stats = d.stats();
    let cycles = stats.cycles.load(Ordering::Relaxed);
    DriverState {
        dsp_load_percent: stats.dsp_load_percent(),
        xruns: stats.xruns.load(Ordering::Relaxed),
        instance_name: d.client_name().to_string(),
        sample_rate: d.sample_rate(),
        buffer_size: d.buffer_size(),
        // Having run a cycle is the only evidence available that a backend is live, and
        // it is the same evidence `send_and_wait` relies on.
        active: cycles > 0,
        last_processed: cycles,
    }
}
