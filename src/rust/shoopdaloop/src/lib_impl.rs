pub mod shoop_app_info;
pub mod add_lib_search_path;
pub mod shoop_rust_py;

use pyo3::prelude::*;
use pyo3::types::{PyList, PyString};
use std::env;
use anyhow;
use shoop_app_info::ShoopAppInfo;

fn shoopdaloop_main_impl<'py>(
    app_info : ShoopAppInfo
) -> Result<i32, anyhow::Error> {
    // Get the command-line arguments
    let args: Vec<String> = env::args().collect();

    pyo3::prepare_freethreaded_python();

    // Initialize the Python interpreter
    Python::with_gil(|py| {
        // Forward command-line arguments
        let sys = py.import_bound("sys")?;
        let py_args: Vec<PyObject> = args.into_iter()
            .map(|arg| PyString::new_bound(py, &arg).to_object(py))
            .collect();
        sys.setattr("argv", PyList::new_bound(py, &py_args))?;

        // Expose Rust functionality to Python modules
        {
            let py_app_info = app_info.create_py_module(py).unwrap();
            sys.getattr("modules")?.set_item("shoop_app_info",
                                            py_app_info)?;
        }
        {
            let shoop_rust_py_module = shoop_rust_py::create_py_module(py).unwrap();
            sys.getattr("modules")?.set_item("shoop_rust",
                                              shoop_rust_py_module)?;
        }

        // Call main
        let shoop = PyModule::import_bound(py, "shoopdaloop")?;
        let result = shoop
            .getattr("main")?
            .call0()?;

        result.extract::<i32>()
              .map_err(|e| {
                e.print(py);
                anyhow::anyhow!("Python error (details printed)")
              })
    })
}

#[cfg(not(feature = "prebuild"))]
pub fn shoopdaloop_main(app_info : ShoopAppInfo) -> i32 {
    match shoopdaloop_main_impl(app_info) {
        Ok(r) => { return r; }
        Err(e) => {
            eprintln!("Python error: {:?}\nBacktrace:\n{:?}", e, e.backtrace());
            return 1;
        }
    }
}