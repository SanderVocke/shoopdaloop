#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qmap.h");
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;
    }

    unsafe extern "RustQt" {
        #[qobject]
        type SettingsIO = super::SettingsIORust;

        #[qinvokable]
        pub fn save_settings(self: &SettingsIO, settings: QMap_QString_QVariant, override_filename: QVariant);

        #[qinvokable]
        pub fn load_settings(self: &SettingsIO, override_filename: QVariant) -> QMap_QString_QVariant;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/make_unique.h");

        #[rust_name = "make_unique_settingsio"]
        fn make_unique() -> UniquePtr<SettingsIO>;

        include!("cxx-qt-lib-shoop/register_qml_type.h");

        #[rust_name = "register_qml_type_settingsio"]
        unsafe fn register_qml_singleton(
            inference_example: *mut SettingsIO,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        );
    }
}

pub struct SettingsIORust {
}

impl Default for SettingsIORust {
    fn default() -> Self {
        Self {
        }
    }
}
