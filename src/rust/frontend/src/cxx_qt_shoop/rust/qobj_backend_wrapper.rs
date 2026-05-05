use crate::engine_update_thread;
use crate::profiling_report::profiling_report_to_qvariantmap;
use anyhow::anyhow;
use backend_bindings::*;
use cxx_qt_lib_shoop::qjsonobject::QJsonObject;
use cxx_qt_lib_shoop::qobject::{qobject_thread, AsQObject};
use cxx_qt_lib_shoop::qquickitem::{qquickitem_to_qobject_mut, AsQQuickItem};
use cxx_qt_lib_shoop::qvariant_helpers::qvariantmap_to_qvariant;
use cxx_qt_lib_shoop::{connect, connection_types};
use std::pin::Pin;
use std::sync::OnceLock;
use std::time;
use cxx_qt::QObject;

use common::logging::macros::*;
shoop_log_unit!("Frontend.BackendWrapper");

pub use crate::cxx_qt_shoop::qobj_backend_wrapper_bridge::ffi::*;
pub use crate::cxx_qt_shoop::qobj_backend_wrapper_bridge::*;

use cxx_qt::{ConnectionType, CxxQtType};

unsafe extern "C" fn register_process_thread() {
    static ONCE_LOCK: OnceLock<()> = OnceLock::new();
    ONCE_LOCK.get_or_init(|| {
        crashhandling::registered_threads::register_thread("audio");
    });
}

fn audio_driver_settings_from_qvariantmap(
    map: &QMap_QString_QVariant,
    driver_type: &AudioDriverType,
) -> Result<AudioDriverSettings, anyhow::Error> {
    let settings: AudioDriverSettings;
    match driver_type {
        AudioDriverType::Jack | AudioDriverType::JackTest => {
            let client_name_hint = map
                .get(&QString::from("client_name_hint"))
                .ok_or_else(|| anyhow!("No client name setting for driver"))?
                .value::<QString>()
                .ok_or_else(|| anyhow!("Wrong type for client name of driver"))?
                .to_string();
            let maybe_server_name = map.get(&QString::from("maybe_server_name")).map(|v| {
                v.value::<QString>()
                    .map(|s| s.to_string())
                    .ok_or_else(|| anyhow!("Wrong type for server name"))
            });
            let maybe_server_name = match maybe_server_name {
                Some(res) => Some(res?),
                None => None,
            };
            settings = AudioDriverSettings::Jack(JackAudioDriverSettings {
                client_name_hint,
                maybe_server_name,
            });
        }
        AudioDriverType::Dummy => {
            let client_name = map
                .get(&QString::from("client_name_hint"))
                .ok_or_else(|| anyhow!("No client name setting for driver"))?
                .value::<QString>()
                .ok_or_else(|| anyhow!("Wrong type for client name of driver"))?
                .to_string();
            let sample_rate = map
                .get(&QString::from("sample_rate"))
                .ok_or_else(|| anyhow!("No sample rate setting for driver"))?
                .value::<i32>()
                .ok_or_else(|| anyhow!("Wrong type for sample rate of driver"))?
                .try_into()
                .map_err(|e| anyhow!("Sample rate out of range: {}", e))?;
            let buffer_size = map
                .get(&QString::from("buffer_size"))
                .ok_or_else(|| anyhow!("No buffer size setting for driver"))?
                .value::<i32>()
                .ok_or_else(|| anyhow!("Wrong type for buffer size of driver"))?
                .try_into()
                .map_err(|e| anyhow!("Buffer size out of range: {}", e))?;
            settings = AudioDriverSettings::Dummy(DummyAudioDriverSettings {
                client_name,
                sample_rate,
                buffer_size,
            });
        }
    }

    Ok(settings)
}

