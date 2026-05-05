#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qquickwindow_helpers.h");
        include!("cxx-qt-lib-shoop/qobject.h");
        type QObject = crate::qobject::QObject;

        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        unsafe fn qobject_as_qquickwindow_title(obj: *mut QObject) -> Result<QString>;
        unsafe fn qobject_as_qquickwindow_grab_and_save(
            obj: *mut QObject,
            filename: QString,
        ) -> Result<bool>;
    }
}
