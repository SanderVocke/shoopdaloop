use config::config::ShoopConfig;
use pyo3::prelude::*;

pub fn create_py_config_module<'py>(
        config: &ShoopConfig,
        py: Python<'py>) -> Result<Bound<'py, PyModule>, PyErr>
{
    let m = PyModule::new_bound(py, "shoop_config")?;
    m.setattr("shoop_version", config.version.clone())?;
    m.setattr("shoop_description", config.description.clone())?;
    m.setattr("shoop_install_info", config.install_info.clone())?;
    m.setattr("shoop_qml_dir", config.qml_dir.clone())?;
    m.setattr("shoop_lua_dir", config.lua_dir.clone())?;
    m.setattr("shoop_resource_dir", config.resource_dir.clone())?;
    m.setattr("shoop_schemas_dir", config.schemas_dir.clone())?;
    Ok(m)
}