impl BackendWrapper {
    pub fn init(mut self: Pin<&mut BackendWrapper>) -> Result<(), anyhow::Error> {
        let driver_type: AudioDriverType;
        let settings: AudioDriverSettings;
        let ready: bool;

        {
            let ref_self = self.as_ref();
            driver_type = AudioDriverType::try_from(*ref_self.backend_type())
                .map_err(|e| anyhow!("Invalid driver type: {}", e))?;
            let client_name_hint = ref_self.client_name_hint();
            let mut settings_map = ref_self.driver_setting_overrides().clone();
            if !settings_map.contains(&QString::from("client_name_hint")) {
                settings_map.insert(
                    QString::from("client_name_hint"),
                    QVariant::from(client_name_hint),
                );
            }
            if !settings_map.contains(&QString::from("sample_rate")) {
                settings_map.insert(QString::from("sample_rate"), QVariant::from(&48000));
            }
            if !settings_map.contains(&QString::from("buffer_size")) {
                settings_map.insert(QString::from("buffer_size"), QVariant::from(&256));
            }
            settings = audio_driver_settings_from_qvariantmap(&settings_map, &driver_type)?;
            ready = *ref_self.ready();
        }

        if ready {
            return Err(anyhow!("Already initialized"));
        }

        debug!(
            "Initializing with type {:?}, settings {:?}",
            driver_type, settings
        );

        let obj_qobject: *mut QObject;
        unsafe {
            let obj_qquickitem = self.as_mut().pin_mut_qquickitem_ptr();
            obj_qobject = qquickitem_to_qobject_mut(obj_qquickitem);
        }

        unsafe {
            let mut rust = self.as_mut().rust_mut();

            let local_driver = AudioDriver::new(driver_type, Some(register_process_thread))
                .map_err(|e| anyhow!("Failed to create driver: {}", e))?;
            local_driver
                .start(&settings)
                .map_err(|e| anyhow!("Failed to start driver: {}", e))?;

            let local_session =
                BackendSession::new().map_err(|e| anyhow!("Failed to create session: {}", e))?;
            local_session.set_audio_driver(&local_driver)?;

            rust.driver = Some(local_driver);
            rust.session = Some(local_session);
            rust.closed = false;

            rust.driver
                .as_mut()
                .ok_or_else(|| anyhow!("Driver is null"))?
                .wait_process();
            rust.driver
                .as_mut()
                .ok_or_else(|| anyhow!("Driver is null"))?
                .get_state(); // Has implicit side-effect necessary for initialization

            let engine_update_thread = crate::engine_update_thread::get_engine_update_thread();
            let engine_update_thread_obj = engine_update_thread.ref_qobject_ptr();

            connect::connect_or_report(
                &*engine_update_thread_obj,
                "update()",
                &*obj_qobject,
                "update_on_other_thread()",
                connection_types::DIRECT_CONNECTION,
            );
        }

        {
            self.as_mut().set_actual_backend_type(driver_type as i32);
            self.as_mut()
                .connect_updated_on_backend_thread(
                    |this: Pin<&mut BackendWrapper>| {
                        this.update_on_gui_thread();
                    },
                    ConnectionType::QueuedConnection,
                )
                .release();
            self.as_mut().set_ready(true);
            debug!("ready");
        }

        Ok(())
    }

    pub fn maybe_init(self: Pin<&mut BackendWrapper>) {
        let do_init: bool;
        let closed: bool;
        let ready: bool;
        let client_name_hint: QString;
        let backend_type: i32;
        {
            let ref_self = self.as_ref();
            closed = ref_self.rust().closed;
            ready = *ref_self.ready();
            client_name_hint = ref_self.client_name_hint().clone();
            backend_type = *ref_self.backend_type();
            do_init = !ready && !closed && !client_name_hint.is_empty() && backend_type != -1;
        }

        if do_init {
            debug!("Initializing");
            match self.init() {
                Ok(_) => {
                    trace!("Initialized");
                }
                Err(e) => {
                    error!("Failed to initialize: {:?}", e);
                }
            }
        } else if closed {
            trace!("Already closed");
        } else if ready {
            trace!("Already ready");
        } else {
            debug!(
                "Not ready to initialize: {:?} {:?} {:?} {:?}",
                closed, ready, client_name_hint, backend_type
            );
        }
    }

    pub fn close(mut self: Pin<&mut BackendWrapper>) {
        let closed: bool;
        {
            let ref_self = self.as_ref();
            closed = ref_self.rust().closed;
        }

        if closed {
            trace!("Already closed");
            return;
        }

        debug!("Closing");

        {
            self.as_mut().set_ready(false);
        }
    }

