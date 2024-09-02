use pyo3::prelude::*;

pub const VERSION : &str = env!("CARGO_PKG_VERSION");
pub const DESCRIPTION : &str = env!("CARGO_PKG_DESCRIPTION");

#[derive(Default)]
pub struct ShoopAppInfo {
    pub version: String,
    pub description: String,
    pub install_info: String,
    pub dynlib_dir: String,
    pub qml_dir: String,
    pub py_dir: String,
    pub lua_dir: String,
    pub resource_dir: String,
    pub schemas_dir: String
}

impl ShoopAppInfo {
    pub fn create_py_module<'py>(
        self: &ShoopAppInfo,
        py: Python<'py>) -> Result<Bound<'py, PyModule>, PyErr> {
        let m = PyModule::new_bound(py, "shoop_app_info")?;
        m.setattr("shoop_version", self.version.clone())?;
        m.setattr("shoop_description", self.description.clone())?;
        m.setattr("shoop_install_info", self.install_info.clone())?;
        m.setattr("shoop_dynlib_dir", self.dynlib_dir.clone())?;
        m.setattr("shoop_qml_dir", self.qml_dir.clone())?;
        m.setattr("shoop_py_dir", self.py_dir.clone())?;
        m.setattr("shoop_lua_dir", self.lua_dir.clone())?;
        m.setattr("shoop_resource_dir", self.resource_dir.clone())?;
        m.setattr("shoop_schemas_dir", self.schemas_dir.clone())?;
        Ok(m)
    }
}