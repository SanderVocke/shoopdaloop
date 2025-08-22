use cxx_qt_lib_shoop::qobject::QObject;
use frontend::any_backend_port::AnyBackendPort;
use frontend::cxx_qt_shoop::qobj_loop_backend_bridge::ffi::qobject_to_loop_backend_ptr;
use frontend::cxx_qt_shoop::qobj_loop_backend_bridge::LoopBackend;
use frontend::cxx_qt_shoop::qobj_update_thread_bridge::UpdateThread;
use pyo3::prelude::*;
use serde_json;
use std::pin::Pin;

use frontend::cxx_qt_shoop::qobj_backend_wrapper_bridge::BackendWrapper as CxxQtBackendWrapper;
use frontend::cxx_qt_shoop::qobj_loop_gui_bridge::ffi::{
    qobject_to_loop_ptr, QList_QVariant, QVariant,
};
use frontend::cxx_qt_shoop::qobj_loop_gui_bridge::LoopGui;
use frontend::cxx_qt_shoop::qobj_port_backend_bridge::PortBackend as CxxQtPortBackend;
use frontend::cxx_qt_shoop::qobj_port_gui_bridge::PortGui as CxxQtPortGui;

use cxx_qt_lib_shoop::qvariant_helpers::{qobject_ptr_to_qvariant, qvariant_to_qobject_ptr};

use shoop_py_backend::audio_channel::AudioChannel;
use shoop_py_backend::audio_port::AudioPort;
use shoop_py_backend::decoupled_midi_port::DecoupledMidiPort;
use shoop_py_backend::fx_chain::FXChain;
use shoop_py_backend::midi_channel::MidiChannel;
use shoop_py_backend::midi_port::MidiPort;

#[pyfunction]
pub fn shoop_rust_set_crash_json_toplevel_field(field_name: &str, json: &str) {
    let json_str = json.to_string();
    let maybe_structured = serde_json::from_str(&json_str);
    let value = if maybe_structured.is_ok() {
        maybe_structured.unwrap()
    } else {
        serde_json::json!(json_str)
    };

    crashhandling::set_crash_json_toplevel_field(field_name, value);
}

#[pyfunction]
pub fn shoop_rust_set_crash_json_tag(tag: &str, json: &str) {
    let json_str = json.to_string();
    let maybe_structured = serde_json::from_str(&json_str);
    let value = if maybe_structured.is_ok() {
        maybe_structured.unwrap()
    } else {
        serde_json::json!(json_str)
    };

    crashhandling::set_crash_json_tag(tag, value);
}

#[pyfunction]
pub fn shoop_rust_make_qml_application_engine(_parent: u64) -> u64 {
    todo!();
}

#[pyfunction]
pub fn shoop_rust_get_engine_update_thread_addr() -> u64 {
    frontend::engine_update_thread::get_engine_update_thread().thread as usize as u64
}

#[pyfunction]
pub fn shoop_rust_get_engine_update_thread_wrapper_addr() -> u64 {
    frontend::engine_update_thread::get_engine_update_thread() as *mut UpdateThread as usize as u64
}

#[pyfunction]
pub fn shoop_rust_create_autoconnect() -> u64 {
    unsafe { frontend::init::shoop_rust_create_autoconnect() as usize as u64 }
}

#[pyfunction]
pub fn shoop_rust_create_fx_chain(
    backend_addr: usize,
    chain_type: u32,
    title: &str,
) -> PyResult<FXChain> {
    unsafe {
        let backend_ptr: *mut CxxQtBackendWrapper = backend_addr as *mut CxxQtBackendWrapper;
        let backend_mut: &mut CxxQtBackendWrapper = backend_ptr.as_mut().unwrap();
        let backend_pin: Pin<&mut CxxQtBackendWrapper> = Pin::new_unchecked(backend_mut);
        Ok(FXChain {
            obj: backend_pin.create_fx_chain(chain_type, title),
        })
    }
}

