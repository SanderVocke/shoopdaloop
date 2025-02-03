use std::pin::Pin;
use backend_bindings::*;
use common::logging::macros::*;
use crate::cxx_qt_lib_shoop::qquickitem::{AsQQuickItem, qquickitem_to_qobject_mut};
use crate::cxx_qt_lib_shoop::qobject::QObject;
use crate::cxx_qt_lib_shoop::qthread::QThread;
use crate::cxx_qt_lib_shoop::qtimer::QTimer;
use crate::cxx_qt_shoop::qobj_signature_backend_wrapper::constants;
use std::slice;
shoop_log_unit!("Frontend.BackendWrapper");

pub use crate::cxx_qt_shoop::qobj_backend_wrapper_bridge::*;
pub use crate::cxx_qt_shoop::qobj_backend_wrapper_bridge::ffi::*;

use cxx_qt::{ConnectionType, CxxQtType};

fn audio_driver_settings_from_qvariantmap(map: &QMap_QString_QVariant,
                                          driver_type : &AudioDriverType) -> AudioDriverSettings
{
    let mut settings : Option<AudioDriverSettings> = None;

    match driver_type {
        AudioDriverType::Jack => {
            let client_name_hint = map.get(&QString::from("client_name_hint"))
                 .expect("No client name setting for driver")
                 .value::<QString>()
                 .expect("Wrong type for client name of driver")
                 .to_string();
            let maybe_server_name = map.get(&QString::from("maybe_server_name"))
                 .map(|v| v.value::<QString>().expect("Wrong type for server name").to_string());
            settings = Some(AudioDriverSettings::Jack(JackAudioDriverSettings {
                client_name_hint,
                maybe_server_name,
            }));
        },
        AudioDriverType::Dummy => {
            let client_name = map.get(&QString::from("client_name_hint"))
                 .expect("No client name setting for driver")
                 .value::<QString>()
                 .expect("Wrong type for client name of driver")
                 .to_string();
            let sample_rate = map.get(&QString::from("sample_rate"))
                 .expect("No sample rate setting for driver")
                 .value::<i32>()
                 .expect("Wrong type for sample rate of driver")
                 .try_into()
                 .expect("Sample rate out of range");
            let buffer_size = map.get(&QString::from("buffer_size"))
                 .expect("No buffer size setting for driver")
                 .value::<i32>()
                 .expect("Wrong type for buffer size of driver")
                 .try_into()
                 .expect("Buffer size out of range");
            settings = Some(AudioDriverSettings::Dummy(DummyAudioDriverSettings {
                client_name,
                sample_rate,
                buffer_size,
            }));
        },
        _ => {
            error!("Unsupported driver type {:?}", driver_type);
        },
    }

    settings.expect("Unable to convert audio driver settings")
}

impl BackendWrapper {
    pub fn init(mut self: Pin<&mut BackendWrapper>) -> Result<(), anyhow::Error>{
        let driver_type : AudioDriverType;
        let settings : AudioDriverSettings;
        let ready : bool;

        {
            let ref_self = self.as_ref();
            driver_type = AudioDriverType::try_from(*ref_self.backend_type()).unwrap();
            let client_name_hint = ref_self.client_name_hint();
            let mut settings_map = ref_self.driver_setting_overrides().clone();
            if !settings_map.contains(&QString::from("client_name_hint")) {
                settings_map.insert(QString::from("client_name_hint"), QVariant::from(client_name_hint));
            }
            if !settings_map.contains(&QString::from("sample_rate")) {
                settings_map.insert(QString::from("sample_rate"), QVariant::from(&48000));
            }
            if !settings_map.contains(&QString::from("buffer_size")) {
                settings_map.insert(QString::from("buffer_size"), QVariant::from(&256));
            }
            settings = audio_driver_settings_from_qvariantmap(&settings_map, &driver_type);
            ready = *ref_self.ready();
        }

        if ready {
            return Err(anyhow::anyhow!("Already initialized"));
        }

        debug!("Initializing with type {:?}, settings {:?}",
               driver_type,
               settings);

        
        let obj_qobject : *mut QObject;
        unsafe {
               let obj_qquickitem = self.as_mut().pin_mut_qquickitem_ptr();
               obj_qobject = qquickitem_to_qobject_mut(obj_qquickitem);
        }
        
        unsafe {
            let mut rust = self.as_mut().rust_mut();

            let local_driver = AudioDriver::new(driver_type).expect("Failed to create driver");
            local_driver.start(&settings).expect("Failed to start driver");

            rust.driver = Some(local_driver);
            rust.session = Some(BackendSession::new().expect("Unable to create session"));
            rust.closed = false;

            rust.update_thread = QThread::make_raw_with_parent(obj_qobject);
            let thread_mut_ref = &mut *rust.update_thread;
            let thread_slice = slice::from_raw_parts_mut(thread_mut_ref, 1);
            let mut thread : Pin<&mut QThread> = Pin::new_unchecked(&mut thread_slice[0]);

            rust.update_timer = QTimer::make_raw();
            let timer_mut_ref = &mut *rust.update_timer;
            let timer_slice = slice::from_raw_parts_mut(timer_mut_ref, 1);
            let mut timer : Pin<&mut QTimer> = Pin::new_unchecked(&mut timer_slice[0]);
            if !timer.as_mut().move_to_thread(thread.as_mut().get_unchecked_mut())? {
                return Err(anyhow::anyhow!("Failed to move timer to thread"));
            }
            timer.as_mut().set_interval(50);
            timer.as_mut().set_single_shot(false);
            timer.as_mut().connect_timeout(obj_qobject, String::from(constants::INVOKABLE_UPDATE_ON_OTHER_THREAD))?;
            thread.as_mut().connect_started(timer.as_mut().qobject_from_ptr(), "start()".to_string())?;
            thread.as_mut().start();
        }

        {
            self.as_mut().connect_updated_on_backend_thread(
                |this: Pin<&mut BackendWrapper>| {
                    this.update_on_gui_thread();
                },
                ConnectionType::QueuedConnection)
                .release();
            self.as_mut().set_ready(true);
        }

        error!("Init not completely implemented");
        Ok(())
    }

