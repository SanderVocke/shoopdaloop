use anyhow;
use common::logging::macros::*;
use config::config::ShoopConfig;
use pyo3::prelude::*;
use pyo3::types::{PyList, PyString};
use std::env;
shoop_log_unit!("Main");

fn shoopdaloop_main_impl<'py>(config: ShoopConfig) -> Result<i32, anyhow::Error> {
    // Set up PYTHONPATH.
    env::set_var(
        "PYTHONPATH",
        &config.pythonpaths.join(common::util::PATH_LIST_SEPARATOR),
    );
    env::set_var("PYTHONHOME", &config.pythonhome);

    let qt_plugins_path = &config.qt_plugins_dir;
    if !qt_plugins_path.is_empty() {
        env::set_var("QT_PLUGIN_PATH", qt_plugins_path);
    }

    let mut qml_dirs: Vec<String> = (&config).additional_qml_dirs.clone();
    qml_dirs.insert(0, (&config).qml_dir.clone());
    let qml_dirs_string = qml_dirs.join(common::util::PATH_LIST_SEPARATOR);
    env::set_var("SHOOP_QML_PATHS", &qml_dirs_string);

    debug!("PYTHONPATH={:?}", env::var("PYTHONPATH"));
    debug!("PYTHONHOME={:?}", env::var("PYTHONHOME"));
    debug!("SHOOP_QML_PATHS={:?}", env::var("SHOOP_QML_PATHS"));
    debug!("QT_PLUGIN_PATH={:?}", env::var("QT_PLUGIN_PATH"));

    // Get the command-line arguments
    let args: Vec<String> = env::args().collect();

    // Initialize the Python interpreter
    Python::with_gil(|py| {
        // Forward command-line arguments
        let sys = py.import("sys")?;
        let os = py.import("os")?;
        let py_args: Vec<_> = args
            .into_iter()
            .map(|arg| PyString::new(py, &arg))
            .collect();
        sys.setattr("argv", PyList::new(py, &py_args)?)?;

        // Print system path
        let runtime_link_path_var = if cfg!(target_os = "windows") {
            "PATH"
        } else if cfg!(target_os = "macos") {
            "DYLD_LIBRARY_PATH"
        } else {
            "LD_LIBRARY_PATH"
        };
        debug!(
            "{runtime_link_path_var}: {:?}",
            env::var(runtime_link_path_var).unwrap()
        );

        // Print python configuration information
        let pythonpaths = sys.getattr("path")?;
        debug!("[python] Python paths: {:?}", pythonpaths);
        let version_info = sys.getattr("version_info")?;
        debug!("[python] Python version: {:?}", version_info);

        // Print env vars once more from within python
        let qml_import_path = os.getattr("getenv")?.call1(("SHOOP_QML_PATHS",))?;
        debug!("[python] QML paths: {:?}", qml_import_path);
        let qt_plugins_path = os.getattr("getenv")?.call1(("QT_PLUGIN_PATH",))?;
        debug!("[python] Qt plugin path: {:?}", qt_plugins_path);

        // Explicitly add DLL search paths for extension modules
        if cfg!(target_os = "windows") {
            for path in &config.dynlibpaths {
                debug!("Add DLL directory: {}", path);
                os.getattr("add_dll_directory")?.call1((path.as_str(),))?;
            }
        }

        // Expose Rust functionality to Python modules
        {
            let py_config = crate::py_config::create_py_config_module(&config, py).unwrap();
            sys.getattr("modules")?
                .set_item("shoop_config", py_config)?;
        }
        {
            let shoop_rust_py_module = crate::shoop_rust_py::create_py_module(py).unwrap();
            sys.getattr("modules")?
                .set_item("shoop_rust", shoop_rust_py_module)?;
        }
        {
            let shoop_py_backend_module = shoop_py_backend::create_py_module(py).unwrap();
            sys.getattr("modules")?
                .set_item("shoop_py_backend", shoop_py_backend_module)?;
        }

        // Call main
        let shoop = PyModule::import(py, "shoopdaloop").map_err(|e| {
            e.print_and_set_sys_last_vars(py);
            anyhow::anyhow!("Python error (details printed)")
        })?;
        let result = shoop.getattr("main")?.call0().map_err(|e| {
            e.print_and_set_sys_last_vars(py);
            anyhow::anyhow!("Python error (details printed)")
        });

        result?
            .extract::<i32>()
            .map_err(|e| anyhow::anyhow!("Python error: {:?}", e))
    })
}

#[cfg(not(feature = "prebuild"))]
pub fn shoopdaloop_main(config: ShoopConfig) -> i32 {
    match shoopdaloop_main_impl(config) {
        Ok(r) => {
            return r;
        }
        Err(e) => {
            error!("Error: {:?}\nBacktrace:\n{:?}", e, e.backtrace());
            return 1;
        }
    }
}