    pub fn update_on_gui_thread(mut self: Pin<&mut BackendWrapper>) {
        trace!("Start update on GUI thread");

        {
            let ref_self = self.as_ref();
            if !ref_self.ready() {
                trace!("update_on_gui_thread called on a BackendWrapper that is not ready");
                return;
            }
        }

        let update_data: BackendWrapperUpdateData;
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
            // Safe to unwrap because we checked is_none above, but match is cleaner or just expect
            update_data = match maybe_update_data {
                Some(d) => *d,
                None => {
                    error!("update_on_gui_thread called on a BackendWrapper with no update data");
                    return;
                }
            };
            rust.update_data = None;
        }

        {
            self.as_mut().set_xruns(update_data.xruns);
            self.as_mut().set_dsp_load(update_data.dsp_load);
            self.as_mut().set_last_processed(update_data.last_processed);
            self.as_mut()
                .set_n_audio_buffers_available(update_data.n_audio_buffers_available);
            self.as_mut()
                .set_n_audio_buffers_created(update_data.n_audio_buffers_created);
            self.as_mut().set_buffer_size(update_data.buffer_size);
            self.as_mut().set_sample_rate(update_data.sample_rate);
        }

        // Triggers other back-end objects to update as well
        self.as_mut().updated_on_gui_thread();

