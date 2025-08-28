use common::logging::macros::*;
shoop_log_unit!("Frontend.KeyModifiers");

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-shoop/ShoopKeyEventParser.h");
        type ShoopKeyEventParser;

        include!("cxx-qt-lib-shoop/qobject.h");
        type QObject = cxx_qt_lib_shoop::qobject::QObject;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[base = ShoopKeyEventParser]
        #[qproperty(bool, shift_pressed)]
        #[qproperty(bool, control_pressed)]
        #[qproperty(bool, alt_pressed)]
        type KeyModifiers = super::KeyModifiersRust;

        #[cxx_override]
        pub fn on_shift_pressed(self: Pin<&mut KeyModifiers>);

        #[cxx_override]
        pub fn on_control_pressed(self: Pin<&mut KeyModifiers>);

        #[cxx_override]
        pub fn on_alt_pressed(self: Pin<&mut KeyModifiers>);

        #[cxx_override]
        pub fn on_shift_released(self: Pin<&mut KeyModifiers>);

        #[cxx_override]
        pub fn on_control_released(self: Pin<&mut KeyModifiers>);

        #[cxx_override]
        pub fn on_alt_released(self: Pin<&mut KeyModifiers>);

        #[inherit]
        #[qinvokable]
        pub unsafe fn install(self: Pin<&mut KeyModifiers>);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/make_unique.h");

        #[rust_name = "make_unique_keymodifiers"]
        fn make_unique() -> UniquePtr<KeyModifiers>;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/register_qml_type.h");

        #[rust_name = "register_qml_singleton_keymodifiers"]
        unsafe fn register_qml_singleton(
            inference_example: *mut KeyModifiers,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        );
    }
}

#[derive(Default)]
pub struct KeyModifiersRust {
    pub shift_pressed: bool,
    pub control_pressed: bool,
    pub alt_pressed: bool,
}