#[pyfunction]
pub fn shoop_rust_connect_audio_channel_to_port(
    channel: &shoop_py_backend::audio_channel::AudioChannel,
    input: bool,
    disconnect: bool,
    port_addr: usize,
) -> PyResult<()> {
    unsafe {
        let port_ptr: *mut QObject = port_addr as *mut QObject;
        let port_gui_ptr: *mut CxxQtPortGui = port_ptr as *mut CxxQtPortGui;
        let port_gui_mut: &mut CxxQtPortGui = port_gui_ptr.as_mut().unwrap();
        let port_gui_pin: Pin<&mut CxxQtPortGui> = Pin::new_unchecked(port_gui_mut);
        let port_backend: *mut CxxQtPortBackend =
            port_gui_pin.backend_port_wrapper.data().unwrap() as *mut CxxQtPortBackend;
        let port_pin: Pin<&mut CxxQtPortBackend> = Pin::new_unchecked(&mut *port_backend);
        if let Some(backend_port) = port_pin.maybe_backend_port.as_ref() {
            if let AnyBackendPort::Audio(backend_port) = backend_port {
                if input {
                    if !disconnect {
                        channel.obj.connect_input(backend_port);
                    } else {
                        channel.obj.disconnect(backend_port);
                    }
                } else {
                    if !disconnect {
                        channel.obj.connect_output(backend_port);
                    } else {
                        channel.obj.disconnect(backend_port);
                    }
                }
            }
        } else {
            println!("could not connect channel to port: port not yet initialized");
        }
    }
    Ok(())
}

#[pyfunction]
pub fn shoop_rust_connect_midi_channel_to_port(
    channel: &shoop_py_backend::midi_channel::MidiChannel,
    input: bool,
    disconnect: bool,
    port_addr: usize,
) -> PyResult<()> {
    unsafe {
        let port_ptr: *mut QObject = port_addr as *mut QObject;
        let port_gui_ptr: *mut CxxQtPortGui = port_ptr as *mut CxxQtPortGui;
        let port_gui_mut: &mut CxxQtPortGui = port_gui_ptr.as_mut().unwrap();
        let port_gui_pin: Pin<&mut CxxQtPortGui> = Pin::new_unchecked(port_gui_mut);
        let port_backend: *mut CxxQtPortBackend =
            port_gui_pin.backend_port_wrapper.data().unwrap() as *mut CxxQtPortBackend;
        let port_pin: Pin<&mut CxxQtPortBackend> = Pin::new_unchecked(&mut *port_backend);
        if let Some(backend_port) = port_pin.maybe_backend_port.as_ref() {
            if let AnyBackendPort::Midi(backend_port) = backend_port {
                if input {
                    if !disconnect {
                        channel.obj.connect_input(backend_port);
                    } else {
                        channel.obj.disconnect(backend_port);
                    }
                } else {
                    if !disconnect {
                        channel.obj.connect_output(backend_port);
                    } else {
                        channel.obj.disconnect(backend_port);
                    }
                }
            }
        } else {
            println!("could not connect channel to port: port not yet initialized");
        }
    }
    Ok(())
}

#[pyfunction]
pub fn shoop_rust_open_driver_audio_port(
    backend_addr: usize,
    name_hint: &str,
    direction: i32,
    min_n_ringbuffer_samples: i32,
) -> PyResult<AudioPort> {
    unsafe {
        let backend_ptr: *mut CxxQtBackendWrapper = backend_addr as *mut CxxQtBackendWrapper;
        let backend_mut: &mut CxxQtBackendWrapper = backend_ptr.as_mut().unwrap();
        let backend_pin: Pin<&mut CxxQtBackendWrapper> = Pin::new_unchecked(backend_mut);
        Ok(AudioPort {
            obj: backend_pin
                .open_driver_audio_port(name_hint, direction, min_n_ringbuffer_samples)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))?,
        })
    }
}

#[pyfunction]
pub fn shoop_rust_open_driver_midi_port(
    backend_addr: usize,
    name_hint: &str,
    direction: i32,
    min_n_ringbuffer_samples: i32,
) -> PyResult<MidiPort> {
    unsafe {
        let backend_ptr: *mut CxxQtBackendWrapper = backend_addr as *mut CxxQtBackendWrapper;
        let backend_mut: &mut CxxQtBackendWrapper = backend_ptr.as_mut().unwrap();
        let backend_pin: Pin<&mut CxxQtBackendWrapper> = Pin::new_unchecked(backend_mut);
        Ok(MidiPort {
            obj: backend_pin
                .open_driver_midi_port(name_hint, direction, min_n_ringbuffer_samples)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))?,
        })
    }
}

