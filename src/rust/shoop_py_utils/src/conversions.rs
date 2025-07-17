use cxx_qt_lib;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict};

pub fn qvariantmap_to_python<'py>(
    py: Python<'py>,
    value: &cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>,
) -> Bound<'py, PyDict> {
    let dict = PyDict::new(py);
    value.iter().for_each(|(key, val)| {
        let key = key.to_string();
        let val = qvariant_to_python(py, &val);
        dict.set_item(key.as_str(), val)
            .expect("cannot set dict key");
    });
    dict
}

pub fn qvariant_to_python<'py>(py: Python<'py>, value: &cxx_qt_lib::QVariant) -> Bound<'py, PyAny> {
    if !value.is_valid() {
        return py.None().bind(py).to_owned();
    }

    match value.value::<u32>() {
        Some(value) => return value.into_pyobject(py).unwrap().as_any().to_owned(),
        None => (),
    }

    match value.value::<i32>() {
        Some(value) => return value.into_pyobject(py).unwrap().as_any().to_owned(),
        None => (),
    }

    match value.value::<u64>() {
        Some(value) => return value.into_pyobject(py).unwrap().as_any().to_owned(),
        None => (),
    }

    match value.value::<i64>() {
        Some(value) => return value.into_pyobject(py).unwrap().as_any().to_owned(),
        None => (),
    }

    match value.value::<bool>() {
        Some(value) => return value.into_pyobject(py).unwrap().as_any().to_owned(),
        None => (),
    }

    match value.value::<cxx_qt_lib::QString>() {
        Some(value) => {
            return value
                .to_string()
                .into_pyobject(py)
                .unwrap()
                .as_any()
                .to_owned()
        }
        None => (),
    }

    panic!("unimplemented")
}

mod tests {
    #[test]
    fn test_u64() {
        use super::*;
        Python::with_gil(|py| {
            let inval: u64 = 12345;
            let variant = cxx_qt_lib::QVariant::from(&inval);
            let result = qvariant_to_python(py, &variant);
            let outval: u64 = result.extract().expect("Could not extract u64");
            assert!(outval == inval);
        });
    }
}
