use pyo3::prelude::*;
use pyo3::types::{PyList, PyString};
use std::env;

// Include generated source for remembering the OUT_DIR at build time.
pub const SHOOP_BUILD_OUT_DIR : &str = env!("OUT_DIR");

fn main() -> PyResult<()> {
    // Get the command-line arguments
    let args: Vec<String> = env::args().collect();

    // Escape hatch: this argument is meant just for spitting out the
    // OUT_DIR used at build. Used in the build process for post-build steps.
    if args.len() == 2 && args[1] == "--shoop-build-print-out-dir" {
        println!("{}", SHOOP_BUILD_OUT_DIR);
        return Ok(());
    }

    // Set up PYTHONPATH for distributed installations
    let executable_path = env::current_exe().unwrap();
    let bundled_pythonpath = executable_path.parent().unwrap().join("shoop_lib").join("py");
    env::set_var("PYTHONPATH", bundled_pythonpath.to_str().unwrap());

    pyo3::prepare_freethreaded_python();

    // Initialize the Python interpreter
    Python::with_gil(|py| {
        let sys = py.import_bound("sys")?;
        let py_args: Vec<PyObject> = args.into_iter()
            .map(|arg| PyString::new_bound(py, &arg).to_object(py))
            .collect();
        sys.setattr("argv", PyList::new_bound(py, &py_args))?;

        let shoop = PyModule::import_bound(py, "shoopdaloop")?;
        let result = shoop
            .getattr("main")?
            .call0()?;
        
        Ok(())
    })
}