#[pyfunction]
pub fn shoop_rust_open_driver_decoupled_midi_port(
    backend_addr: usize,
    name_hint: &str,
    direction: i32,
) -> PyResult<DecoupledMidiPort> {
    unsafe {
        let backend_ptr: *mut CxxQtBackendWrapper = backend_addr as *mut CxxQtBackendWrapper;
        let backend_mut: &mut CxxQtBackendWrapper = backend_ptr.as_mut().unwrap();
        let backend_pin: Pin<&mut CxxQtBackendWrapper> = Pin::new_unchecked(backend_mut);
        Ok(DecoupledMidiPort {
            obj: backend_pin
                .open_driver_decoupled_midi_port(name_hint, direction)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))?,
        })
    }
}

#[pyfunction]
pub fn shoop_rust_add_loop_audio_channel(loop_addr: usize, mode: i32) -> PyResult<AudioChannel> {
    unsafe {
        let loop_ptr: *mut LoopGui = loop_addr as *mut LoopGui;
        let loop_mut: &mut LoopGui = loop_ptr.as_mut().unwrap();
        let loop_pin: Pin<&mut LoopGui> = Pin::new_unchecked(loop_mut);
        Ok(AudioChannel {
            obj: loop_pin
                .add_audio_channel(mode)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))?,
        })
    }
}

#[pyfunction]
pub fn shoop_rust_add_loop_midi_channel(loop_addr: usize, mode: i32) -> PyResult<MidiChannel> {
    unsafe {
        let loop_ptr: *mut LoopGui = loop_addr as *mut LoopGui;
        let loop_mut: &mut LoopGui = loop_ptr.as_mut().unwrap();
        let loop_pin: Pin<&mut LoopGui> = Pin::new_unchecked(loop_mut);
        Ok(MidiChannel {
            obj: loop_pin
                .add_midi_channel(mode)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))?,
        })
    }
}

#[pyfunction]
pub fn shoop_rust_transition_gui_loop(
    loop_addr: usize,
    mode: i32,
    maybe_delay: i32,
    maybe_align_to_sync_at: i32,
) -> PyResult<()> {
    unsafe {
        // FIXME: start to implement generically
        // let loop_qobj : *mut QObject = loop_addr as *mut QObject;
        // invoke::<_,(),_>(loop_qobj.as_mut().unwrap(),
        //                  qobj_loop_bridge::constants::INVOKABLE_TRANSITION.to_string(),
        //                  connection_types::DIRECT_CONNECTION,
        //                 &(mode, maybe_delay, maybe_align_to_sync_at));

        let loop_ptr: *mut LoopGui = loop_addr as *mut LoopGui;
        let loop_mut: &mut LoopGui = loop_ptr.as_mut().unwrap();
        let loop_pin: Pin<&mut LoopGui> = Pin::new_unchecked(loop_mut);
        loop_pin.transition(mode, maybe_delay, maybe_align_to_sync_at);
        Ok(())
    }
}

#[pyfunction]
pub fn shoop_rust_transition_gui_loops(
    loop_addrs: Vec<usize>,
    mode: i32,
    maybe_delay: i32,
    maybe_align_to_sync_at: i32,
) -> PyResult<()> {
    let loop_variants: Vec<QVariant> = loop_addrs
        .iter()
        .map(|addr| qobject_ptr_to_qvariant(&(*addr as *mut QObject)).unwrap())
        .collect();
    let loops_list: QList_QVariant = QList_QVariant::from(loop_variants);
    if loops_list.is_empty() {
        return Ok(());
    } else {
        let first_loop = loops_list.get(0).unwrap();
        unsafe {
            let loop_gui_qobj: *mut QObject = qvariant_to_qobject_ptr(first_loop).unwrap();
            let loop_gui_ptr: *mut LoopGui = qobject_to_loop_ptr(loop_gui_qobj);
            let loop_gui_pin: Pin<&mut LoopGui> = std::pin::Pin::new_unchecked(&mut *loop_gui_ptr);
            loop_gui_pin.transition_multiple(loops_list, mode, maybe_delay, maybe_align_to_sync_at);
        }
    }
    Ok(())
}