        trace!("End update on GUI thread");
    }

    pub fn update_on_other_thread(mut self: Pin<&mut BackendWrapper>) {
        trace!("Begin update on back-end thread");

        {
            let maybe_new_interval: Option<time::Duration>;
            {
                let mut rust_mut = self.as_mut().rust_mut();
                let now = time::Instant::now();
                if let Some(last_updated_time) = rust_mut.last_updated.as_ref() {
                    maybe_new_interval = Some(now.duration_since(*last_updated_time));
                } else {
                    maybe_new_interval = None;
                }
                rust_mut.last_updated = Some(now);
            }
            if maybe_new_interval.is_some() {
                if let Some(interval) = maybe_new_interval {
                    self.as_mut()
                        .set_last_update_interval(interval.as_secs_f32());
                }
            }
        }

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
        let session_state;
        {
            let rust = self.as_mut().rust_mut();
            if rust.driver.is_none() {
                trace!("update_on_other_thread called on a BackendWrapper with no driver");
                return;
            }
            driver_state = match rust.driver.as_ref() {
                Some(d) => d.get_state(),
                None => {
                    error!("Driver is null in update_on_other_thread");
                    return;
                }
            };
            if rust.session.is_none() {
                trace!("update_on_other_thread called on a BackendWrapper with no session");
                return;
            }
            session_state = match rust.session.as_ref() {
                Some(s) => s.get_state(),
                None => {
                    error!("Session is null in update_on_other_thread");
                    return;
                }
            };
        }

        let update_data = BackendWrapperUpdateData {
            xruns: current_xruns + driver_state.xruns_since_last as i32,
            dsp_load: driver_state.dsp_load_percent,
            last_processed: driver_state.last_processed as i32,
            n_audio_buffers_available: session_state.n_audio_buffers_available as i32,
            n_audio_buffers_created: session_state.n_audio_buffers_created as i32,
            sample_rate: driver_state.sample_rate as i32,
            buffer_size: driver_state.buffer_size as i32,
        };

        {
            let mut rust = self.as_mut().rust_mut();
            rust.update_data = Some(update_data);
        }

        // Triggers other back-end objects to update as well
        self.as_mut().updated_on_backend_thread();

        trace!("End update on back-end thread");
    }

    pub unsafe fn get_gui_thread(self: &BackendWrapper) -> *mut QThread {
        qobject_thread(self.qobject_ref()).unwrap_or_else(|e| {
            error!("Could not get GUI thread: {}", e);
            std::ptr::null_mut()
        })
    }

    pub fn get_backend_thread(self: &BackendWrapper) -> *mut QThread {
        engine_update_thread::get_engine_update_thread().thread
    }

    pub fn dummy_enter_controlled_mode(mut self: Pin<&mut BackendWrapper>) {
        let mut mut_rust = self.as_mut().rust_mut();

        if let Some(driver) = mut_rust.driver.as_mut() {
            driver.dummy_enter_controlled_mode();
            mut_rust.plotter_mode.plot(1.0, "BackendWrapper");
        } else {
            warn!("dummy_enter_controlled_mode called on a BackendWrapper with no driver");
        }
    }

    pub fn dummy_enter_automatic_mode(mut self: Pin<&mut BackendWrapper>) {
        let mut mut_rust = self.as_mut().rust_mut();

        if let Some(driver) = mut_rust.driver.as_mut() {
            driver.dummy_enter_automatic_mode();
            mut_rust.plotter_mode.plot(0.0, "BackendWrapper");
        } else {
            warn!("dummy_enter_automatic_mode called on a BackendWrapper with no driver");
        }
    }

    pub fn dummy_is_controlled(mut self: Pin<&mut BackendWrapper>) -> bool {
        let mut_rust = self.as_mut().rust_mut();

        if let Some(driver) = mut_rust.driver.as_ref() {
            driver.dummy_is_controlled()
        } else {
            warn!("dummy_is_controlled called on a BackendWrapper with no driver");
            false
        }
    }

    pub fn dummy_request_controlled_frames(mut self: Pin<&mut BackendWrapper>, n: i32) {
        trace!("dummy_request_controlled_frames {}", n);
        let mut mut_rust = self.as_mut().rust_mut();

        if let Some(driver) = mut_rust.driver.as_mut() {
            driver.dummy_request_controlled_frames(n as u32);
            mut_rust
                .plotter_samples_requested
                .plot(n as f64, "BackendWrapper");
        } else {
            warn!("dummy_request_controlled_frames called on a BackendWrapper with no driver");
        }
        trace!("dummy_request_controlled_frames done");
    }

    pub fn dummy_n_requested_frames(mut self: Pin<&mut BackendWrapper>) -> i32 {
        let mut mut_rust = self.as_mut().rust_mut();

        let result = if let Some(driver) = mut_rust.driver.as_ref() {
            driver.dummy_n_requested_frames() as i32
        } else {
            warn!("dummy_n_requested_frames called on a BackendWrapper with no driver");
            0
        };
        mut_rust
            .plotter_samples_pending
            .plot(result as f64, "BackendWrapper");
        result
    }

    pub fn dummy_run_requested_frames(mut self: Pin<&mut BackendWrapper>) {
        let mut mut_rust = self.as_mut().rust_mut();

        if let Some(driver) = mut_rust.driver.as_mut() {
            driver.dummy_run_requested_frames();
            mut_rust.plotter_samples_pending.plot(0.0, "BackendWrapper");
        } else {
            warn!("dummy_run_requested_frames called on a BackendWrapper with no driver");
        }
    }

    pub fn dummy_add_external_mock_port(
        mut self: Pin<&mut BackendWrapper>,
        name: QString,
        direction: i32,
        data_type: i32,
    ) {
        let mut mut_rust = self.as_mut().rust_mut();

        if let Some(driver) = mut_rust.driver.as_mut() {
            driver.dummy_add_external_mock_port(
                name.to_string().as_str(),
                direction as u32,
                data_type as u32,
            );
        } else {
            warn!("dummy_add_external_mock_port called on a BackendWrapper with no driver");
        }
    }

    pub fn dummy_remove_external_mock_port(mut self: Pin<&mut BackendWrapper>, name: QString) {
        let mut mut_rust = self.as_mut().rust_mut();

        if let Some(driver) = mut_rust.driver.as_mut() {
            driver.dummy_remove_external_mock_port(name.to_string().as_str());
        } else {
            warn!("dummy_remove_external_mock_port called on a BackendWrapper with no driver");
        }
    }

    pub fn dummy_remove_all_external_mock_ports(mut self: Pin<&mut BackendWrapper>) {
        let mut mut_rust = self.as_mut().rust_mut();

        if let Some(driver) = mut_rust.driver.as_mut() {
            driver.dummy_remove_all_external_mock_ports();
        } else {
            warn!("dummy_remove_all_external_mock_ports called on a BackendWrapper with no driver");
        }
    }

    pub fn wait_process(mut self: Pin<&mut BackendWrapper>) {
        let mut mut_rust = self.as_mut().rust_mut();

        if let Some(driver) = mut_rust.driver.as_mut() {
            driver.wait_process();
        }
    }

    pub fn get_profiling_report(self: Pin<&mut BackendWrapper>) -> QVariant {
        if let Some(session) = &self.session {
            let report = session.get_profiling_report();
            let report_variant = profiling_report_to_qvariantmap(&report);
            qvariantmap_to_qvariant(&report_variant).unwrap_or_else(|e| {
                error!("could not convert qvariantmap to qvariant: {}", e);
                QVariant::default()
            })
        } else {
            error!("get_profiling_report called on a BackendWrapper with no session");
            QVariant::default()
        }
    }

    pub fn open_driver_audio_port(
        mut self: Pin<&mut BackendWrapper>,
        name_hint: &str,
        direction: i32,
        min_n_ringbuffer_samples: i32,
    ) -> Result<AudioPort, anyhow::Error> {
        let mut_rust = self.as_mut().rust_mut();

        if mut_rust.session.is_none() {
            return Err(anyhow!(
                "open_driver_audio_port called on a BackendWrapper with no session"
            ));
        }
        if mut_rust.driver.is_none() {
            return Err(anyhow!(
                "open_driver_audio_port called on a BackendWrapper with no driver"
            ));
        }

        let port = backend_bindings::AudioPort::new_driver_port(
            mut_rust
                .session
                .as_ref()
                .ok_or_else(|| anyhow!("Session is null"))?,
            mut_rust
                .driver
                .as_ref()
                .ok_or_else(|| anyhow!("Driver is null"))?,
            name_hint.to_string().as_str(),
            &PortDirection::try_from(direction)
                .map_err(|e| anyhow!("Invalid port direction: {}", e))?,
            min_n_ringbuffer_samples as u32,
        )
        .map_err(|e| anyhow!("Failed to create audio port: {}", e))?;

        Ok(port)
    }

    pub fn open_driver_midi_port(
        mut self: Pin<&mut BackendWrapper>,
        name_hint: &str,
        direction: i32,
        min_n_ringbuffer_samples: i32,
    ) -> Result<MidiPort, anyhow::Error> {
        let mut_rust = self.as_mut().rust_mut();

        if mut_rust.session.is_none() {
            return Err(anyhow!(
                "open_driver_midi_port called on a BackendWrapper with no session"
            ));
        }
        if mut_rust.driver.is_none() {
            return Err(anyhow!(
                "open_driver_midi_port called on a BackendWrapper with no driver"
            ));
        }

        let port = backend_bindings::MidiPort::new_driver_port(
            mut_rust
                .session
                .as_ref()
                .ok_or_else(|| anyhow!("Session is null"))?,
            mut_rust
                .driver
                .as_ref()
                .ok_or_else(|| anyhow!("Driver is null"))?,
            name_hint.to_string().as_str(),
            &PortDirection::try_from(direction)
                .map_err(|e| anyhow!("Invalid port direction: {}", e))?,
            min_n_ringbuffer_samples as u32,
        )
        .map_err(|e| anyhow!("Failed to create midi port: {}", e))?;

        Ok(port)
    }

    pub fn open_driver_decoupled_midi_port(
        mut self: Pin<&mut BackendWrapper>,
        name_hint: &str,
        direction: i32,
    ) -> Result<DecoupledMidiPort, anyhow::Error> {
        let mut_rust = self.as_mut().rust_mut();

        if mut_rust.session.is_none() {
            return Err(anyhow!(
                "open_driver_decoupled_midi_port called on a BackendWrapper with no session"
            ));
        }
        if mut_rust.driver.is_none() {
            return Err(anyhow!(
                "open_driver_decoupled_midi_port called on a BackendWrapper with no driver"
            ));
        }

        let port = backend_bindings::DecoupledMidiPort::new_driver_port(
            mut_rust
                .driver
                .as_ref()
                .ok_or_else(|| anyhow!("Driver is null"))?,
            name_hint.to_string().as_str(),
            &PortDirection::try_from(direction)
                .map_err(|e| anyhow!("Invalid port direction: {}", e))?,
        )
        .map_err(|e| anyhow!("Failed to create decoupled midi port: {}", e))?;

        Ok(port)
    }

    pub fn segfault_on_process_thread(mut self: Pin<&mut BackendWrapper>) {
        let mut_rust = self.as_mut().rust_mut();

        if let Some(session) = mut_rust.session.as_ref() {
            session.segfault_on_process_thread();
        } else {
            warn!("segfault_on_process_thread called on a BackendWrapper with no session");
        }
    }

    pub fn abort_on_process_thread(mut self: Pin<&mut BackendWrapper>) {
        let mut_rust = self.as_mut().rust_mut();

        if let Some(session) = mut_rust.session.as_ref() {
            session.abort_on_process_thread();
        } else {
            warn!("abort_on_process_thread called on a BackendWrapper with no session");
        }
    }

    pub fn find_external_ports(
        self: Pin<&mut BackendWrapper>,
        maybe_name_regex: QString,
        port_direction: i32,
        data_type: i32,
    ) -> QList_QVariant {
        let rust = self.rust();
        let name_regex = maybe_name_regex.to_string();
        let ports = match rust.driver.as_ref() {
            Some(d) => d.find_external_ports(
                Some(name_regex.as_str()),
                port_direction as u32,
                data_type as u32,
            ),
            None => {
                error!("find_external_ports called on a BackendWrapper with no driver");
                return QList_QVariant::default();
            }
        };
        let mut descriptors = QList_QVariant::default();

        for port in ports.iter() {
            let mut qmap = QMap_QString_QVariant::default();
            qmap.insert(
                QString::from("name"),
                QVariant::from(&QString::from(&port.name)),
            );
            qmap.insert(
                QString::from("direction"),
                QVariant::from(&(port.direction as i32)),
            );
            qmap.insert(
                QString::from("data_type"),
                QVariant::from(&(port.data_type as i32)),
            );

            // TODO: indirect conversion because QMap as QVariant seems unsupported by cxx-qt
            if let Ok(json) = QJsonObject::from_variant_map(&qmap) {
                if let Ok(variant) = json.to_variant() {
                    descriptors.append(variant);
                }
            }
        }

        descriptors
    }

    pub fn create_loop(mut self: Pin<&mut BackendWrapper>, name: &str) -> Loop {
        let mut mut_rust = self.as_mut().rust_mut();
        mut_rust
            .session
            .as_mut()
            // TODO: Handle session null
            .expect("Session is null")
            .create_loop(name)
            // TODO: Handle error gracefully (return dummy?)
            .expect("Failed to create loop")
    }

    pub fn create_fx_chain(
        mut self: Pin<&mut BackendWrapper>,
        chain_type: u32,
        title: &str,
    ) -> FXChain {
        let mut mut_rust = self.as_mut().rust_mut();
        mut_rust
            .session
            .as_mut()
            // TODO: Handle session null
            .expect("Session is null")
            .create_fx_chain(
                chain_type
                    .try_into()
                    // TODO: Handle invalid chain type
                    .expect("Invalid chain type"),
                title,
            )
            // TODO: Handle error gracefully
            .expect("Failed to create fx chain")
    }
    pub fn backend_type_is_supported(self: Pin<&mut BackendWrapper>, _type: i32) -> bool {
        match AudioDriverType::try_from(_type) {
            Ok(driver_type) => driver_type_supported(driver_type),
            Err(_) => false,
        }
    }
}

pub fn register_qml_type(module_name: &str, type_name: &str) {
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    unsafe {
        register_qml_type_backend_wrapper(std::ptr::null_mut(), &mut mdl, 1, 0, &mut tp);
    }
}

#[cfg(test)]
mod tests {
    use crate::cxx_qt_shoop::qobj_backend_wrapper_bridge::ffi::*;

    #[test]
    fn test_class_name() {
        let obj = super::make_unique_backend_wrapper();
        let classname = qobject_class_name_backend_wrapper(obj.as_ref().expect("Object is null"));
        assert_eq!(
            classname.expect("Failed to get class name"),
            "BackendWrapper"
        );
    }
}
