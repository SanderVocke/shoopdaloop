#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qmap.h");
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;
    }

    unsafe extern "RustQt" {
        #[qobject]
        type SchemaValidator = super::SchemaValidatorRust;

        #[qinvokable]
        fn validate_schema(
            self: Pin<&mut SchemaValidator>,
            object: QMap_QString_QVariant,
            descriptor: QString,
            schemaname: QString,
            asynchronous: bool,
        ) -> bool;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/make_unique.h");

        #[rust_name = "make_unique_schemavalidator"]
        fn make_unique() -> UniquePtr<SchemaValidator>;

        include!("cxx-qt-lib-shoop/register_qml_type.h");

        #[rust_name = "register_qml_type_schemavalidator"]
        fn register_qml_singleton(
            inference_example: &SchemaValidator,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        );
    }
}

pub struct SchemaValidatorRust {
    pub validator: shoop_py_wrapped_objects::schema_validator::SchemaValidator,
}

impl Default for SchemaValidatorRust {
    fn default() -> Self {
        Self {
            validator: shoop_py_wrapped_objects::schema_validator::SchemaValidator::default(),
        }
    }
}
