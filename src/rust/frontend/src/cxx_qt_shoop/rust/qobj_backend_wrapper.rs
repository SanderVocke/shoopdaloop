use common::logging::macros::*;
shoop_log_unit!("Frontend.BackendWrapper");

pub use crate::cxx_qt_shoop::qobj_backend_wrapper_bridge::BackendWrapper;
pub use crate::cxx_qt_shoop::qobj_backend_wrapper_bridge::ffi::make_unique_backend_wrapper as make_unique;

impl BackendWrapper {
    
    pub fn register_backend_object(self: Pin<&mut BackendWrapper>, _obj: QVariant) {
        println!("register_backend_object called");
    }

    pub fn unregister_backend_object(self: Pin<&mut BackendWrapper>, _obj: QVariant) {
        println!("unregister_backend_object called");
    }
    
    pub fn update_on_gui_thread(self: Pin<&mut BackendWrapper>) {
        println!("update_on_gui_thread called");
    }
    
    pub fn update_on_other_thread(self: Pin<&mut BackendWrapper>) {
        println!("update_on_other_thread called");
    }
    
    pub fn get_sample_rate(self: Pin<&mut BackendWrapper>) -> i32 {
        println!("get_sample_rate called");
        0
    }
    
    pub fn get_buffer_size(self: Pin<&mut BackendWrapper>) -> i32 {
        println!("get_buffer_size called");
        0
    }
    
    pub fn close(self: Pin<&mut BackendWrapper>) {
        println!("close called");
    }
    
    pub fn get_backend_driver_obj(self: Pin<&mut BackendWrapper>) -> QVariant {
        println!("get_backend_driver_obj called");
        QVariant::default()
    }
    
    pub fn get_backend_session_obj(self: Pin<&mut BackendWrapper>) -> QVariant {
        println!("get_backend_session_obj called");
        QVariant::default()
    }
    
    pub fn dummy_enter_controlled_mode(self: Pin<&mut BackendWrapper>) {
        println!("dummy_enter_controlled_mode called");
    }
    
    pub fn dummy_enter_automatic_mode(self: Pin<&mut BackendWrapper>) {
        println!("dummy_enter_automatic_mode called");
    }
    
    pub fn dummy_is_controlled(self: Pin<&mut BackendWrapper>) -> bool {
        println!("dummy_is_controlled called");
        false
    }
    
    pub fn dummy_request_controlled_frames(self: Pin<&mut BackendWrapper>, _n: i32) {
        println!("dummy_request_controlled_frames called");
    }
    
    pub fn dummy_n_requested_frames(self: Pin<&mut BackendWrapper>) -> i32 {
        println!("dummy_n_requested_frames called");
        0
    }
    
    pub fn dummy_run_requested_frames(self: Pin<&mut BackendWrapper>) {
        println!("dummy_run_requested_frames called");
    }
    
    pub fn dummy_add_external_mock_port(self: Pin<&mut BackendWrapper>, _name: &str, _direction: i32, _data_type: i32) {
        println!("dummy_add_external_mock_port called");
    }
    
    pub fn dummy_remove_external_mock_port(self: Pin<&mut BackendWrapper>, _name: &str) {
        println!("dummy_remove_external_mock_port called");
    }
    
    pub fn dummy_remove_all_external_mock_ports(self: Pin<&mut BackendWrapper>) {
        println!("dummy_remove_all_external_mock_ports called");
    }
    
    pub fn wait_process(self: Pin<&mut BackendWrapper>) {
        println!("wait_process called");
    }
    
    pub fn maybe_init(self: Pin<&mut BackendWrapper>) {
        println!("maybe_init called");
    }
    
    pub fn get_profiling_report(self: Pin<&mut BackendWrapper>) -> QVariant {
        println!("get_profiling_report called");
        QVariant::default()
    }
    
    pub fn backend_type_is_supported(self: Pin<&mut BackendWrapper>, _type: i32) -> bool {
        println!("backend_type_is_supported called");
        false
    }
    
    pub fn open_driver_audio_port(self: Pin<&mut BackendWrapper>, _name_hint: &str, _direction: i32, _min_n_ringbuffer_samples: i32) -> QVariant {
        println!("open_driver_audio_port called");
        QVariant::default()
    }
    
    pub fn open_driver_midi_port(self: Pin<&mut BackendWrapper>, _name_hint: &str, _direction: i32, _min_n_ringbuffer_samples: i32) -> QVariant {
        println!("open_driver_midi_port called");
        QVariant::default()
    }
    
    pub fn segfault_on_process_thread(self: Pin<&mut BackendWrapper>) {
        println!("segfault_on_process_thread called");
    }
    
    pub fn abort_on_process_thread(self: Pin<&mut BackendWrapper>) {
        println!("abort_on_process_thread called");
    }
    
    pub fn find_external_ports(self: Pin<&mut BackendWrapper>, _maybe_name_regex: &str, _port_direction: i32, _data_type: i32) -> Vec<QVariant> {
        println!("find_external_ports called");
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
