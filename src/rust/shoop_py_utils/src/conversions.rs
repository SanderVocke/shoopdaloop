use cxx_qt_lib;
use cxx_qt_lib_shoop::qmetatype_helpers::*;
use cxx_qt_lib_shoop::qvariant_helpers::{qvariant_type_id, qvariant_type_name};
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyList};

use common::logging::macros::*;
shoop_log_unit!("Frontend.PyConversion");

pub fn qvariantmap_to_python<'py>(
    py: Python<'py>,
    value: &cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>,
) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    value.iter().try_for_each(|(key, val)| -> PyResult<()> {
        let key = key.to_string();
        let val = qvariant_to_python(py, &val).map_err(|e| {
            PyErr::new::<PyException, _>(format!("Conversion error on key {key}: {e}"))
        })?;
        dict.set_item(key.as_str(), val)
            .expect("cannot set dict key");
        Ok(())
    })?;
    Ok(dict)
}

pub fn qvariantlist_to_python<'py>(
    py: Python<'py>,
    value: &cxx_qt_lib::QList<cxx_qt_lib::QVariant>,
) -> PyResult<Bound<'py, PyList>> {
    let mut vec: Vec<Bound<'py, PyAny>> = Vec::new();
    value
        .iter()
        .enumerate()
        .try_for_each(|(idx, val)| -> Result<(), PyErr> {
            let val = qvariant_to_python(py, &val).map_err(|e| {
                PyErr::new::<PyException, _>(format!("Conversion error on index {idx}: {e}"))
            })?;
            vec.push(val);
            Ok(())
        })?;
    let list = PyList::new(py, vec)?;
    Ok(list)
}

pub fn qstringlist_to_python<'py>(
    py: Python<'py>,
    value: &cxx_qt_lib::QList<cxx_qt_lib::QString>,
) -> PyResult<Bound<'py, PyList>> {
    let mut vec: Vec<Bound<'py, PyAny>> = Vec::new();
    value
        .iter()
        .enumerate()
        .try_for_each(|(_, val)| -> Result<(), PyErr> {
            let val = val.to_string();
            vec.push(val.into_pyobject(py).unwrap().as_any().to_owned());
            Ok(())
        })?;
    let list = PyList::new(py, vec)?;
    Ok(list)
}

pub fn qvariant_to_python<'py>(
    py: Python<'py>,
    value: &cxx_qt_lib::QVariant,
) -> PyResult<Bound<'py, PyAny>> {
    if value.is_null() {
        return Ok(py.None().bind(py).to_owned());
    }

    let type_id = qvariant_type_id(value).unwrap();

    match type_id {
        _v if _v == qmetatype_id_int() => {
            return Ok(value
                .value::<i64>()
                .unwrap()
                .into_pyobject(py)
                .unwrap()
                .as_any()
                .to_owned());
        }
        _v if _v == qmetatype_id_qstring() => {
            return Ok(value
                .value::<cxx_qt_lib::QString>()
                .unwrap()
                .to_string()
                .into_pyobject(py)
                .unwrap()
                .as_any()
                .to_owned());
        }
        _v if _v == qmetatype_id_bool() => {
            return Ok(value
                .value::<bool>()
                .unwrap()
                .into_pyobject(py)
                .unwrap()
                .as_any()
                .to_owned());
        }
        _v if _v == qmetatype_id_float() => {
            return Ok(value
                .value::<f32>()
                .unwrap()
                .into_pyobject(py)
                .unwrap()
                .as_any()
                .to_owned());
        }
        _v if _v == qmetatype_id_qvariantmap() => {
            let variantmap =
                cxx_qt_lib_shoop::qvariant_helpers::qvariant_to_qvariantmap(value).unwrap();
            return Ok(qvariantmap_to_python(py, &variantmap)?.into_any());
        }
        _v if _v == qmetatype_id_qvariantlist() => {
            let variantlist =
                cxx_qt_lib_shoop::qvariant_helpers::qvariant_to_qvariantlist(value).unwrap();
            return Ok(qvariantlist_to_python(py, &variantlist)?.into_any());
        }
        _v if _v == qmetatype_id_qstringlist() => {
            let stringlist =
                cxx_qt_lib_shoop::qvariant_helpers::qvariant_to_qstringlist(value).unwrap();
            return Ok(qstringlist_to_python(py, &stringlist)?.into_any());
        }
        _ => {}
    }

    let typename = qvariant_type_name(value).unwrap();
    Err(PyErr::new::<PyException, _>(format!(
        "unimplemented QVariant type conversion for typename {typename}"
    )))
}
