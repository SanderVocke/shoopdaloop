use pyo3::prelude::*;
use pyo3::types::{PyList, PyString};
use std::env;

fn main() -> PyResult<()> {
    // Get the command-line arguments
    let args: Vec<String> = env::args().collect();

    pyo3::prepare_freethreaded_python();

    // Initialize the Python interpreter
    Python::with_gil(|py| {
        let sys = py.import_bound("sys")?;
        let py_args: Vec<PyObject> = args.into_iter()
            .map(|arg| PyString::new_bound(py, &arg).to_object(py))
            .collect();
        sys.setattr("argv", PyList::new_bound(py, &py_args))?;
    //     sys.getattr("argv")?.set_item(py, PyList::new_bound(py, &py_args))?;

        let shoop = PyModule::import_bound(py, "shoopdaloop")?;
        let result = shoop
            .getattr("main")?
            .call0()?;
        
        Ok(())
    })
}
