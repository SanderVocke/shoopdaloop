use common::logging::macros::*;
shoop_log_unit!("Frontend.AutoConnect");

pub use crate::cxx_qt_shoop::qobj_autoconnect_bridge::AutoConnect;
pub use crate::cxx_qt_shoop::qobj_autoconnect_bridge::constants::*;
pub use crate::cxx_qt_shoop::qobj_autoconnect_bridge::ffi::make_raw_autoconnect as make_raw;
pub use crate::cxx_qt_shoop::qobj_autoconnect_bridge::ffi::make_unique_autoconnect as make_unique;
use crate::cxx_qt_shoop::qobj_autoconnect_bridge::*;
use crate::cxx_qt_shoop::qobj_autoconnect_bridge::ffi::*;

use crate::cxx_qt_shoop::type_external_port_descriptor::ExternalPortDescriptor;
use backend_bindings::{PortDataType, PortDirection};
use std::pin::Pin;
use crate::cxx_qt_lib_shoop::qquickitem::{AsQQuickItem, IsQQuickItem, qquickitem_to_qobject_mut};
use crate::cxx_qt_lib_shoop::qobject::AsQObject;
use crate::cxx_qt_lib_shoop::{qobject, qtimer};
use crate::cxx_qt_shoop::{fn_qlist_helpers, fn_qvariantmap_helpers, qobj_signature_backend_wrapper, qobj_signature_port};
use regex::Regex;
use anyhow::Context;

use super::qobj_find_parent_item;
use std::slice;

impl AutoConnect {
    pub unsafe fn initialize_impl_with_result(mut self : Pin<&mut AutoConnect>) -> Result<(), anyhow::Error> {
        debug!("Initializing");
        {
            self.as_mut().on_parent_changed(|o,_| {
                let mut rust : Pin<&mut AutoConnectRust> = o.cxx_qt_ffi_rust_mut();
                rust.find_backend_wrapper.as_mut().unwrap().rescan();
            }).release();
            self.as_mut().on_internal_port_changed(|o| {
                debug!("internal_port -> {:?}", o.internal_port());
                o.update();
            }).release();
            self.as_mut().on_connect_to_port_regex_changed(|o| {
                debug!("connect_to_port_regex -> {:?}", o.connect_to_port_regex());
                o.update();
            }).release();
        }

        let obj_qquickitem = self.as_mut().pin_mut_qquickitem_ptr();
        let obj_ptr = self.get_unchecked_mut() as *mut AutoConnect;
        let obj_qobject = qquickitem_to_qobject_mut(obj_qquickitem);
        let mut pinned = Pin::new_unchecked(&mut *obj_ptr);
        let mut rust : Pin<&mut AutoConnectRust> = pinned.as_mut().cxx_qt_ffi_rust_mut();

        {
            let mut rust_finder_access = rust.as_mut();
            let mut finder = rust_finder_access.find_backend_wrapper.as_mut().unwrap();
            let finder_qquickitem = finder.as_mut().pin_mut_qquickitem_ptr();

            finder.as_mut().set_parent_item(obj_qquickitem);
            connect_to_autoconnect(
                finder_qquickitem,
                String::from(qobj_find_parent_item::SIGNAL_FOUND_ITEM_WITH_TRUE_CHECKED_PROPERTY_CHANGED),
                obj_ptr,
                String::from(constants::INVOKABLE_UPDATE))?;
            finder.as_mut().rescan();
        }

        rust.timer = qtimer::make_raw_with_parent(obj_qobject);
        let timer_mut_ref = &mut *rust.timer;
        let timer_slice = slice::from_raw_parts_mut(timer_mut_ref, 1);
        let mut timer : Pin<&mut QTimer> = Pin::new_unchecked(&mut timer_slice[0]);
        timer.as_mut().set_interval(1000);
        timer.as_mut().set_single_shot(false);
        timer.as_mut().connect_timeout(obj_qobject, String::from(constants::INVOKABLE_UPDATE))?;
        timer.as_mut().start();

        Ok(())
    }

