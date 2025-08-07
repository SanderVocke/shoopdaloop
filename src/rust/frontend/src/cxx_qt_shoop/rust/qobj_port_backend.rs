use crate::cxx_qt_shoop::rust::qobj_port_backend_bridge::ffi::*;
use common::logging::macros::*;
use cxx_qt::CxxQtType;
use std::pin::Pin;
shoop_log_unit!("Frontend.Port");

impl PortBackend {
    pub fn get_initialized(&self) -> bool {
        todo!();
    }

    pub fn get_name(&self) -> &QString {
        todo!();
    }

    pub fn get_muted(&self) -> bool {
        todo!();
    }

    pub fn get_passthrough_muted(&self) -> bool {
        todo!();
    }

    pub fn get_audio_input_peak(&self) -> f64 {
        todo!();
    }

    pub fn get_audio_output_peak(&self) -> f64 {
        todo!();
    }

    pub fn get_midi_n_input_events(&self) -> i32 {
        todo!();
    }

    pub fn get_midi_n_output_events(&self) -> i32 {
        todo!();
    }

    pub fn get_midi_n_input_notes_active(&self) -> i32 {
        todo!();
    }

    pub fn get_midi_n_output_notes_active(&self) -> i32 {
        todo!();
    }

    pub fn initialize_impl(self: Pin<&mut PortBackend>) {
        todo!();
    }

    pub fn close(self: Pin<&mut PortBackend>) {
        todo!();
    }

    pub fn connect_external_port(self: Pin<&mut PortBackend>, name: QString) {
        todo!();
    }

    pub fn disconnect_external_port(self: Pin<&mut PortBackend>, name: QString) {
        todo!();
    }

    pub fn get_connections_state(self: Pin<&mut PortBackend>) -> QMap_QString_QVariant {
        todo!();
    }

    pub fn get_connected_external_ports(self: Pin<&mut PortBackend>) -> QList_QString {
        todo!();
    }

    pub fn try_make_connections(self: Pin<&mut PortBackend>, connections: QList_QString) {
        todo!();
    }

    pub unsafe fn set_backend(mut self: Pin<&mut PortBackend>, backend: *mut QObject) {
        todo!();
    }

    pub fn set_name_hint(mut self: Pin<&mut PortBackend>, name_hint: QString) {
        todo!();
    }

    pub fn set_input_connectability(mut self: Pin<&mut PortBackend>, input_connectability: i32) {
        todo!();
    }

    pub fn set_output_connectability(mut self: Pin<&mut PortBackend>, output_connectability: i32) {
        todo!();
    }

    pub fn set_is_internal(mut self: Pin<&mut PortBackend>, is_internal: bool) {
        todo!();
    }

    pub fn set_internal_port_connections(
        mut self: Pin<&mut PortBackend>,
        internal_port_connections: QList_QVariant,
    ) {
        todo!();
    }

    pub fn set_n_ringbuffer_samples(mut self: Pin<&mut PortBackend>, n_ringbuffer_samples: i32) {
        todo!();
    }

    pub fn set_audio_gain(mut self: Pin<&mut PortBackend>, audio_gain: f64) {
        todo!();
    }

    pub fn dummy_queue_audio_data(self: Pin<&mut PortBackend>, audio_data: QList_f64) {
        todo!();
    }

    pub fn dummy_dequeue_audio_data(self: Pin<&mut PortBackend>) -> QList_f64 {
        todo!();
    }

    pub fn dummy_request_data(self: Pin<&mut PortBackend>, n: i32) {
        todo!();
    }

    pub fn dummy_queue_midi_msgs(self: Pin<&mut PortBackend>, midi_msgs: QList_QVariant) {
        todo!();
    }

    pub fn dummy_dequeue_midi_msgs(self: Pin<&mut PortBackend>) -> QList_QVariant {
        todo!();
    }

    pub fn dummy_clear_queues(self: Pin<&mut PortBackend>) {
        todo!();
    }
}
