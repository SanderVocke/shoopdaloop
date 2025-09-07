#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/register_qml_type.h");
        unsafe fn register_qmlfile_singleton(
            path: &mut String,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        );
    }
}
