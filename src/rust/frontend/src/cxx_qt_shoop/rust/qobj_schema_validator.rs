use cxx_qt_lib_shoop::qjsonobject::QJsonObject;

pub use crate::cxx_qt_shoop::qobj_schema_validator_bridge::ffi::SchemaValidator;
use crate::cxx_qt_shoop::qobj_schema_validator_bridge::ffi::*;
use crate::init::GLOBAL_CONFIG;
use std::path::PathBuf;
use std::pin::Pin;

use common::logging::macros::*;
shoop_log_unit!("Frontend.SchemaValidator");

pub fn register_qml_singleton(module_name: &str, type_name: &str) {
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    unsafe {
        register_qml_type_schemavalidator(std::ptr::null_mut(), &mut mdl, 1, 0, &mut tp);
    }
}

impl SchemaValidator {
    pub fn validate_schema(
        self: Pin<&mut SchemaValidator>,
        object: QMap_QString_QVariant,
        descriptor: QString,
        schemaname: QString,
        _asynchronous: bool,
    ) -> bool {
        match || -> Result<bool, anyhow::Error> {
            let json_object = QJsonObject::from_variant_map(&object)
                .map_err(|e| anyhow::anyhow!("Failed to convert: {e}"))?;
            let json = json_object
                .to_json()
                .map_err(|e| anyhow::anyhow!("Failed to convert: {e}"))?;
            let json = serde_json::from_str::<serde_json::Value>(&json)?;

            let schemas_dir = GLOBAL_CONFIG
                .get()
                .as_ref()
                .map(|config| config.schemas_dir.clone())
                .ok_or(anyhow::anyhow!("Global config not initialized"))?;
            let schemas_dir = PathBuf::from(schemas_dir);
            let schema_file = schemas_dir
                .join("schemas")
                .join(schemaname.to_string() + ".json");
            let schema = std::fs::read_to_string(&schema_file)
                .map_err(|e| anyhow::anyhow!("Failed to read schema file {schema_file:?}: {e}"))?;
            let schema = serde_json::from_str::<serde_json::Value>(&schema)?;

            let valid = jsonschema::is_valid(&schema, &json);
            if !valid {
                error!("Error validating {descriptor:?} against schema {schemaname:?}");
            } else {
                debug!("Successfully validated {descriptor:?} against schema {schemaname:?}");
            }

            Ok(valid)
        }() {
            Ok(result) => result,
            Err(e) => {
                error!("could not validate schema: {e}");
                false
            }
        }
    }
}
