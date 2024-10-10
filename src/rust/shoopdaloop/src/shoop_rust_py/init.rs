use pyo3::prelude::*;
use frontend;

#[pyfunction]
pub fn shoop_rust_init() {
    frontend::init::shoop_rust_init();
}
