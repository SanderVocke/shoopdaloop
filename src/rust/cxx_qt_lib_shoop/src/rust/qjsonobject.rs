use cxx::UniquePtr;
use cxx_qt;

#[cxx_qt::bridge]
mod ffi {

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qjsonobject.h");
        type QJsonObject;

        include!("cxx-qt-lib/qmap.h");
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        #[rust_name = "qjsonobject_to_variant_map"]
        pub fn qjsonobjectToVariantMap(obj: &QJsonObject) -> Result<QMap_QString_QVariant>;

        #[rust_name = "qjsonobject_from_variant_map"]
        pub fn qjsonobjectFromVariantMap(
            map: &QMap_QString_QVariant,
        ) -> Result<UniquePtr<QJsonObject>>;

        #[rust_name = "qjsonobject_from_json"]
        pub fn qjsonobjectFromJson(json: &String) -> Result<UniquePtr<QJsonObject>>;

        #[rust_name = "qjsonobject_to_json"]
        pub fn qjsonobjectToJson(obj: &QJsonObject) -> Result<String>;

        #[rust_name = "qjsonobject_to_variant"]
        pub fn qjsonobjectToVariant(obj: &QJsonObject) -> Result<QVariant>;
    }
}

pub use ffi::*;

impl QJsonObject {
    pub fn from_json(json: &str) -> Result<UniquePtr<QJsonObject>, String> {
        let owned = json.to_owned();
        match ffi::qjsonobject_from_json(&owned) {
            Ok(obj) => Ok(obj),
            Err(e) => Err(format!("Failed to parse JSON: {}", e)),
        }
    }

    pub fn to_json(&self) -> Result<String, String> {
        match ffi::qjsonobject_to_json(self) {
            Ok(json) => Ok(json),
            Err(e) => Err(format!("Failed to serialize JSON: {}", e)),
        }
    }

    pub fn from_variant_map(map: &QMap_QString_QVariant) -> Result<UniquePtr<QJsonObject>, String> {
        match ffi::qjsonobject_from_variant_map(map) {
            Ok(obj) => Ok(obj),
            Err(e) => Err(format!(
                "Failed to convert variant map to JSON object: {}",
                e
            )),
        }
    }

    pub fn to_variant_map(&self) -> Result<QMap_QString_QVariant, String> {
        match ffi::qjsonobject_to_variant_map(self) {
            Ok(map) => Ok(map),
            Err(e) => Err(format!(
                "Failed to convert JSON object to variant map: {}",
                e
            )),
        }
    }

    pub fn to_variant(&self) -> Result<QVariant, String> {
        match ffi::qjsonobject_to_variant(self) {
            Ok(variant) => Ok(variant),
            Err(e) => Err(format!("Failed to convert JSON object to variant: {}", e)),
        }
    }
}