    pub fn maybe_init(self: Pin<&mut BackendWrapper>) {
        let do_init : bool;
        let closed : bool;
        let ready : bool;
        let client_name_hint : QString;
        let backend_type : i32;
        {
            let ref_self = self.as_ref();
            closed = ref_self.rust().closed;
            ready = *ref_self.ready();
            client_name_hint = ref_self.client_name_hint().clone();
            backend_type = *ref_self.backend_type();
            do_init = !ready &&
                      !closed &&
                      !client_name_hint.is_empty() &&
                      backend_type != -1;
                      
        }

        if do_init {
            debug!("Initializing");
            match self.init() {
                Ok(_) => {
                    trace!("Initialized");
                },
                Err(e) => {
                    error!("Failed to initialize: {:?}", e);
                }
            }
        } else if closed {
            trace!("Already closed");
        } else if ready {
            trace!("Already ready");
        } else {
            debug!("Not ready to initialize: {:?} {:?} {:?} {:?}",
                   closed,
                   ready,
                   client_name_hint,
                   backend_type);
        }
    }

    pub fn close(self: Pin<&mut BackendWrapper>) {
        error!("close unimplemented")
    }
    
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
    
    pub fn dummy_enter_controlled_mode(mut self: Pin<&mut BackendWrapper>) {
        let mut_rust = self.as_mut().rust_mut();

        if mut_rust.driver.is_none() {
            warn!("dummy_enter_controlled_mode called on a BackendWrapper with no driver");
        } else {
            mut_rust.driver.as_ref().unwrap().dummy_enter_controlled_mode();
        }
    }
    
    pub fn dummy_enter_automatic_mode(mut self: Pin<&mut BackendWrapper>) {
        let mut_rust = self.as_mut().rust_mut();

        if mut_rust.driver.is_none() {
            warn!("dummy_enter_automatic_mode called on a BackendWrapper with no driver");
        } else {
            mut_rust.driver.as_ref().unwrap().dummy_enter_automatic_mode();
        }
    }
    
    pub fn dummy_is_controlled(mut self: Pin<&mut BackendWrapper>) -> bool {
        let mut_rust = self.as_mut().rust_mut();

        if mut_rust.driver.is_none() {
            warn!("dummy_is_controlled called on a BackendWrapper with no driver");
            false
        } else {
            mut_rust.driver.as_ref().unwrap().dummy_is_controlled()
        }
    }
    
    pub fn dummy_request_controlled_frames(mut self: Pin<&mut BackendWrapper>, n: i32) {
        let mut_rust = self.as_mut().rust_mut();

        if mut_rust.driver.is_none() {
            warn!("dummy_request_controlled_frames called on a BackendWrapper with no driver");
        } else {
            mut_rust.driver.as_ref().unwrap().dummy_request_controlled_frames(n as u32);
        }
    }
    
    pub fn dummy_n_requested_frames(mut self: Pin<&mut BackendWrapper>) -> i32 {
        let mut_rust = self.as_mut().rust_mut();

        if mut_rust.driver.is_none() {
            warn!("dummy_n_requested_frames called on a BackendWrapper with no driver");
            0
        } else {
            mut_rust.driver.as_ref().unwrap().dummy_n_requested_frames() as i32
        }
    }
    
    pub fn dummy_run_requested_frames(mut self: Pin<&mut BackendWrapper>) {
        let mut_rust = self.as_mut().rust_mut();

        if mut_rust.driver.is_none() {
            warn!("dummy_run_requested_frames called on a BackendWrapper with no driver");
        } else {
            mut_rust.driver.as_ref().unwrap().dummy_run_requested_frames();
        }
    }
    
    pub fn dummy_add_external_mock_port(mut self: Pin<&mut BackendWrapper>, name: QString, direction: i32, data_type: i32) {
        let mut_rust = self.as_mut().rust_mut();

        if mut_rust.driver.is_none() {
            warn!("dummy_add_external_mock_port called on a BackendWrapper with no driver");
        } else {
            mut_rust.driver.as_ref().unwrap().dummy_add_external_mock_port
                (name.to_string().as_str(),
                 direction as u32,
                 data_type as u32);
        }
    }
    
