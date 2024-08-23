use pyo3::prelude::*;
use pyo3::types::{PyList, PyString};
use std::env;
use std::path::{PathBuf, Path};
use glob::glob;

pub mod shoop_app_info;
pub mod add_lib_search_path;
pub mod shoop_rust;

pub fn shoopdaloop_main(
    shoop_lib_path : &Path
) -> PyResult<()> {
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
            let shoop_app_info_module = shoop_app_info::create_py_module
                                                    (py, shoop_lib_path)
                                                    .unwrap();
            sys.getattr("modules")?.set_item("shoop_app_info",
                                            shoop_app_info_module)?;
        }
        {
            let shoop_rust_module = shoop_rust::create_py_module(py).unwrap();
            sys.getattr("modules")?.set_item("shoop_rust",
                                             shoop_rust_module)?;
        }

        // Call main
        let shoop = PyModule::import_bound(py, "shoopdaloop")?;
        let result = shoop
            .getattr("main")?
            .call0()?;

        std::process::exit(result.extract::<i32>()?);
    })
}
