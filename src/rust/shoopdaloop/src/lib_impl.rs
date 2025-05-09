use pyo3::prelude::*;
use pyo3::types::{PyList, PyString};
use std::env;
use anyhow;
use crate::shoop_app_info::ShoopAppInfo;

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

        let lib: String = sys.getattr("executable")?.extract()?;
        println!("Python library loaded: {}", lib);

        // Expose Rust functionality to Python modules
        {
            let py_app_info = app_info.create_py_module(py).unwrap();
            sys.getattr("modules")?.set_item("shoop_app_info",
                                            py_app_info)?;
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
pub fn shoopdaloop_main(app_info : ShoopAppInfo) -> i32 {
    match shoopdaloop_main_impl(app_info) {
        Ok(r) => { return r; }
        Err(e) => {
            println!("Error: {:?}\nBacktrace:\n{:?}", e, e.backtrace());
            return 1;
        }
    }
}
