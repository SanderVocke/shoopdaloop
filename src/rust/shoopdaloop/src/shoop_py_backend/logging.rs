use pyo3::prelude::*;
use pyo3::exceptions::PyValueError;
use backend_bindings::{self, Logger as BackendLogger};
use std::collections::HashMap;

#[pyclass(eq, eq_int)]
#[derive(PartialEq, Clone)]
pub enum LogLevel {
    AlwaysTrace = backend_bindings::LogLevel::AlwaysTrace as isize,
    DebugTrace = backend_bindings::LogLevel::DebugTrace as isize,
    Debug = backend_bindings::LogLevel::Debug as isize,
    Info = backend_bindings::LogLevel::Info as isize,
    Warn = backend_bindings::LogLevel::Warn as isize,
    Err = backend_bindings::LogLevel::Err as isize,
}

impl TryFrom<backend_bindings::LogLevel> for LogLevel {
    type Error = anyhow::Error;
    fn try_from(value: backend_bindings::LogLevel) -> Result<Self, anyhow::Error> {
        match value {
            backend_bindings::LogLevel::AlwaysTrace => Ok(LogLevel::AlwaysTrace),
            backend_bindings::LogLevel::DebugTrace => Ok(LogLevel::DebugTrace),
            backend_bindings::LogLevel::Debug => Ok(LogLevel::Debug),
            backend_bindings::LogLevel::Info => Ok(LogLevel::Info),
            backend_bindings::LogLevel::Warn => Ok(LogLevel::Warn),
            backend_bindings::LogLevel::Err => Ok(LogLevel::Err),
        }
    }
}

impl LogLevel {
    pub fn to_ffi(&self) -> backend_bindings::LogLevel {
        match self {
            LogLevel::AlwaysTrace => backend_bindings::LogLevel::AlwaysTrace,
            LogLevel::DebugTrace => backend_bindings::LogLevel::DebugTrace,
            LogLevel::Debug => backend_bindings::LogLevel::Debug,
            LogLevel::Info => backend_bindings::LogLevel::Info,
            LogLevel::Warn => backend_bindings::LogLevel::Warn,
            LogLevel::Err => backend_bindings::LogLevel::Err,
        }
    }
}

#[pymethods]
impl LogLevel {
    #[new]
    fn py_new(value: u32) -> PyResult<Self> {
        match backend_bindings::LogLevel::try_from(value) {
            Ok(val) => Ok(LogLevel::try_from(val).unwrap()),
            Err(_) => Err(PyErr::new::<pyo3::exceptions::PyValueError, _>("Invalid LogLevel")),
        }
    }

    #[staticmethod]
    pub fn enum_items() -> HashMap<&'static str, isize> {
        let mut items = HashMap::new();
        items.insert("AlwaysTrace", LogLevel::AlwaysTrace as isize);
        items.insert("DebugTrace", LogLevel::DebugTrace as isize);
        items.insert("Debug", LogLevel::Debug as isize);
        items.insert("Info", LogLevel::Info as isize);
        items.insert("Warning", LogLevel::Warn as isize);
        items.insert("Error", LogLevel::Err as isize);
        items
    }
}

#[pyclass]
pub struct PyLogger {
    logger: BackendLogger,
}

#[pymethods]
impl PyLogger {
    #[new]
    pub fn new(name: &str) -> PyResult<Self> {
        match BackendLogger::new(name) {
            Ok(logger) => Ok(PyLogger { logger }),
            Err(_) => Err(PyValueError::new_err("Failed to create logger")),
        }
    }

    pub fn log(&self, level: LogLevel, msg: &str) {
        self.logger.log(level.to_ffi(), msg);
    }

    pub fn should_log(&self, level: LogLevel) -> bool {
        self.logger.should_log(level.to_ffi())
    }
}
pub fn set_global_logging_level(level : &LogLevel) {
    backend_bindings::set_global_logging_level(&level.to_ffi());
}

pub fn register_in_module<'py>(m: &Bound<'py, PyModule>) -> PyResult<()> {
    m.add_class::<PyLogger>()?;
    m.add_function(wrap_pyfunction!(set_global_logging_level, m)?)?;
    Ok(())
}
