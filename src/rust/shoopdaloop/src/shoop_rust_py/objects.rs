use pyo3::prelude::*;
use std::pin::Pin;

use frontend::cxx_qt_shoop::qobj_backend_wrapper_bridge::BackendWrapper;

use crate::shoop_py_backend::shoop_loop::Loop;

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