    pub fn update_with_result(mut self: Pin<&mut Self>) -> Result<(), anyhow::Error> {
        debug!("Updating");
        {
            if self.as_mut().is_closed().to_owned() {
                debug!("Closed, skipping update");
                return Ok(());
            }
        }

        let internal_port : *mut QObject;
        {
            internal_port = self.as_mut().internal_port().clone();
            if internal_port.is_null() {
                debug!("Internal port not set, skipping update");
                return Ok(());
            }
        }

        let mut rust = self.as_mut().cxx_qt_ffi_rust_mut();
        let regex_str = format!("^{}$", rust.connect_to_port_regex.to_string());
        let finder = rust.find_backend_wrapper.as_mut().unwrap();
        let regex = Regex::new(regex_str.as_str())
                           .with_context(|| "Invalid regex")?;
        let backend = finder.found_item_with_true_checked_property();
        if backend.is_null() {
            debug!("Backend not present or ready, skipping update");
            return Ok(());
        }

        unsafe {
            let backend_qobj = backend.as_mut().unwrap().mut_qobject_ptr();
            let q_connections_state : QMap_QString_QVariant =
                invoke_with_return_variantmap(
                    internal_port,
                    String::from(qobj_signature_port::constants::INVOKABLE_DETERMINE_CONNECTIONS_STATE))?;
            let connections_state = fn_qvariantmap_helpers::try_as_hashmap_convertto::<bool>(&q_connections_state)?;
            debug!("Got connections state: {:?}", connections_state);

            let my_data_type : PortDataType = qobject::qobject_property_int(
                &*internal_port,
                String::from(qobj_signature_port::constants::PROP_DATA_TYPE))
                .map_err(|err| anyhow::anyhow!(err))
                .and_then(|o| PortDataType::try_from(o))?;
            let my_direction : PortDirection = qobject::qobject_property_int(
                &*internal_port,
                String::from(qobj_signature_port::constants::PROP_DIRECTION))
                .map_err(|err| anyhow::anyhow!(err))
                .and_then(|o| PortDirection::try_from(o))?;
            let my_name : String = qobject::qobject_property_string(
                &*internal_port,
                String::from(qobj_signature_port::constants::PROP_NAME))
                .and_then(|o| Ok(o.to_string()))?;
            let my_port_initialized : bool = qobject::qobject_property_bool(
                &*internal_port,
                String::from(qobj_signature_port::constants::PROP_INITIALIZED))?;
            let q_external_ports : QList_QVariant =
                invoke_find_external_ports(
                    backend_qobj,
                    String::from(qobj_signature_backend_wrapper::constants::INVOKABLE_FIND_EXTERNAL_PORTS),
                    self.as_mut().connect_to_port_regex().clone(),
                    if my_direction == PortDirection::Input { PortDirection::Output as i32 }
                                                       else  { PortDirection::Input as i32 },
                    my_data_type as i32)?;
            let external_candidates = fn_qlist_helpers::try_as_list_into::<ExternalPortDescriptor>(&q_external_ports)?;
            debug!("Queried for external ports: {:?}", external_candidates);

            for candidate in external_candidates {
                let candidate : &ExternalPortDescriptor = &candidate;
                let maybe_entry : Option<bool> = connections_state.get(&candidate.name).map(|o| o.clone());
                let is_connected : bool = maybe_entry.unwrap_or(false);
                let is_match : bool =
                    candidate.data_type == my_data_type &&
                    candidate.direction != my_direction &&
                    regex.is_match(&candidate.name);

                if !is_connected &&
                    is_match &&
                    my_port_initialized
                {
                    debug!("{} auto-connecting to {}", my_name, candidate.name);
                    let result : Result<bool, _> = invoke_connect_external_port(
                        internal_port,
                        String::from(qobj_signature_port::constants::INVOKABLE_BOOL_CONNECT_EXTERNAL_PORT),
                        QString::from(&candidate.name));
                    let result : Result<(), _> = match result {
                        Ok(o) => if o { Ok (()) } else { Err(anyhow::anyhow!("Connection failed")) },
                        Err(e) => Err(e.into()),
                    };
                    match result {
                        Ok(()) => {
                            info!("{} auto-connected to {}", my_name, candidate.name);
                            self.as_mut().connected();
                        },
                        Err(e) => {
                            error!("{} failed to auto-connect to {}: {:?}", my_name, candidate.name, e);
                        }
                    }
                } else if is_connected {
                    debug!("{} already connected to {}", my_name, candidate.name);
                } else if !my_port_initialized {
                    debug!("internal port {} not yet initialized", my_name);
                    self.as_mut().only_external_found();
                } else if !is_match {
                    debug!("found port is not a match");
                }
            }

            Ok(())
        }
    }

    pub fn update(self: Pin<&mut Self>) {
        match self.update_with_result() {
            Ok(()) => (),
            Err(e) => { error!("Failed to update: {:?}", e); }
        }
    }

    pub fn initialize_impl(self : Pin<&mut AutoConnect>) {
        unsafe {
            match self.initialize_impl_with_result() {
                Ok(()) => (),
                Err(e) => { error!("Failed to initialize: {:?}", e); }
            }
        }
    }
}

