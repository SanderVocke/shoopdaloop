use pyo3::prelude::*;
use std::path::Path;

pub const VERSION : &str = env!("CARGO_PKG_VERSION");
pub const DESCRIPTION : &str = env!("CARGO_PKG_DESCRIPTION");

pub fn create_py_module<'py>(
    py : Python<'py>,
    install_info: &str,
    dynlib_dir: &Path,
    qml_dir: &Path,
    py_dir: &Path,
    lua_dir: &Path,
    resource_dir: &Path,
    schemas_dir: &Path
) -> Result<Bound<'py, PyModule>, PyErr> {
    let m = PyModule::new_bound(py, "shoop_app_info")?;
    m.setattr("version", VERSION)?;
    m.setattr("description", DESCRIPTION)?;
    m.setattr("install_info", install_info)?;
    m.setattr("dynlib_dir", dynlib_dir.to_str().unwrap())?;
    m.setattr("qml_dir", qml_dir.to_str().unwrap())?;
    m.setattr("py_dir", py_dir.to_str().unwrap())?;
    m.setattr("lua_dir", lua_dir.to_str().unwrap())?;
    m.setattr("resource_dir", resource_dir.to_str().unwrap())?;
    m.setattr("schemas_dir", schemas_dir.to_str().unwrap())?;
    Ok(m)
}