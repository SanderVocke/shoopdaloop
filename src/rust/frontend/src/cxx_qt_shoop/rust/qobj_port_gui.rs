use crate::cxx_qt_shoop::rust::qobj_port_gui_bridge::ffi::*;
use common::logging::macros::*;
use cxx_qt::CxxQtType;
use std::pin::Pin;
shoop_log_unit!("Frontend.Port");

impl PortGui {
    pub fn initialize_impl(self: Pin<&mut PortGui>) {}

    pub fn connect_external_port(self: Pin<&mut PortGui>, name: QString) {
        unsafe {
            self.backend_connect_external_port(name);
        }
    }

    pub fn disconnect_external_port(self: Pin<&mut PortGui>, name: QString) {
        unsafe {
            self.backend_disconnect_external_port(name);
        }
    }

    pub fn get_connections_state(self: Pin<&mut PortGui>) -> QMap_QString_QVariant {
        todo!();
    }

    pub fn get_connected_external_ports(self: Pin<&mut PortGui>) -> QList_QString {
        todo!();
    }

    pub fn try_make_connections(self: Pin<&mut PortGui>, connections: QList_QString) {
        unsafe {
            self.backend_try_make_connections(connections);
        }
    }

    pub unsafe fn set_backend(mut self: Pin<&mut PortGui>, backend: *mut QObject) {
        if backend != self.backend {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.backend = backend;
            unsafe {
                self.as_mut().backend_changed(backend);
            }
        }
    }

    pub fn set_name_hint(mut self: Pin<&mut PortGui>, name_hint: QString) {
        if name_hint != self.name_hint {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.name_hint = name_hint.clone();
            unsafe {
                self.as_mut().name_hint_changed(name_hint);
            }
        }
    }

    pub fn set_input_connectability(mut self: Pin<&mut PortGui>, input_connectability: i32) {
        if input_connectability != self.input_connectability {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.input_connectability = input_connectability;
            unsafe {
                self.as_mut()
                    .input_connectability_changed(input_connectability);
            }
        }
    }

    pub fn set_output_connectability(mut self: Pin<&mut PortGui>, output_connectability: i32) {
        if output_connectability != self.output_connectability {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.output_connectability = output_connectability;
            unsafe {
                self.as_mut()
                    .output_connectability_changed(output_connectability);
            }
        }
    }

    pub fn set_is_internal(mut self: Pin<&mut PortGui>, is_internal: bool) {
        if is_internal != self.is_internal {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.is_internal = is_internal;
            unsafe {
                self.as_mut().is_internal_changed(is_internal);
            }
        }
    }

    pub fn set_internal_port_connections(
        mut self: Pin<&mut PortGui>,
        internal_port_connections: QList_QVariant,
    ) {
        if internal_port_connections != self.internal_port_connections {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.internal_port_connections = internal_port_connections.clone();
            unsafe {
                self.as_mut()
                    .internal_port_connections_changed(internal_port_connections);
            }
        }
    }

    pub fn set_n_ringbuffer_samples(mut self: Pin<&mut PortGui>, n_ringbuffer_samples: i32) {
        if n_ringbuffer_samples != self.n_ringbuffer_samples {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.n_ringbuffer_samples = n_ringbuffer_samples;
            unsafe {
                self.as_mut()
                    .n_ringbuffer_samples_changed(n_ringbuffer_samples);
            }
        }
    }

    pub fn set_audio_gain(mut self: Pin<&mut PortGui>, audio_gain: f64) {
        if audio_gain != self.audio_gain {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.audio_gain = audio_gain;
            unsafe {
                self.as_mut().audio_gain_changed(audio_gain);
            }
        }
    }

    pub fn dummy_queue_audio_data(self: Pin<&mut PortGui>, audio_data: QList_f64) {
        unsafe {
            self.backend_dummy_queue_audio_data(audio_data);
        }
    }

    pub fn dummy_dequeue_audio_data(self: Pin<&mut PortGui>) -> QList_f64 {
        todo!();
    }

    pub fn dummy_request_data(self: Pin<&mut PortGui>, n: i32) {
        unsafe {
            self.backend_dummy_request_data(n);
        }
    }

    pub fn dummy_queue_midi_msgs(self: Pin<&mut PortGui>, midi_msgs: QList_QVariant) {
        unsafe {
            self.backend_dummy_queue_midi_msgs(midi_msgs);
        }
    }

    pub fn dummy_dequeue_midi_msgs(self: Pin<&mut PortGui>) -> QList_QVariant {
        todo!();
    }

    pub fn dummy_clear_queues(self: Pin<&mut PortGui>) {
        unsafe {
            self.backend_dummy_clear_queues();
        }
    }
}
