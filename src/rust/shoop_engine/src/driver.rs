//! What driver a session is running on, and what the host offers to run it on.
//!
//! Types and enumeration only. There used to be a `Driver` trait here, extracted once
//! three drivers existed; two of those three were duplicates of the drivers in
//! `app_backend.rs` and have been deleted, and the trait went with them rather than being
//! kept for a single implementation. Cycles arrive too differently to abstract usefully
//! anyway -- JACK pushes from one callback covering both directions, cpal from two that
//! have to be bridged through a ring, the dummy driver is pulled by its own thread -- so
//! each driver owns its own loop and this module owns only the vocabulary they report in.

use enum_iterator::Sequence;
use num_enum::{IntoPrimitive, TryFromPrimitive};

#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Sequence)]
#[repr(i32)]
pub enum AudioDriverType {
    Jack = 0,
    JackTest = 1,
    Dummy = 2,
    Cpal = 3,
    CpalTest = 4,
}

#[derive(Clone, Debug)]
pub struct AudioDriverState {
    pub dsp_load_percent: f32,
    pub xruns_since_last: u32,
    /// Cycles run against a schedule older than the topology, cumulative.
    ///
    /// Expected to tick up briefly after a topology change and then stop. A count that
    /// keeps climbing means graph changes are not being applied -- which presents as
    /// silence, so it is reported rather than left to be inferred.
    pub stale_graph_cycles: u32,
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
    pub cycles: u32,
    pub frames: u32,
    pub pending_commands: u32,
    pub commands_applied: u32,
    pub last_applied_command: u64,
    pub trace_snapshots_dropped: u32,
    pub capture_underruns: u32,
    pub capture_overruns: u32,
    pub graph_arms: u64,
    pub graph_applies: u64,
}

pub fn driver_type_supported(driver_type: AudioDriverType) -> bool {
    matches!(
        driver_type,
        AudioDriverType::Dummy
            | AudioDriverType::Jack
            | AudioDriverType::JackTest
            | AudioDriverType::Cpal
            | AudioDriverType::CpalTest
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
