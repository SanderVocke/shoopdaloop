use cxx_qt;

#[cxx_qt::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        include!("cxx-qt-lib-shoop/qobject.h");
        include!("cxx-qt-lib-shoop/qmetatype.h");
        include!("cxx-qt-lib/qmap.h");
        include!("cxx-qt-lib/qlist.h");
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
        type QString = cxx_qt_lib::QString;
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;
        type QList_QVariant = cxx_qt_lib::QList<QVariant>;
        type QList_QString = cxx_qt_lib::QList<QString>;

        type QObject = crate::qobject::QObject;

        #[rust_name = "qmetatype_id_int"]
        unsafe fn meta_type_id(v: *mut i32) -> i32;

        #[rust_name = "qmetatype_id_qstring"]
        unsafe fn meta_type_id(v: *mut QString) -> i32;

        #[rust_name = "qmetatype_id_bool"]
        unsafe fn meta_type_id(v: *mut bool) -> i32;

        #[rust_name = "qmetatype_id_qobject_ptr"]
        unsafe fn meta_type_id(v: *mut *mut QObject) -> i32;

        #[rust_name = "qmetatype_id_qvariantmap"]
        unsafe fn meta_type_id(v: *mut QMap_QString_QVariant) -> i32;

        #[rust_name = "qmetatype_id_qvariantlist"]
        unsafe fn meta_type_id(v: *mut QList_QVariant) -> i32;

        #[rust_name = "qmetatype_id_qstringlist"]
        unsafe fn meta_type_id(v: *mut QList_QString) -> i32;
    }
}

use once_cell::sync::Lazy;

static QMETATYPE_ID_INT: Lazy<i32> =
    Lazy::new(|| unsafe { ffi::qmetatype_id_int(std::ptr::null_mut()) });
static QMETATYPE_ID_QSTRING: Lazy<i32> =
    Lazy::new(|| unsafe { ffi::qmetatype_id_qstring(std::ptr::null_mut()) });
static QMETATYPE_ID_BOOL: Lazy<i32> =
    Lazy::new(|| unsafe { ffi::qmetatype_id_bool(std::ptr::null_mut()) });
static QMETATYPE_ID_QOBJECT_PTR: Lazy<i32> =
    Lazy::new(|| unsafe { ffi::qmetatype_id_qobject_ptr(std::ptr::null_mut()) });
static QMETATYPE_ID_QVARIANTMAP: Lazy<i32> =
    Lazy::new(|| unsafe { ffi::qmetatype_id_qvariantmap(std::ptr::null_mut()) });
static QMETATYPE_ID_QVARIANTLIST: Lazy<i32> =
    Lazy::new(|| unsafe { ffi::qmetatype_id_qvariantlist(std::ptr::null_mut()) });
static QMETATYPE_ID_QSTRINGLIST: Lazy<i32> =
    Lazy::new(|| unsafe { ffi::qmetatype_id_qstringlist(std::ptr::null_mut()) });

pub fn qmetatype_id_int() -> i32 {
    *Lazy::force(&QMETATYPE_ID_INT)
}
pub fn qmetatype_id_qstring() -> i32 {
    *Lazy::force(&QMETATYPE_ID_QSTRING)
}
pub fn qmetatype_id_bool() -> i32 {
    *Lazy::force(&QMETATYPE_ID_BOOL)
}
pub fn qmetatype_id_qobject_ptr() -> i32 {
    *Lazy::force(&QMETATYPE_ID_QOBJECT_PTR)
}
pub fn qmetatype_id_qvariantmap() -> i32 {
    *Lazy::force(&QMETATYPE_ID_QVARIANTMAP)
}
pub fn qmetatype_id_qvariantlist() -> i32 {
    *Lazy::force(&QMETATYPE_ID_QVARIANTLIST)
}
pub fn qmetatype_id_qstringlist() -> i32 {
    *Lazy::force(&QMETATYPE_ID_QSTRINGLIST)
}
