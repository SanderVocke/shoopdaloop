#![allow(dead_code)]

use crate::jack_api::{filter_port_names, with_test_api, JackApiObject};
use anyhow::anyhow;
use std::sync::Arc;

crate::define_rust_bridge_object_wrappers!(JackApiBridgeStrong, JackApiBridgeWeak, JackApiObject);

#[cxx::bridge(namespace = "backend_rust")]
pub mod ffi {
    unsafe extern "C++" {
        include!("internal/jack/JackApiCxxTrampolines.h");

        fn jackapi_process_cb(driver_ptr: usize, nframes: u32) -> i32;
        fn jackapi_xrun_cb(driver_ptr: usize) -> i32;
        fn jackapi_port_update_cb(driver_ptr: usize);
        fn jackapi_error_log(msg: &str);
        fn jackapi_info_log(msg: &str);
    }

    extern "Rust" {
        type JackApiBridgeStrong;
        type JackApiBridgeWeak;

        fn new_jack_api() -> Box<JackApiBridgeStrong>;
        fn new_jack_test_api() -> Box<JackApiBridgeStrong>;

        fn id(self: &JackApiBridgeStrong) -> u64;
        fn valid(self: &JackApiBridgeStrong) -> bool;
        fn downgrade(self: &JackApiBridgeStrong) -> Box<JackApiBridgeWeak>;
        fn clone_strong(self: &JackApiBridgeStrong) -> Box<JackApiBridgeStrong>;
        fn strong_count(self: &JackApiBridgeStrong) -> usize;
        fn weak_count(self: &JackApiBridgeStrong) -> usize;

        fn id(self: &JackApiBridgeWeak) -> u64;
        fn valid(self: &JackApiBridgeWeak) -> bool;
        fn upgrade(self: &JackApiBridgeWeak) -> Box<JackApiBridgeStrong>;
        fn clone(self: &JackApiBridgeWeak) -> Box<JackApiBridgeWeak>;
        fn strong_count(self: &JackApiBridgeWeak) -> usize;
        fn weak_count(self: &JackApiBridgeWeak) -> usize;

        fn jack_api_supports_processing(api: &JackApiBridgeStrong) -> bool;
        fn jack_api_init(api: &JackApiBridgeStrong) -> Result<()>;
        fn jack_api_client_open(api: &JackApiBridgeStrong, name: &str) -> Result<usize>;
        fn jack_api_activate(api: &JackApiBridgeStrong, client: usize) -> i32;
        fn jack_api_deactivate(api: &JackApiBridgeStrong, client: usize) -> i32;
        fn jack_api_client_close(api: &JackApiBridgeStrong, client: usize) -> i32;
        fn jack_api_get_client_name(api: &JackApiBridgeStrong, client: usize) -> String;
        fn jack_api_get_sample_rate(api: &JackApiBridgeStrong, client: usize) -> u32;
        fn jack_api_get_buffer_size(api: &JackApiBridgeStrong, client: usize) -> u32;
        fn jack_api_cpu_load(api: &JackApiBridgeStrong, client: usize) -> f32;

        fn jack_api_set_process_callback(api: &JackApiBridgeStrong, client: usize, driver_ptr: usize) -> i32;
        fn jack_api_set_xrun_callback(api: &JackApiBridgeStrong, client: usize, driver_ptr: usize) -> i32;
        fn jack_api_set_port_connect_callback(api: &JackApiBridgeStrong, client: usize, driver_ptr: usize) -> i32;
        fn jack_api_set_port_registration_callback(api: &JackApiBridgeStrong, client: usize, driver_ptr: usize) -> i32;
        fn jack_api_set_port_rename_callback(api: &JackApiBridgeStrong, client: usize, driver_ptr: usize) -> i32;
        fn jack_api_set_error_info_logging(api: &JackApiBridgeStrong);

        fn jack_api_port_register(
            api: &JackApiBridgeStrong,
            client: usize,
            name: &str,
            data_type: u32,
            direction: u32,
        ) -> usize;
        fn jack_api_port_unregister(api: &JackApiBridgeStrong, client: usize, port: usize) -> i32;
        fn jack_api_port_name(api: &JackApiBridgeStrong, port: usize) -> String;
        fn jack_api_port_get_buffer(api: &JackApiBridgeStrong, port: usize, nframes: u32) -> usize;
        fn jack_api_port_is_input(api: &JackApiBridgeStrong, port: usize) -> bool;
        fn jack_api_port_is_output(api: &JackApiBridgeStrong, port: usize) -> bool;
        fn jack_api_port_is_audio(api: &JackApiBridgeStrong, port: usize) -> bool;
        fn jack_api_port_is_midi(api: &JackApiBridgeStrong, port: usize) -> bool;
        fn jack_api_port_is_mine(api: &JackApiBridgeStrong, client: usize, port: usize) -> bool;
        fn jack_api_get_ports(api: &JackApiBridgeStrong, client: usize) -> Vec<String>;
        fn jack_api_get_filtered_ports(
            api: &JackApiBridgeStrong,
            client: usize,
            name_regex: &str,
            use_name_regex: bool,
            direction_filter: u32,
            use_direction_filter: bool,
            data_type_filter: u32,
            use_data_type_filter: bool,
        ) -> Vec<String>;
        fn jack_api_port_by_name(api: &JackApiBridgeStrong, client: usize, name: &str) -> usize;
        fn jack_api_port_get_all_connections(api: &JackApiBridgeStrong, client: usize, port: usize) -> Vec<String>;
        fn jack_api_connect(api: &JackApiBridgeStrong, client: usize, src: &str, dst: &str) -> i32;
        fn jack_api_disconnect(api: &JackApiBridgeStrong, client: usize, src: &str, dst: &str) -> i32;

        fn jack_api_midi_get_event_count(api: &JackApiBridgeStrong, buffer: usize) -> u32;
        unsafe fn jack_api_midi_event_get(
            api: &JackApiBridgeStrong,
            buffer: usize,
            index: u32,
            out_time: &mut u32,
            out_size: &mut u16,
            out_data: *mut u8,
        ) -> bool;
        fn jack_api_midi_clear_buffer(api: &JackApiBridgeStrong, buffer: usize);
        unsafe fn jack_api_midi_event_write(
            api: &JackApiBridgeStrong,
            buffer: usize,
            time: u32,
            size: u16,
            data: *const u8,
        ) -> i32;

        fn jack_test_api_reset(api: &JackApiBridgeStrong) -> bool;
        unsafe fn jack_test_api_push_midi_event(
            api: &JackApiBridgeStrong,
            port: usize,
            time: u32,
            size: u16,
            data: *const u8,
        ) -> bool;
        fn jack_test_api_clear_midi_buffer(api: &JackApiBridgeStrong, port: usize) -> bool;
        fn jack_test_api_midi_buffer_size(api: &JackApiBridgeStrong, port: usize) -> usize;
        unsafe fn jack_test_api_get_midi_event(
            api: &JackApiBridgeStrong,
            port: usize,
            index: usize,
            out_time: &mut u32,
            out_size: &mut u16,
            out_data: *mut u8,
        ) -> bool;
    }
}

