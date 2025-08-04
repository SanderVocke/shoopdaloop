use cxx_qt::CxxQtType;

pub use crate::cxx_qt_shoop::qobj_schema_validator_bridge::ffi::SchemaValidator;
use crate::cxx_qt_shoop::qobj_schema_validator_bridge::ffi::*;
use std::pin::Pin;

pub fn register_qml_singleton(module_name: &str, type_name: &str) {
    let obj = make_unique_schemavalidator();
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    register_qml_type_schemavalidator(obj.as_ref().unwrap(), &mut mdl, 1, 0, &mut tp);
}

impl SchemaValidator {
    pub fn validate_schema(
        self: Pin<&mut SchemaValidator>,
        object: QMap_QString_QVariant,
        descriptor: QString,
        schemaname: QString,
        asynchronous: bool,
    ) -> bool {
        let rust = self.rust();
        rust.validator.validate_schema(
            &object,
            descriptor.to_string().as_str(),
            schemaname.to_string().as_str(),
            asynchronous,
        )
    }
}