    pub fn dummy_remove_external_mock_port(mut self: Pin<&mut BackendWrapper>, name: QString) {
        let mut_rust = self.as_mut().rust_mut();

        if mut_rust.driver.is_none() {
            warn!("dummy_remove_external_mock_port called on a BackendWrapper with no driver");
        } else {
            mut_rust.driver.as_ref().unwrap().dummy_remove_external_mock_port(name.to_string().as_str());
        }
    }
    
    pub fn dummy_remove_all_external_mock_ports(mut self: Pin<&mut BackendWrapper>) {
        let mut_rust = self.as_mut().rust_mut();

        if mut_rust.driver.is_none() {
            warn!("dummy_remove_all_external_mock_ports called on a BackendWrapper with no driver");
        } else {
            mut_rust.driver.as_ref().unwrap().dummy_remove_all_external_mock_ports();
        }
    }
    
    pub fn wait_process(mut self: Pin<&mut BackendWrapper>) {
        let mut_rust = self.as_mut().rust_mut();

        if mut_rust.driver.is_some() {
            mut_rust.driver.as_ref().unwrap().wait_process();
        }
    }
    
    pub fn get_profiling_report(self: Pin<&mut BackendWrapper>) -> QVariant {
        // TODO
        error!("get_profiling_report unimplemented");
        QVariant::default()
    }
    
    pub fn backend_type_is_supported(self: Pin<&mut BackendWrapper>, t: i32) -> bool {
        driver_type_supported(AudioDriverType::try_from(t).unwrap())
    }

    pub fn open_driver_audio_port(mut self: Pin<&mut BackendWrapper>, name_hint: &str, direction: i32, min_n_ringbuffer_samples: i32) -> Result<AudioPort, anyhow::Error> {
        let mut_rust = self.as_mut().rust_mut();

        if mut_rust.session.is_none() {
            return Err(anyhow::anyhow!("open_driver_audio_port called on a BackendWrapper with no session"));
        }
        if mut_rust.driver.is_none() {
            return Err(anyhow::anyhow!("open_driver_audio_port called on a BackendWrapper with no driver"));
        }

        let port = backend_bindings::AudioPort::new_driver_port
           (&mut_rust.session.as_ref().unwrap(),
            &mut_rust.driver.as_ref().unwrap(),
            name_hint.to_string().as_str(),
            &PortDirection::try_from(direction).unwrap(),
            min_n_ringbuffer_samples as u32)
            .expect("Failed to create audio port");
        
        Ok(port)
    }

    pub fn open_driver_midi_port(mut self: Pin<&mut BackendWrapper>, name_hint: &str, direction: i32, min_n_ringbuffer_samples: i32) -> Result<MidiPort, anyhow::Error> {
        let mut_rust = self.as_mut().rust_mut();

        if mut_rust.session.is_none() {
            return Err(anyhow::anyhow!("open_driver_midi_port called on a BackendWrapper with no session"));
        }
        if mut_rust.driver.is_none() {
            return Err(anyhow::anyhow!("open_driver_midi_port called on a BackendWrapper with no driver"));
        }

        let port = backend_bindings::MidiPort::new_driver_port
           (&mut_rust.session.as_ref().unwrap(),
            &mut_rust.driver.as_ref().unwrap(),
            name_hint.to_string().as_str(),
            &PortDirection::try_from(direction).unwrap(),
            min_n_ringbuffer_samples as u32)
            .expect("Failed to create midi port");
        
        Ok(port)
    }
    
    pub fn segfault_on_process_thread(mut self: Pin<&mut BackendWrapper>) {
        let mut_rust = self.as_mut().rust_mut();

        if mut_rust.session.is_none() {
            warn!("segfault_on_process_thread called on a BackendWrapper with no session");
        } else {
            mut_rust.session.as_ref().unwrap().segfault_on_process_thread();
        }
    }
    
    pub fn abort_on_process_thread(mut self: Pin<&mut BackendWrapper>) {
        let mut_rust = self.as_mut().rust_mut();

        if mut_rust.session.is_none() {
            warn!("abort_on_process_thread called on a BackendWrapper with no session");
        } else {
            mut_rust.session.as_ref().unwrap().abort_on_process_thread();
        }
    }
    
    pub fn find_external_ports(self: Pin<&mut BackendWrapper>, _maybe_name_regex: QString, _port_direction: i32, _data_type: i32) -> QList_QVariant {
        error!("find_external_ports unimplemented");
        QList_QVariant::default()
    }

    pub fn create_loop(mut self: Pin<&mut BackendWrapper>) -> Loop {
        let mut mut_rust = self.as_mut().rust_mut();
        mut_rust.session.as_mut().unwrap().create_loop().unwrap()
    }
}

pub fn register_qml_type(module_name: &str, type_name: &str) {
    let obj = make_unique_backend_wrapper();
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    register_qml_type_backend_wrapper(obj.as_ref().unwrap(), &mut mdl, 1, 0, &mut tp);
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
