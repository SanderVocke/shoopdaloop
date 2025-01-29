use std::pin::Pin;
use common::logging::macros::*;
shoop_log_unit!("Frontend.BackendWrapper");

pub use crate::cxx_qt_shoop::qobj_backend_wrapper_bridge::*;
pub use crate::cxx_qt_shoop::qobj_backend_wrapper_bridge::ffi::*;

use cxx_qt::CxxQtType;

impl BackendWrapper {
    
    pub fn update_on_gui_thread(mut self: Pin<&mut BackendWrapper>) {
        trace!("Update on GUI thread");

        {
            let ref_self = self.as_ref();
            if !ref_self.ready() {
                trace!("update_on_gui_thread called on a BackendWrapper that is not ready");
                return;
            }
        }

        let update_data : BackendWrapperUpdateData;
        {
            let mut rust = self.as_mut().rust_mut();
            let maybe_update_data = rust.update_data.as_ref();
            if maybe_update_data.is_none() {
                return;
            }
            if rust.session.is_none() {
                trace!("update_on_gui_thread called on a BackendWrapper with no session");
                return;
            }
            update_data = *maybe_update_data.unwrap();
            rust.update_data = None;
        }

        {
            self.as_mut().set_xruns(update_data.xruns);
            self.as_mut().set_dsp_load(update_data.dsp_load);
            self.as_mut().set_last_processed(update_data.last_processed);
        }

        // Triggers other back-end objects to update as well
        self.as_mut().updated_on_gui_thread();
    }
    
    pub fn update_on_other_thread(mut self: Pin<&mut BackendWrapper>) {
        trace!("Update on back-end thread");

        let current_xruns;
        {
            let ref_self = self.as_ref();
            if !ref_self.ready() {
                trace!("update_on_other_thread called on a BackendWrapper that is not ready");
                return;
            }
            current_xruns = *self.xruns();
        }

        let driver_state;
        {
            let rust = self.as_mut().rust_mut();
            if rust.driver.is_none() {
                trace!("update_on_other_thread called on a BackendWrapper with no driver");
                return;
            }
            driver_state = rust.driver.as_ref().unwrap().get_state();
        }

        let update_data = BackendWrapperUpdateData {
            xruns: current_xruns + driver_state.xruns_since_last as i32,
            dsp_load: driver_state.dsp_load_percent,
            last_processed: driver_state.last_processed as i32,
        };

        {
            let mut rust = self.as_mut().rust_mut();
            rust.update_data = Some(update_data);
        }

        // Triggers other back-end objects to update as well
        self.as_mut().updated_on_backend_thread();
    }
    
    pub fn get_sample_rate(mut self: Pin<&mut BackendWrapper>) -> i32 {
        let mut_rust = self.as_mut().rust_mut();

        if mut_rust.driver.is_none() {
            warn!("get_sample_rate called on a BackendWrapper with no driver");
            return 0;
        }
        
        mut_rust.driver.as_ref().unwrap().get_sample_rate() as i32
    }
    
    pub fn get_buffer_size(mut self: Pin<&mut BackendWrapper>) -> i32 {
        let mut_rust = self.as_mut().rust_mut();

        if mut_rust.driver.is_none() {
            warn!("get_sample_rate called on a BackendWrapper with no driver");
            return 0;
        }
        
        mut_rust.driver.as_ref().unwrap().get_buffer_size() as i32
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
        let obj = super::make_unique_backend_wrapper();
        let classname = qobject_class_name_backend_wrapper (obj.as_ref().unwrap());
        assert_eq!(classname.unwrap(), "BackendWrapper");
    }
}