fn new_jack_api() -> Box<JackApiBridgeStrong> {
    Box::new(JackApiBridgeStrong::from_arc(Arc::new(JackApiObject::new_real())))
}

fn new_jack_test_api() -> Box<JackApiBridgeStrong> {
    Box::new(JackApiBridgeStrong::from_arc(Arc::new(JackApiObject::new_test())))
}

fn jack_api_supports_processing(api: &JackApiBridgeStrong) -> bool {
    api.0.with(|api| api.inner.supports_processing()).unwrap_or(false)
}

fn jack_api_init(api: &JackApiBridgeStrong) -> Result<(), Box<dyn std::error::Error>> {
    api.0
        .with(|api| api.inner.init())
        .unwrap_or_else(|| Err(anyhow!("invalid JACK API bridge object")))
        .map_err(Into::into)
}

fn jack_api_client_open(
    api: &JackApiBridgeStrong,
    name: &str,
) -> Result<usize, Box<dyn std::error::Error>> {
    api.0
        .with(|api| api.inner.client_open(name))
        .unwrap_or_else(|| Err(anyhow!("invalid JACK API bridge object")))
        .map_err(Into::into)
}

fn jack_api_activate(api: &JackApiBridgeStrong, client: usize) -> i32 {
    api.0.with(|api| api.inner.activate(client)).unwrap_or(-1)
}
fn jack_api_deactivate(api: &JackApiBridgeStrong, client: usize) -> i32 {
    api.0.with(|api| api.inner.deactivate(client)).unwrap_or(-1)
}
fn jack_api_client_close(api: &JackApiBridgeStrong, client: usize) -> i32 {
    api.0.with(|api| api.inner.client_close(client)).unwrap_or(-1)
}
fn jack_api_get_client_name(api: &JackApiBridgeStrong, client: usize) -> String {
    api.0.with(|api| api.inner.get_client_name(client)).unwrap_or_default()
}
fn jack_api_get_sample_rate(api: &JackApiBridgeStrong, client: usize) -> u32 {
    api.0.with(|api| api.inner.get_sample_rate(client)).unwrap_or(0)
}
fn jack_api_get_buffer_size(api: &JackApiBridgeStrong, client: usize) -> u32 {
    api.0.with(|api| api.inner.get_buffer_size(client)).unwrap_or(0)
}
fn jack_api_cpu_load(api: &JackApiBridgeStrong, client: usize) -> f32 {
    api.0.with(|api| api.inner.cpu_load(client)).unwrap_or(0.0)
}

