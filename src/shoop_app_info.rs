use pyo3::prelude::*;
use std::path::Path;

pub const VERSION : &str = env!("CARGO_PKG_VERSION");
pub const DESCRIPTION : &str = env!("CARGO_PKG_DESCRIPTION");

pub fn create_py_module<'py>(
    py : Python<'py>,
    shoop_lib_dir: &Path
) -> Result<Bound<'py, PyModule>, PyErr> {
    let m = PyModule::new_bound(py, "shoop_app_info")?;
    m.setattr("version", VERSION)?;
    m.setattr("description", DESCRIPTION)?;
    m.setattr("lib_dir", shoop_lib_dir.to_str().unwrap())?;
    Ok(m)
}