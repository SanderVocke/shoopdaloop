use common::logging::macros::*;
shoop_log_unit!("Frontend.BackendWrapper");

pub use crate::cxx_qt_shoop::qobj_backend_wrapper_bridge::BackendWrapper;
pub use crate::cxx_qt_shoop::qobj_backend_wrapper_bridge::ffi::make_unique_backend_wrapper as make_unique;

impl BackendWrapper {
    
    pub fn register_backend_object(self: Pin<&mut BackendWrapper>, _obj: QVariant);

    pub fn unregister_backend_object(self: Pin<&mut BackendWrapper>, _obj: QVariant);
    
    pub fn update_on_gui_thread(self: Pin<&mut BackendWrapper>);
    
    pub fn update_on_other_thread(self: Pin<&mut BackendWrapper>);
    
    pub fn get_sample_rate(self: Pin<&mut BackendWrapper>) -> i32;
    
    pub fn get_buffer_size(self: Pin<&mut BackendWrapper>) -> i32;
    
    pub fn close(self: Pin<&mut BackendWrapper>);
    
    pub fn get_backend_driver_obj(self: Pin<&mut BackendWrapper>) -> QVariant;
    
    pub fn get_backend_session_obj(self: Pin<&mut BackendWrapper>) -> QVariant;
    
    pub fn dummy_enter_controlled_mode(self: Pin<&mut BackendWrapper>);
    
    pub fn dummy_enter_automatic_mode(self: Pin<&mut BackendWrapper>);
    
    pub fn dummy_is_controlled(self: Pin<&mut BackendWrapper>) -> bool;
    
    pub fn dummy_request_controlled_frames(self: Pin<&mut BackendWrapper>, _n: i32);
    
    pub fn dummy_n_requested_frames(self: Pin<&mut BackendWrapper>) -> i32;
    
    pub fn dummy_run_requested_frames(self: Pin<&mut BackendWrapper>);
    
    pub fn dummy_add_external_mock_port(self: Pin<&mut BackendWrapper>, _name: &str, _direction: i32, _data_type: i32);
    
    pub fn dummy_remove_external_mock_port(self: Pin<&mut BackendWrapper>, _name: &str);
    
    pub fn dummy_remove_all_external_mock_ports(self: Pin<&mut BackendWrapper>);
    
    pub fn wait_process(self: Pin<&mut BackendWrapper>);
    
    pub fn maybe_init(self: Pin<&mut BackendWrapper>);
    
    pub fn get_profiling_report(self: Pin<&mut BackendWrapper>) -> QVariant;
    
    pub fn backend_type_is_supported(self: Pin<&mut BackendWrapper>, _type: i32) -> bool;
    
    pub fn open_driver_audio_port(self: Pin<&mut BackendWrapper>, _name_hint: &str, _direction: i32, _min_n_ringbuffer_samples: i32) -> QVariant;
    
    pub fn open_driver_midi_port(self: Pin<&mut BackendWrapper>, _name_hint: &str, _direction: i32, _min_n_ringbuffer_samples: i32) -> QVariant;
    
    pub fn segfault_on_process_thread(self: Pin<&mut BackendWrapper>);
    
    pub fn abort_on_process_thread(self: Pin<&mut BackendWrapper>);
    
    pub fn find_external_ports(self: Pin<&mut BackendWrapper>, _maybe_name_regex: &str, _port_direction: i32, _data_type: i32) -> Vec<QVariant>;
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