fn jack_api_set_process_callback(api: &JackApiBridgeStrong, client: usize, driver_ptr: usize) -> i32 {
    api.0.with(|api| api.inner.set_process_callback(client, driver_ptr)).unwrap_or(-1)
}
fn jack_api_set_xrun_callback(api: &JackApiBridgeStrong, client: usize, driver_ptr: usize) -> i32 {
    api.0.with(|api| api.inner.set_xrun_callback(client, driver_ptr)).unwrap_or(-1)
}
fn jack_api_set_port_connect_callback(api: &JackApiBridgeStrong, client: usize, driver_ptr: usize) -> i32 {
    api.0.with(|api| api.inner.set_port_connect_callback(client, driver_ptr)).unwrap_or(-1)
}
fn jack_api_set_port_registration_callback(api: &JackApiBridgeStrong, client: usize, driver_ptr: usize) -> i32 {
    api.0.with(|api| api.inner.set_port_registration_callback(client, driver_ptr)).unwrap_or(-1)
}
fn jack_api_set_port_rename_callback(api: &JackApiBridgeStrong, client: usize, driver_ptr: usize) -> i32 {
    api.0.with(|api| api.inner.set_port_rename_callback(client, driver_ptr)).unwrap_or(-1)
}
fn jack_api_set_error_info_logging(api: &JackApiBridgeStrong) {
    let _ = api.0.with(|api| api.inner.set_error_info_logging());
}

fn jack_api_port_register(
    api: &JackApiBridgeStrong,
    client: usize,
    name: &str,
    data_type: u32,
    direction: u32,
) -> usize {
    api.0
        .with(|api| api.inner.port_register(client, name, data_type, direction))
        .unwrap_or(0)
}
fn jack_api_port_unregister(api: &JackApiBridgeStrong, client: usize, port: usize) -> i32 {
    api.0.with(|api| api.inner.port_unregister(client, port)).unwrap_or(-1)
}
fn jack_api_port_name(api: &JackApiBridgeStrong, port: usize) -> String {
    api.0.with(|api| api.inner.port_name(port)).unwrap_or_default()
}
fn jack_api_port_get_buffer(api: &JackApiBridgeStrong, port: usize, nframes: u32) -> usize {
    api.0.with(|api| api.inner.port_get_buffer(port, nframes)).unwrap_or(0)
}
fn jack_api_port_is_input(api: &JackApiBridgeStrong, port: usize) -> bool {
    api.0.with(|api| api.inner.port_is_input(port)).unwrap_or(false)
}
fn jack_api_port_is_output(api: &JackApiBridgeStrong, port: usize) -> bool {
    api.0.with(|api| api.inner.port_is_output(port)).unwrap_or(false)
}
fn jack_api_port_is_audio(api: &JackApiBridgeStrong, port: usize) -> bool {
    api.0.with(|api| api.inner.port_is_audio(port)).unwrap_or(false)
}
fn jack_api_port_is_midi(api: &JackApiBridgeStrong, port: usize) -> bool {
    api.0.with(|api| api.inner.port_is_midi(port)).unwrap_or(false)
}
fn jack_api_port_is_mine(api: &JackApiBridgeStrong, client: usize, port: usize) -> bool {
    api.0.with(|api| api.inner.port_is_mine(client, port)).unwrap_or(false)
}
fn jack_api_get_ports(api: &JackApiBridgeStrong, client: usize) -> Vec<String> {
    api.0.with(|api| api.inner.get_ports(client)).unwrap_or_default()
}
fn jack_api_get_filtered_ports(
    api: &JackApiBridgeStrong,
    client: usize,
    name_regex: &str,
    use_name_regex: bool,
    direction_filter: u32,
    use_direction_filter: bool,
    data_type_filter: u32,
    use_data_type_filter: bool,
) -> Vec<String> {
    api.0
        .with(|api| {
            filter_port_names(
                api.inner.as_ref(),
                client,
                use_name_regex.then_some(name_regex),
                use_direction_filter.then_some(direction_filter),
                use_data_type_filter.then_some(data_type_filter),
            )
        })
        .unwrap_or_default()
}
fn jack_api_port_by_name(api: &JackApiBridgeStrong, client: usize, name: &str) -> usize {
    api.0.with(|api| api.inner.port_by_name(client, name)).unwrap_or(0)
}
fn jack_api_port_get_all_connections(api: &JackApiBridgeStrong, client: usize, port: usize) -> Vec<String> {
    api.0
        .with(|api| api.inner.port_get_all_connections(client, port))
        .unwrap_or_default()
}
fn jack_api_connect(api: &JackApiBridgeStrong, client: usize, src: &str, dst: &str) -> i32 {
    api.0.with(|api| api.inner.connect(client, src, dst)).unwrap_or(-1)
}
fn jack_api_disconnect(api: &JackApiBridgeStrong, client: usize, src: &str, dst: &str) -> i32 {
    api.0.with(|api| api.inner.disconnect(client, src, dst)).unwrap_or(-1)
}

