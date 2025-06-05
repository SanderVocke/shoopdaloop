use pyo3::prelude::*;
use pyo3::types::{PyList, PyString};
use std::env;
use anyhow;
use crate::config::ShoopConfig;
use common::logging::macros::*;
shoop_log_unit!("Main");

fn shoopdaloop_main_impl<'py>(
    config : ShoopConfig
) -> Result<i32, anyhow::Error> {
    // Set up PYTHONPATH.
    env::set_var("PYTHONPATH", config.pythonpaths.join(common::fs::PATH_LIST_SEPARATOR));

    // Get the command-line arguments
    let args: Vec<String> = env::args().collect();

    pyo3::prepare_freethreaded_python();

    // Initialize the Python interpreter
    Python::with_gil(|py| {
        // Forward command-line arguments
        let sys = py.import_bound("sys")?;
        let os = py.import_bound("os")?;
        let py_args: Vec<PyObject> = args.into_iter()
            .map(|arg| PyString::new_bound(py, &arg).to_object(py))
            .collect();
        sys.setattr("argv", PyList::new_bound(py, &py_args))?;

        // Print system path
        let runtime_link_path_var =
        if cfg!(target_os = "windows") { "PATH" }
        else if cfg!(target_os = "macos") { "DYLD_LIBRARY_PATH" }
        else { "LD_LIBRARY_PATH" };
        debug!("{runtime_link_path_var}: {:?}", env::var(runtime_link_path_var).unwrap());

        // Print python configuration information
        let pythonpaths = sys.getattr("path")?;
        debug!("Python paths: {:?}", pythonpaths);
        let version_info = sys.getattr("version_info")?;
        debug!("Python version: {:?}", version_info);

        // Expose Rust functionality to Python modules
        {
            let py_config = crate::py_config::create_py_config_module(&config, py).unwrap();
            sys.getattr("modules")?.set_item("shoop_config",
                                            py_config)?;
        }
        {
            let shoop_rust_py_module = crate::shoop_rust_py::create_py_module(py).unwrap();
            sys.getattr("modules")?.set_item("shoop_rust",
                                              shoop_rust_py_module)?;
        }
        {
            let shoop_py_backend_module = crate::shoop_py_backend::create_py_module(py).unwrap();
            sys.getattr("modules")?.set_item("shoop_py_backend",
                                              shoop_py_backend_module)?;
        }

        // Call main
        let shoop = PyModule::import_bound(py, "shoopdaloop")
            .map_err(|e| {
                e.print_and_set_sys_last_vars(py);
                anyhow::anyhow!("Python error (details printed)")
            })?;
        let result = shoop
            .getattr("main")?
            .call0()
            .map_err(|e| {
                e.print_and_set_sys_last_vars(py);
                anyhow::anyhow!("Python error (details printed)")
            });

        result?.extract::<i32>()
            .map_err(|e| anyhow::anyhow!("Python error: {:?}", e))
    })
}

#[cfg(not(feature = "prebuild"))]
pub fn shoopdaloop_main(config : ShoopConfig) -> i32 {
    match shoopdaloop_main_impl(config) {
        Ok(r) => { return r; }
        Err(e) => {
            error!("Error: {:?}\nBacktrace:\n{:?}", e, e.backtrace());
            return 1;
        }
    }
}