#[pyfunction]
pub fn shoop_rust_transition_backend_loop(
    loop_addr: usize,
    mode: i32,
    maybe_delay: i32,
    maybe_align_to_sync_at: i32,
) -> PyResult<()> {
    unsafe {
        // FIXME: start to implement generically
        // let loop_qobj : *mut QObject = loop_addr as *mut QObject;
        // invoke::<_,(),_>(loop_qobj.as_mut().unwrap(),
        //                  qobj_loop_bridge::constants::INVOKABLE_TRANSITION.to_string(),
        //                  connection_types::DIRECT_CONNECTION,
        //                 &(mode, maybe_delay, maybe_align_to_sync_at));

        let loop_ptr: *mut LoopBackend = loop_addr as *mut LoopBackend;
        let loop_mut: &mut LoopBackend = loop_ptr.as_mut().unwrap();
        let loop_pin: Pin<&mut LoopBackend> = Pin::new_unchecked(loop_mut);
        loop_pin.transition(mode, maybe_delay, maybe_align_to_sync_at);
        Ok(())
    }
}

#[pyfunction]
pub fn shoop_rust_transition_backend_loops(
    loop_addrs: Vec<usize>,
    mode: i32,
    maybe_delay: i32,
    maybe_align_to_sync_at: i32,
) -> PyResult<()> {
    let loop_variants: Vec<QVariant> = loop_addrs
        .iter()
        .map(|addr| qobject_ptr_to_qvariant(&(*addr as *mut QObject)).unwrap())
        .collect();
    let loops_list: QList_QVariant = QList_QVariant::from(loop_variants);
    if loops_list.is_empty() {
        return Ok(());
    } else {
        let first_loop = loops_list.get(0).unwrap();
        unsafe {
            let loop_gui_qobj: *mut QObject = qvariant_to_qobject_ptr(first_loop).unwrap();
            let loop_gui_ptr: *mut LoopBackend = qobject_to_loop_backend_ptr(loop_gui_qobj);
            let loop_gui_pin: Pin<&mut LoopBackend> =
                std::pin::Pin::new_unchecked(&mut *loop_gui_ptr);
            loop_gui_pin.transition_multiple(loops_list, mode, maybe_delay, maybe_align_to_sync_at);
        }
    }
    Ok(())
}

#[pyfunction]
pub fn shoop_rust_gui_loop_adopt_ringbuffers(
    loop_addr: usize,
    reverse_start: i32,
    n_cycles: i32,
    go_to_cycle: i32,
    go_to_mode: i32,
) {
    unsafe {
        let loop_ptr: *mut LoopGui = loop_addr as *mut LoopGui;
        let loop_mut: &mut LoopGui = loop_ptr.as_mut().unwrap();
        let loop_pin: Pin<&mut LoopGui> = Pin::new_unchecked(loop_mut);
        loop_pin.adopt_ringbuffers(
            if reverse_start >= 0 {
                QVariant::from(&reverse_start)
            } else {
                QVariant::default()
            },
            if n_cycles >= 0 {
                QVariant::from(&n_cycles)
            } else {
                QVariant::default()
            },
            if go_to_cycle >= 0 {
                QVariant::from(&go_to_cycle)
            } else {
                QVariant::default()
            },
            go_to_mode,
        );
    }
}

#[pyfunction]
pub fn shoop_rust_backend_loop_adopt_ringbuffers(
    loop_addr: usize,
    reverse_start: i32,
    n_cycles: i32,
    go_to_cycle: i32,
    go_to_mode: i32,
) {
    unsafe {
        let loop_ptr: *mut LoopBackend = loop_addr as *mut LoopBackend;
        let loop_mut: &mut LoopBackend = loop_ptr.as_mut().unwrap();
        let loop_pin: Pin<&mut LoopBackend> = Pin::new_unchecked(loop_mut);
        loop_pin.adopt_ringbuffers(
            if reverse_start >= 0 {
                QVariant::from(&reverse_start)
            } else {
                QVariant::default()
            },
            if n_cycles >= 0 {
                QVariant::from(&n_cycles)
            } else {
                QVariant::default()
            },
            if go_to_cycle >= 0 {
                QVariant::from(&go_to_cycle)
            } else {
                QVariant::default()
            },
            go_to_mode,
        );
    }
}