fn jack_api_midi_get_event_count(api: &JackApiBridgeStrong, buffer: usize) -> u32 {
    api.0.with(|api| api.inner.midi_get_event_count(buffer)).unwrap_or(0)
}
unsafe fn jack_api_midi_event_get(
    api: &JackApiBridgeStrong,
    buffer: usize,
    index: u32,
    out_time: &mut u32,
    out_size: &mut u16,
    out_data: *mut u8,
) -> bool {
    api.0
        .with(|api| unsafe { api.inner.midi_event_get(buffer, index, out_time, out_size, out_data) })
        .unwrap_or(false)
}
fn jack_api_midi_clear_buffer(api: &JackApiBridgeStrong, buffer: usize) {
    let _ = api.0.with(|api| api.inner.midi_clear_buffer(buffer));
}
unsafe fn jack_api_midi_event_write(
    api: &JackApiBridgeStrong,
    buffer: usize,
    time: u32,
    size: u16,
    data: *const u8,
) -> i32 {
    api.0
        .with(|api| unsafe { api.inner.midi_event_write(buffer, time, size, data) })
        .unwrap_or(-1)
}

fn jack_test_api_reset(api: &JackApiBridgeStrong) -> bool {
    api.0
        .with(|api| with_test_api(api, |api| api.test_reset()).is_some())
        .unwrap_or(false)
}
unsafe fn jack_test_api_push_midi_event(
    api: &JackApiBridgeStrong,
    port: usize,
    time: u32,
    size: u16,
    data: *const u8,
) -> bool {
    api.0
        .with(|api| {
            with_test_api(api, |api| unsafe { api.test_push_midi_event(port, time, size, data) })
                .unwrap_or(false)
        })
        .unwrap_or(false)
}
fn jack_test_api_clear_midi_buffer(api: &JackApiBridgeStrong, port: usize) -> bool {
    api.0
        .with(|api| with_test_api(api, |api| api.test_clear_midi_buffer(port)).unwrap_or(false))
        .unwrap_or(false)
}
fn jack_test_api_midi_buffer_size(api: &JackApiBridgeStrong, port: usize) -> usize {
    api.0
        .with(|api| with_test_api(api, |api| api.test_midi_buffer_size(port)).unwrap_or(0))
        .unwrap_or(0)
}
unsafe fn jack_test_api_get_midi_event(
    api: &JackApiBridgeStrong,
    port: usize,
    index: usize,
    out_time: &mut u32,
    out_size: &mut u16,
    out_data: *mut u8,
) -> bool {
    api.0
        .with(|api| {
            with_test_api(api, |api| unsafe {
                api.test_get_midi_event(port, index, out_time, out_size, out_data)
            })
            .unwrap_or(false)
        })
        .unwrap_or(false)
}
