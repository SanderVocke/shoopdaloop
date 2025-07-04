use pyo3::prelude::*;
use frontend;

#[pyfunction]
pub fn shoop_rust_init() {
    frontend::init::shoop_rust_init();
}

#[pyfunction]
pub fn shoop_rust_init_engine_update_thread() {
    frontend::engine_update_thread::init();
}
