use cxx_qt_lib;
use pyo3::prelude::*;
use pyo3::types::PyAny;

pub struct SchemaValidator {
    py_object: Py<PyAny>,
}

impl Default for SchemaValidator {
    fn default() -> Self {
        let result = || -> PyResult<Py<PyAny>> {
            Python::with_gil(|py| {
                let module = py.import("shoopdaloop.lib.q_objects.SchemaValidatorImpl")?;
                let object = module
                    .getattr("SchemaValidatorImpl")?
                    .extract::<Py<PyAny>>()?;
                Ok(object)
            })
        }();
        match result {
            Ok(object) => SchemaValidator { py_object: object },
            Err(_) => panic!("Could not create a new instance of the SchemaValidator class"),
        }
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
        Python::with_gil(|py| {
            let dict = shoop_py_utils::conversions::qvariantmap_to_python(py, &obj);
            let args = (dict, obj_desc, schemaname, asynchronous)
                .into_pyobject(py)
                .unwrap();
            let result: bool = self
                .py_object
                .getattr(py, "validate_schema")
                .unwrap()
                .call(py, args, None)
                .unwrap()
                .extract(py)
                .unwrap();
            return result;
        });
        return false;
    }
}
