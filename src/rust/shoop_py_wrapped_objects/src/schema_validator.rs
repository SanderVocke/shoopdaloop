use cxx_qt_lib;
use pyo3::prelude::*;
use pyo3::types::PyAny;

use common::logging::macros::*;
shoop_log_unit!("Frontend.SchemaValidator");

pub struct SchemaValidator {
    py_object: Option<Py<PyAny>>,
}

fn create_py_schema_validator<'py>(py: Python<'py>) -> PyResult<Py<PyAny>> {
    let module = py
        .import("shoopdaloop.lib.q_objects.SchemaValidatorImpl")
        .unwrap();
    let class = module
        .getattr("SchemaValidatorImpl")
        .unwrap()
        .extract::<Py<PyAny>>()
        .unwrap();
    let object = class.call0(py).unwrap();
    Ok(object)
}

impl Default for SchemaValidator {
    fn default() -> Self {
        Python::with_gil(|py| {
            return SchemaValidator {
                py_object: Some(create_py_schema_validator(py).unwrap()),
            };
        })
    }
}

impl SchemaValidator {
    pub fn validate_schema(
        &self,
        obj: &cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>,
        obj_desc: &str,
        schemaname: &str,
        asynchronous: bool,
    ) -> bool {
        match Python::with_gil(|py| -> PyResult<bool> {
            let dict = shoop_py_utils::conversions::qvariantmap_to_python(py, &obj)?;
            let args = (dict, obj_desc, schemaname, asynchronous)
                .into_pyobject(py)
                .unwrap();
            let result: bool = self
                .py_object
                .as_ref()
                .unwrap()
                .getattr(py, "validate_schema")
                .unwrap()
                .call(py, args, None)
                .unwrap()
                .extract(py)
                .unwrap();
            debug!("Schema validation for {schemaname} returned {result}");
            Ok(result)
        }) {
            Ok(result) => result,
            Err(e) => {
                error!("Failed to validate schema, ignoring: {e}");
                return true;
            }
        }
    }
}
