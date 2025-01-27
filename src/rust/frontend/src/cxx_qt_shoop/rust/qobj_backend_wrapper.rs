use common::logging::macros::*;
shoop_log_unit!("Frontend.BackendWrapper");

pub use crate::cxx_qt_shoop::qobj_backend_wrapper_bridge::BackendWrapper;
pub use crate::cxx_qt_shoop::qobj_backend_wrapper_bridge::ffi::make_unique_backend_wrapper as make_unique;

impl BackendWrapper {
    pub fn register_backend_object(&mut self, _obj: QVariant) {}

    pub fn unregister_backend_object(&mut self, _obj: QVariant) {}

    pub fn update_on_gui_thread(&mut self) {}

    pub fn update_on_other_thread(&mut self) {}

    pub fn get_sample_rate(&mut self) -> i32 {
        0
    }

    pub fn get_buffer_size(&mut self) -> i32 {
        0
    }

    pub fn close(&mut self) {}

    pub fn get_backend_driver_obj(&mut self) -> QVariant {
        QVariant::default()
    }

    pub fn get_backend_session_obj(&mut self) -> QVariant {
        QVariant::default()
    }

    pub fn dummy_enter_controlled_mode(&mut self) {}

    pub fn dummy_enter_automatic_mode(&mut self) {}

    pub fn dummy_is_controlled(&mut self) -> bool {
        false
    }

    pub fn dummy_request_controlled_frames(&mut self, _n: i32) {}

    pub fn dummy_n_requested_frames(&mut self) -> i32 {
        0
    }

    pub fn dummy_run_requested_frames(&mut self) {}

    pub fn dummy_add_external_mock_port(&mut self, _name: &str, _direction: i32, _data_type: i32) {}

    pub fn dummy_remove_external_mock_port(&mut self, _name: &str) {}

    pub fn dummy_remove_all_external_mock_ports(&mut self) {}

    pub fn wait_process(&mut self) {}

    pub fn maybe_init(&mut self) {}

    pub fn get_profiling_report(&mut self) -> QVariant {
        QVariant::default()
    }

    pub fn backend_type_is_supported(&mut self, _type: i32) -> bool {
        false
    }

    pub fn open_driver_audio_port(&mut self, _name_hint: &str, _direction: i32, _min_n_ringbuffer_samples: i32) -> QVariant {
        QVariant::default()
    }

    pub fn open_driver_midi_port(&mut self, _name_hint: &str, _direction: i32, _min_n_ringbuffer_samples: i32) -> QVariant {
        QVariant::default()
    }

    pub fn segfault_on_process_thread(&mut self) {}

    pub fn abort_on_process_thread(&mut self) {}

    pub fn find_external_ports(&mut self, _maybe_name_regex: &str, _port_direction: i32, _data_type: i32) -> Vec<QVariant> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::cxx_qt_shoop::qobj_backend_wrapper_bridge::ffi::*;

    #[test]
    fn test_class_name() {
        let obj = super::make_unique();
        let classname = qobject_class_name_backend_wrapper (obj.as_ref().unwrap());
        assert_eq!(classname.unwrap(), "BackendWrapper");
    }
}