pub fn register_qml_type(module_name : &str, type_name : &str) {
    let obj = make_unique_autoconnect();
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    register_qml_type_autoconnect(obj.as_ref().unwrap(), &mut mdl, 1, 0, &mut tp);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cxx_qt_lib_shoop;
    use crate::cxx_qt_lib_shoop::qsignalspy::QSignalSpy;
    use crate::cxx_qt_shoop::test::qobj_test_backend_wrapper;
    use crate::cxx_qt_shoop::test::qobj_test_port;
    #[test]
    fn test_class_name() {
        let obj = make_unique_autoconnect();
        let classname = qobject_class_name_autoconnect (obj.as_ref().unwrap());
        assert!(classname.is_ok());
        assert_eq!(classname.unwrap(), "AutoConnect");
    }

    #[test]
    fn test_connect() {
        unsafe {
            // Create the port to connect to
            let mut port = qobj_test_port::make_unique();
            let port_ptr = port.as_mut().unwrap().pin_mut_qobject_ptr();
            port.pin_mut().set_connections_state_from_json(r#"
                {
                    "port_1": false
                }
                "#).expect("Could not set connections state");
            port.pin_mut().set_direction(PortDirection::Input as i32);
            port.pin_mut().set_data_type(PortDataType::Audio as i32);
            port.pin_mut().set_initialized(true);
            port.pin_mut().set_name(QString::from("my_port"));

            // Create the fake backend
            let mut backend = qobj_test_backend_wrapper::make_unique();
            backend.as_mut().unwrap().set_initialized(true);
            {
                let mut backend_rust = backend.pin_mut().cxx_qt_ffi_rust_mut();
                backend_rust.mock_external_ports.push(ExternalPortDescriptor {
                    name: String::from("port_1"),
                    direction: PortDirection::Output,
                    data_type: PortDataType::Audio
                });
            }
            let backend_ptr = backend.as_mut().unwrap().pin_mut_qquickitem_ptr();

            // Instantiate the connector
            let mut obj = make_unique_autoconnect();

            // A spy to check that the connection is made
            let autoconnect_connected_spy : *mut QSignalSpy;
            let port_connection_made_spy : *mut QSignalSpy;
            {
                let obj_ptr = obj.as_ref().unwrap().ref_qobject_ptr();
                autoconnect_connected_spy = cxx_qt_lib_shoop::qsignalspy::make_raw(obj_ptr, String::from(constants::SIGNAL_CONNECTED))
                      .expect("Couldn't create spy");
                port_connection_made_spy = cxx_qt_lib_shoop::qsignalspy::make_raw(port_ptr, String::from(qobj_test_port::SIGNAL_EXTERNAL_CONNECTION_MADE))
                      .expect("Couldn't create spy");
            }

            obj.as_mut().unwrap().set_connect_to_port_regex(QString::from("port_1"));
            obj.as_mut().unwrap().set_internal_port(port_ptr);
            obj.as_mut().unwrap().set_parent_item(backend_ptr);

            assert_eq!(autoconnect_connected_spy.as_ref().unwrap().count().expect("Could not get count"), 1);
            assert_eq!(port_connection_made_spy.as_ref().unwrap().count().expect("Could not get count"), 1);
        }
    }

    #[test]
    fn test_dont_connect_name() {
        unsafe {
            // Create the port to connect to
            let mut port = qobj_test_port::make_unique();
            let port_ptr = port.as_mut().unwrap().pin_mut_qobject_ptr();
            port.pin_mut().set_connections_state_from_json(r#"
                {
                    "not_port_1": false
                }
                "#).expect("Could not set connections state");
            port.pin_mut().set_direction(PortDirection::Input as i32);
            port.pin_mut().set_data_type(PortDataType::Audio as i32);
            port.pin_mut().set_initialized(true);
            port.pin_mut().set_name(QString::from("my_port"));

            // Create the fake backend
            let mut backend = qobj_test_backend_wrapper::make_unique();
            backend.as_mut().unwrap().set_initialized(true);
            {
                let mut backend_rust = backend.pin_mut().cxx_qt_ffi_rust_mut();
                backend_rust.mock_external_ports.push(ExternalPortDescriptor {
                    name: String::from("not_port_1"),
                    direction: PortDirection::Output,
                    data_type: PortDataType::Audio
                });
            }
            let backend_ptr = backend.as_mut().unwrap().pin_mut_qquickitem_ptr();

            // Instantiate the connector
            let mut obj = make_unique_autoconnect();

            // A spy to check that the connection is made
            let autoconnect_connected_spy : *mut QSignalSpy;
            let port_connection_made_spy : *mut QSignalSpy;
            {
                let obj_ptr = obj.as_ref().unwrap().ref_qobject_ptr();
                autoconnect_connected_spy = cxx_qt_lib_shoop::qsignalspy::make_raw(obj_ptr, String::from(constants::SIGNAL_CONNECTED))
                      .expect("Couldn't create spy");
                port_connection_made_spy = cxx_qt_lib_shoop::qsignalspy::make_raw(port_ptr, String::from(qobj_test_port::SIGNAL_EXTERNAL_CONNECTION_MADE))
                      .expect("Couldn't create spy");
            }

            obj.as_mut().unwrap().set_connect_to_port_regex(QString::from("port_1"));
            obj.as_mut().unwrap().set_internal_port(port_ptr);
            obj.as_mut().unwrap().set_parent_item(backend_ptr);

            assert_eq!(autoconnect_connected_spy.as_ref().unwrap().count().expect("Could not get count"), 0);
            assert_eq!(port_connection_made_spy.as_ref().unwrap().count().expect("Could not get count"), 0);
        }
    }

    #[test]
    fn test_dont_connect_datatype() {
        unsafe {
            // Create the port to connect to
            let mut port = qobj_test_port::make_unique();
            let port_ptr = port.as_mut().unwrap().pin_mut_qobject_ptr();
            port.pin_mut().set_connections_state_from_json(r#"
                {
                    "port_1": false
                }
                "#).expect("Could not set connections state");
            port.pin_mut().set_direction(PortDirection::Input as i32);
            port.pin_mut().set_data_type(PortDataType::Midi as i32);
            port.pin_mut().set_initialized(true);
            port.pin_mut().set_name(QString::from("my_port"));

            // Create the fake backend
            let mut backend = qobj_test_backend_wrapper::make_unique();
            backend.as_mut().unwrap().set_initialized(true);
            {
                let mut backend_rust = backend.pin_mut().cxx_qt_ffi_rust_mut();
                backend_rust.mock_external_ports.push(ExternalPortDescriptor {
                    name: String::from("port_1"),
                    direction: PortDirection::Output,
                    data_type: PortDataType::Audio
                });
            }
            let backend_ptr = backend.as_mut().unwrap().pin_mut_qquickitem_ptr();

            // Instantiate the connector
            let mut obj = make_unique_autoconnect();

            // A spy to check that the connection is made
            let autoconnect_connected_spy : *mut QSignalSpy;
            let port_connection_made_spy : *mut QSignalSpy;
            {
                let obj_ptr = obj.as_ref().unwrap().ref_qobject_ptr();
                autoconnect_connected_spy = cxx_qt_lib_shoop::qsignalspy::make_raw(obj_ptr, String::from(constants::SIGNAL_CONNECTED))
                      .expect("Couldn't create spy");
                port_connection_made_spy = cxx_qt_lib_shoop::qsignalspy::make_raw(port_ptr, String::from(qobj_test_port::SIGNAL_EXTERNAL_CONNECTION_MADE))
                      .expect("Couldn't create spy");
            }

            obj.as_mut().unwrap().set_connect_to_port_regex(QString::from("port_1"));
            obj.as_mut().unwrap().set_internal_port(port_ptr);
            obj.as_mut().unwrap().set_parent_item(backend_ptr);

            assert_eq!(autoconnect_connected_spy.as_ref().unwrap().count().expect("Could not get count"), 0);
            assert_eq!(port_connection_made_spy.as_ref().unwrap().count().expect("Could not get count"), 0);
        }
    }

    #[test]
    fn test_dont_connect_direction() {
        unsafe {
            // Create the port to connect to
            let mut port = qobj_test_port::make_unique();
            let port_ptr = port.as_mut().unwrap().pin_mut_qobject_ptr();
            port.pin_mut().set_connections_state_from_json(r#"
                {
                    "port_1": false
                }
                "#).expect("Could not set connections state");
            port.pin_mut().set_direction(PortDirection::Output as i32);
            port.pin_mut().set_data_type(PortDataType::Audio as i32);
            port.pin_mut().set_initialized(true);
            port.pin_mut().set_name(QString::from("my_port"));

            // Create the fake backend
            let mut backend = qobj_test_backend_wrapper::make_unique();
            backend.as_mut().unwrap().set_initialized(true);
            {
                let mut backend_rust = backend.pin_mut().cxx_qt_ffi_rust_mut();
                backend_rust.mock_external_ports.push(ExternalPortDescriptor {
                    name: String::from("port_1"),
                    direction: PortDirection::Output,
                    data_type: PortDataType::Audio
                });
            }
            let backend_ptr = backend.as_mut().unwrap().pin_mut_qquickitem_ptr();

            // Instantiate the connector
            let mut obj = make_unique_autoconnect();

            // A spy to check that the connection is made
            let autoconnect_connected_spy : *mut QSignalSpy;
            let port_connection_made_spy : *mut QSignalSpy;
            {
                let obj_ptr = obj.as_ref().unwrap().ref_qobject_ptr();
                autoconnect_connected_spy = cxx_qt_lib_shoop::qsignalspy::make_raw(obj_ptr, String::from(constants::SIGNAL_CONNECTED))
                      .expect("Couldn't create spy");
                port_connection_made_spy = cxx_qt_lib_shoop::qsignalspy::make_raw(port_ptr, String::from(qobj_test_port::SIGNAL_EXTERNAL_CONNECTION_MADE))
                      .expect("Couldn't create spy");
            }

            obj.as_mut().unwrap().set_connect_to_port_regex(QString::from("port_1"));
            obj.as_mut().unwrap().set_internal_port(port_ptr);
            obj.as_mut().unwrap().set_parent_item(backend_ptr);

            assert_eq!(autoconnect_connected_spy.as_ref().unwrap().count().expect("Could not get count"), 0);
            assert_eq!(port_connection_made_spy.as_ref().unwrap().count().expect("Could not get count"), 0);
        }
    }

    #[test]
    fn test_connect_after_rescan() {
        unsafe {
            // Create the port to connect to
            let mut port = qobj_test_port::make_unique();
            let port_ptr = port.as_mut().unwrap().pin_mut_qobject_ptr();
            port.pin_mut().set_connections_state_from_json(r#"
                {}
                "#).expect("Could not set connections state");
            port.pin_mut().set_direction(PortDirection::Input as i32);
            port.pin_mut().set_data_type(PortDataType::Audio as i32);
            port.pin_mut().set_initialized(true);
            port.pin_mut().set_name(QString::from("my_port"));

            // Create the fake backend
            let mut backend = qobj_test_backend_wrapper::make_unique();
            backend.as_mut().unwrap().set_initialized(true);
            let backend_ptr = backend.as_mut().unwrap().pin_mut_qquickitem_ptr();

            // Instantiate the connector
            let mut obj = make_unique_autoconnect();

            // A spy to check that the connection is made
            let autoconnect_connected_spy : *mut QSignalSpy;
            let port_connection_made_spy : *mut QSignalSpy;
            {
                let obj_ptr = obj.as_ref().unwrap().ref_qobject_ptr();
                autoconnect_connected_spy = cxx_qt_lib_shoop::qsignalspy::make_raw(obj_ptr, String::from(constants::SIGNAL_CONNECTED))
                      .expect("Couldn't create spy");
                port_connection_made_spy = cxx_qt_lib_shoop::qsignalspy::make_raw(port_ptr, String::from(qobj_test_port::SIGNAL_EXTERNAL_CONNECTION_MADE))
                      .expect("Couldn't create spy");
            }

            obj.as_mut().unwrap().set_connect_to_port_regex(QString::from("port_1"));
            obj.as_mut().unwrap().set_internal_port(port_ptr);
            obj.as_mut().unwrap().set_parent_item(backend_ptr);

            assert_eq!(autoconnect_connected_spy.as_ref().unwrap().count().expect("Could not get count"), 0);
            assert_eq!(port_connection_made_spy.as_ref().unwrap().count().expect("Could not get count"), 0);
            {
                let mut backend_rust = backend.pin_mut().cxx_qt_ffi_rust_mut();
                backend_rust.mock_external_ports.push(ExternalPortDescriptor {
                    name: String::from("port_1"),
                    direction: PortDirection::Output,
                    data_type: PortDataType::Audio
                });
            }

            assert_eq!(autoconnect_connected_spy.as_ref().unwrap().count().expect("Could not get count"), 0);
            assert_eq!(port_connection_made_spy.as_ref().unwrap().count().expect("Could not get count"), 0);

            obj.as_mut().unwrap().update();

            assert_eq!(autoconnect_connected_spy.as_ref().unwrap().count().expect("Could not get count"), 1);
            assert_eq!(port_connection_made_spy.as_ref().unwrap().count().expect("Could not get count"), 1);
        }
    }
}