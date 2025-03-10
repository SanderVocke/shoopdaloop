use pyo3::prelude::*;
use std::pin::Pin;

use frontend::cxx_qt_shoop::qobj_backend_wrapper_bridge::BackendWrapper as CxxQtBackendWrapper;
use frontend::cxx_qt_shoop::qobj_loop_bridge::Loop as CxxQtLoop;
use frontend::cxx_qt_shoop::qobj_loop_bridge::ffi::{QObject, QVariant, QList_QVariant};

use frontend::cxx_qt_lib_shoop::qvariant_helpers::qobject_ptr_to_qvariant;

use crate::shoop_py_backend::shoop_loop::Loop;
use crate::shoop_py_backend::fx_chain::FXChain;
use crate::shoop_py_backend::audio_port::AudioPort;
use crate::shoop_py_backend::midi_port::MidiPort;
use crate::shoop_py_backend::decoupled_midi_port::DecoupledMidiPort;
use crate::shoop_py_backend::audio_channel::AudioChannel;
use crate::shoop_py_backend::midi_channel::MidiChannel;

#[pyfunction]
pub fn shoop_rust_create_autoconnect() -> u64 { unsafe { frontend::init::shoop_rust_create_autoconnect() as usize as u64 } }

#[pyfunction]
pub fn shoop_rust_create_loop(backend_addr : usize) -> Loop {
    unsafe {
        let backend_ptr : *mut CxxQtBackendWrapper = backend_addr as *mut CxxQtBackendWrapper;
        let backend_mut : &mut CxxQtBackendWrapper = backend_ptr.as_mut().unwrap();
        let backend_pin : Pin<&mut CxxQtBackendWrapper> = Pin::new_unchecked(backend_mut);
        Loop { obj : backend_pin.create_loop() }
    }
}

#[pyfunction]
pub fn shoop_rust_create_fx_chain(backend_addr : usize, chain_type : u32, title : &str) -> PyResult<FXChain> {
    unsafe {
        let backend_ptr : *mut CxxQtBackendWrapper = backend_addr as *mut CxxQtBackendWrapper;
        let backend_mut : &mut CxxQtBackendWrapper = backend_ptr.as_mut().unwrap();
        let backend_pin : Pin<&mut CxxQtBackendWrapper> = Pin::new_unchecked(backend_mut);
        Ok(FXChain { obj : backend_pin.create_fx_chain(chain_type, title) })
    }
}

#[pyfunction]
pub fn shoop_rust_open_driver_audio_port(backend_addr : usize, name_hint : &str, direction : i32, min_n_ringbuffer_samples : i32) -> PyResult<AudioPort> {
    unsafe {
        let backend_ptr : *mut CxxQtBackendWrapper = backend_addr as *mut CxxQtBackendWrapper;
        let backend_mut : &mut CxxQtBackendWrapper = backend_ptr.as_mut().unwrap();
        let backend_pin : Pin<&mut CxxQtBackendWrapper> = Pin::new_unchecked(backend_mut);
        Ok(AudioPort { obj : backend_pin.open_driver_audio_port(name_hint, direction, min_n_ringbuffer_samples)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))?
        })
    }
}

#[pyfunction]
pub fn shoop_rust_open_driver_midi_port(backend_addr : usize, name_hint : &str, direction : i32, min_n_ringbuffer_samples : i32) -> PyResult<MidiPort> {
    unsafe {
        let backend_ptr : *mut CxxQtBackendWrapper = backend_addr as *mut CxxQtBackendWrapper;
        let backend_mut : &mut CxxQtBackendWrapper = backend_ptr.as_mut().unwrap();
        let backend_pin : Pin<&mut CxxQtBackendWrapper> = Pin::new_unchecked(backend_mut);
        Ok(MidiPort { obj : backend_pin.open_driver_midi_port(name_hint, direction, min_n_ringbuffer_samples)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))?
        })
    }
}

#[pyfunction]
pub fn shoop_rust_open_driver_decoupled_midi_port(backend_addr : usize, name_hint : &str, direction : i32) -> PyResult<DecoupledMidiPort> {
    unsafe {
        let backend_ptr : *mut CxxQtBackendWrapper = backend_addr as *mut CxxQtBackendWrapper;
        let backend_mut : &mut CxxQtBackendWrapper = backend_ptr.as_mut().unwrap();
        let backend_pin : Pin<&mut CxxQtBackendWrapper> = Pin::new_unchecked(backend_mut);
        Ok(DecoupledMidiPort { obj : backend_pin.open_driver_decoupled_midi_port(name_hint, direction)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))?
        })
    }
}

#[pyfunction]
pub fn shoop_rust_add_loop_audio_channel(loop_addr : usize, mode : i32) -> PyResult<AudioChannel> {
    unsafe {
        let loop_ptr : *mut CxxQtLoop = loop_addr as *mut CxxQtLoop;
        let loop_mut : &mut CxxQtLoop = loop_ptr.as_mut().unwrap();
        let loop_pin : Pin<&mut CxxQtLoop> = Pin::new_unchecked(loop_mut);
        Ok(AudioChannel { obj : loop_pin.add_audio_channel(mode)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))?
        })
    }
}

#[pyfunction]
pub fn shoop_rust_add_loop_midi_channel(loop_addr : usize, mode : i32) -> PyResult<MidiChannel> {
    unsafe {
        let loop_ptr : *mut CxxQtLoop = loop_addr as *mut CxxQtLoop;
        let loop_mut : &mut CxxQtLoop = loop_ptr.as_mut().unwrap();
        let loop_pin : Pin<&mut CxxQtLoop> = Pin::new_unchecked(loop_mut);
        Ok(MidiChannel { obj : loop_pin.add_midi_channel(mode)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))?
        })
    }
}

#[pyfunction]
pub fn shoop_rust_transition_loop(loop_addr : usize,
                                  mode : i32,
                                  maybe_delay : i32,
                                  maybe_align_to_sync_at : i32) -> PyResult<()>
{
    unsafe {
        let loop_ptr : *mut CxxQtLoop = loop_addr as *mut CxxQtLoop;
        let loop_mut : &mut CxxQtLoop = loop_ptr.as_mut().unwrap();
        let loop_pin : Pin<&mut CxxQtLoop> = Pin::new_unchecked(loop_mut);
        loop_pin.transition(mode, maybe_delay, maybe_align_to_sync_at);
        Ok(())
    }
}

#[pyfunction]
pub fn shoop_rust_transition_loops(loop_addrs : Vec<usize>,
                                   mode : i32,
                                   maybe_delay : i32,
                                   maybe_align_to_sync_at : i32) -> PyResult<()>
{
    unsafe {
        let loop_variants : Vec<QVariant> =
            loop_addrs.iter()
                      .map(|addr| qobject_ptr_to_qvariant(*addr as *mut QObject))
                      .collect();
        let loops_list : QList_QVariant = QList_QVariant::from(loop_variants);
        Ok(())
    }
}