use crate::cxx_qt_shoop::qobj_autoconnect_bridge::ffi::make_unique_autoconnect;
use crate::cxx_qt_shoop::qobj_midi_control_port_bridge::ffi::*;
use crate::cxx_qt_shoop::qobj_midi_control_port_bridge::MidiControlPort;
use crate::cxx_qt_shoop::qobj_midi_control_port_bridge::MidiControlPortRust;
use backend_bindings::PortDirection;
use common::logging::macros::*;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QMap;
use cxx_qt_lib_shoop::connect::connect_or_report;
use cxx_qt_lib_shoop::connection_types;
use cxx_qt_lib_shoop::invokable::invoke;
use cxx_qt_lib_shoop::qobject::AsQObject;
use midi_processing::channel;
use midi_processing::is_cc;
use midi_processing::is_note_off;
use midi_processing::is_note_on;
use midi_processing::note;
use std::pin::Pin;
shoop_log_unit!("Frontend.MidiControlPort");

impl MidiControlPort {
    pub fn initialize_impl(self: Pin<&mut MidiControlPort>) {
        debug!("Initializing");
    }

    pub fn get_connections_state(mut self: Pin<&mut MidiControlPort>) -> QMap_QString_QVariant {
        if let Some(backend_port) = self.as_mut().backend_port_wrapper.as_ref() {
            let state = backend_port.get_connections_state();
            let mut rval: QMap_QString_QVariant = QMap::default();
            for (name, connected) in state {
                rval.insert(QString::from(&name), QVariant::from(&connected));
            }
            rval
        } else {
            trace!(
                "Attempted to get connections state of uninitialized MIDI control port {:?}",
                self.name_hint
            );
            QMap::default()
        }
    }

    pub fn connect_external_port(mut self: Pin<&mut MidiControlPort>, name: QString) {
        if let Some(backend_port) = self.as_mut().backend_port_wrapper.as_ref() {
            debug!("Connecting to external port {:?}", name);
            backend_port.connect_external_port(&name.to_string());
        } else {
            error!(
                "Attempted to connect uninitialized port {:?}",
                self.name_hint
            );
        }
    }

    pub fn disconnect_external_port(mut self: Pin<&mut MidiControlPort>, name: QString) {
        if let Some(backend_port) = self.as_mut().backend_port_wrapper.as_ref() {
            debug!("Disconnecting from external port {:?}", name);
            backend_port.disconnect_external_port(&name.to_string());
        } else {
            error!(
                "Attempted to connect uninitialized port {:?}",
                self.name_hint
            );
        }
    }

    pub fn update_send_queue(mut self: Pin<&mut MidiControlPort>) {
        let do_send = |msg: &[u8], rust_mut: &mut MidiControlPortRust| {
            if PortDirection::try_from(rust_mut.direction).unwrap_or(PortDirection::Input)
                == PortDirection::Output
            {
                if let Some(port) = rust_mut.backend_port_wrapper.as_ref() {
                    debug!("Sending: {:?}", msg);
                    port.send_midi(msg);
                } else {
                    warn!("Dropping message to send: port not initialized");
                }
            } else {
                error!("Dropped msg in send queue: port is not an output");
            }
        };

        let mut rust_mut = self.as_mut().rust_mut();

        if rust_mut.send_rate_limit_hz == 0 {
            while rust_mut.send_queue.len() > 0 {
                do_send(&rust_mut.send_queue.remove(0), &mut rust_mut);
            }
        } else if rust_mut.send_queue.len() > 0 {
            do_send(&rust_mut.send_queue.remove(0), &mut rust_mut);
            unsafe {
                if let Err(e) = invoke::<_, (), _>(
                    &mut *self.send_timer,
                    "start",
                    connection_types::DIRECT_CONNECTION,
                    &(),
                ) {
                    error!("Failed to start send timer: {:?}", e);
                }
            }
        }
    }

    pub fn queue_send_msg(mut self: Pin<&mut MidiControlPort>, msg: QList_u8) {
        let mut rust_mut = self.as_mut().rust_mut();
        rust_mut
            .send_queue
            .push(msg.iter().map(|x| *x).collect::<Vec<u8>>());
        if rust_mut.send_rate_limit_hz > 0 {
            unsafe {
                if let Err(e) = invoke::<_, (), _>(
                    &mut *self.send_timer,
                    "start",
                    connection_types::DIRECT_CONNECTION,
                    &(),
                ) {
                    error!("Failed to start send timer: {:?}", e);
                }
            }
        } else {
            self.as_mut().update_send_queue();
        }
    }

    pub fn autoconnect_update(mut self: Pin<&mut MidiControlPort>) {
        debug!("Autoconnect update");

        let self_qobj = unsafe { self.as_mut().pin_mut_qobject_ptr() };
        let mut rust_mut = self.as_mut().rust_mut();
        rust_mut.autoconnecters.clear();

        for regex in rust_mut.autoconnect_regexes.clone().iter() {
            let autoconnect = make_unique_autoconnect();
            let mut pin_mut_autoconnect =
                unsafe { std::pin::Pin::new_unchecked(&mut *autoconnect.as_mut_ptr()) };
            pin_mut_autoconnect
                .as_mut()
                .set_connect_to_port_regex(regex.clone());
            pin_mut_autoconnect.as_mut().set_internal_port(self_qobj);

            unsafe {
                connect_or_report(
                    &mut *autoconnect.as_mut_ptr(),
                    "only_external_found()",
                    &mut *self_qobj,
                    "detected_external_autoconnect_partner_while_closed()",
                    connection_types::AUTO_CONNECTION,
                );
                connect_or_report(
                    &mut *autoconnect.as_mut_ptr(),
                    "connected()",
                    &mut *self_qobj,
                    "connected()",
                    connection_types::AUTO_CONNECTION,
                );
                if let Err(e) = invoke::<_, (), _>(
                    &mut *autoconnect.as_mut_ptr(),
                    "update()",
                    connection_types::AUTO_CONNECTION,
                    &(),
                ) {
                    error!("Failed to update autoconnect: {:?}", e);
                }
            }

            rust_mut.autoconnecters.push(autoconnect);
        }
    }

    fn handle_msg(mut self: Pin<&mut MidiControlPort>, msg: &[u8]) {
        let mut rust_mut = self.as_mut().rust_mut();
        if is_note_on(msg) {
            rust_mut.active_notes.insert((channel(msg), note(msg)));
        } else if is_note_off(msg) {
            rust_mut.active_notes.remove(&(channel(msg), note(msg)));
        } else if is_cc(msg) {
            rust_mut.cc_states[channel(msg) as usize][note(msg) as usize] = Some(msg[2]);
        }
    }

    pub fn poll(mut self: Pin<&mut MidiControlPort>) {
        while let Some(Some(msg)) = self
            .as_mut()
            .backend_port_wrapper
            .as_ref()
            .map(|port| port.maybe_next_message())
        {
            debug!("Received: {:?}", msg.data);
            self.as_mut().handle_msg(&msg.data);
        }
    }
}

pub fn register_qml_type(module_name: &str, type_name: &str) {
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    unsafe {
        register_qml_type_midi_control_port(std::ptr::null_mut(), &mut mdl, 1, 0, &mut tp);
    }
}
