use pyo3::prelude::*;
use std::pin::Pin;

use frontend::cxx_qt_shoop::qobj_backend_wrapper_bridge::BackendWrapper;

use crate::shoop_py_backend::shoop_loop::Loop;
use crate::shoop_py_backend::audio_port::AudioPort;
use crate::shoop_py_backend::midi_port::MidiPort;

#[pyfunction]
pub fn shoop_rust_create_autoconnect() -> u64 { unsafe { frontend::init::shoop_rust_create_autoconnect() as usize as u64 } }

#[pyfunction]
pub fn shoop_rust_create_loop(backend_addr : usize) -> Loop {
    unsafe {
        let backend_ptr : *mut BackendWrapper = backend_addr as *mut BackendWrapper;
        let backend_mut : &mut BackendWrapper = backend_ptr.as_mut().unwrap();
        let backend_pin : Pin<&mut BackendWrapper> = Pin::new_unchecked(backend_mut);
        Loop { obj : backend_pin.create_loop() }
    }
}

#[pyfunction]
pub fn shoop_rust_open_driver_audio_port(backend_addr : usize, name_hint : &str, direction : i32, min_n_ringbuffer_samples : i32) -> PyResult<AudioPort> {
    unsafe {
        let backend_ptr : *mut BackendWrapper = backend_addr as *mut BackendWrapper;
        let backend_mut : &mut BackendWrapper = backend_ptr.as_mut().unwrap();
        let backend_pin : Pin<&mut BackendWrapper> = Pin::new_unchecked(backend_mut);
        Ok(AudioPort { obj : backend_pin.open_driver_audio_port(name_hint, direction, min_n_ringbuffer_samples)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))?
        })
    }
}

#[pyfunction]
pub fn shoop_rust_open_driver_midi_port(backend_addr : usize, name_hint : &str, direction : i32, min_n_ringbuffer_samples : i32) -> PyResult<MidiPort> {
    unsafe {
        let backend_ptr : *mut BackendWrapper = backend_addr as *mut BackendWrapper;
        let backend_mut : &mut BackendWrapper = backend_ptr.as_mut().unwrap();
        let backend_pin : Pin<&mut BackendWrapper> = Pin::new_unchecked(backend_mut);
        Ok(MidiPort { obj : backend_pin.open_driver_midi_port(name_hint, direction, min_n_ringbuffer_samples)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))?
        })
    }
}
