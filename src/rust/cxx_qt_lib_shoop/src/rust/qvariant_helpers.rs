use cxx_qt;

#[cxx_qt::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib-shoop/qobject.h");
        type QObject = crate::qobject::QObject;

        include!("cxx-qt-lib-shoop/qjsvalue.h");
        type QJSValue = crate::qjsvalue::QJSValue;

        include!("cxx-qt-lib/qlist.h");
        include!("cxx-qt-lib/qstring.h");
        include!("cxx-qt-lib/qmap.h");
        type QString = cxx_qt_lib::QString;
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;
        type QList_QString = cxx_qt_lib::QList<QString>;
        type QList_QVariant = cxx_qt_lib::QList<QVariant>;
        type QList_i32 = cxx_qt_lib::QList<i32>;
        type QList_f32 = cxx_qt_lib::QList<f32>;

        include!("cxx-qt-lib-shoop/qsharedpointer_qobject.h");
        type QSharedPointer_QObject = crate::qsharedpointer_qobject::QSharedPointer_QObject;

        include!("cxx-qt-lib-shoop/qweakpointer_qobject.h");
        type QWeakPointer_QObject = crate::qweakpointer_qobject::QWeakPointer_QObject;

        include!("cxx-qt-lib-shoop/qvariant_helpers.h");

        #[rust_name = "qvariant_type_name"]
        fn qvariantTypeName(obj: &QVariant) -> Result<&str>;

        #[rust_name = "qvariant_type_id"]
        fn qvariantTypeId(obj: &QVariant) -> Result<i32>;

        #[rust_name = "qvariant_convertible_to_qobject_ptr"]
        unsafe fn qvariantConvertibleTo(obj: &QVariant, example: *mut *mut QObject)
            -> Result<bool>;

        #[rust_name = "qvariant_to_qobject_ptr"]
        unsafe fn qvariantAs(
            variant: &QVariant,
            example: *mut *mut QObject,
        ) -> Result<*mut QObject>;

        #[rust_name = "qobject_ptr_to_qvariant"]
        unsafe fn asQVariant(obj: &*mut QObject) -> Result<QVariant>;

        #[rust_name = "qvariant_convertible_to_qvariantmap"]
        unsafe fn qvariantConvertibleTo(
            obj: &QVariant,
            example: *mut QMap_QString_QVariant,
        ) -> Result<bool>;

        #[rust_name = "qvariant_to_qvariantmap"]
        unsafe fn qvariantAs(
            variant: &QVariant,
            example: *mut QMap_QString_QVariant,
        ) -> Result<QMap_QString_QVariant>;

        #[rust_name = "qvariantmap_to_qvariant"]
        unsafe fn asQVariant(obj: &QMap_QString_QVariant) -> Result<QVariant>;

        #[rust_name = "qvariant_convertible_to_qstringlist"]
        unsafe fn qvariantConvertibleTo(
            obj: &QVariant,
            example: *mut QList_QString,
        ) -> Result<bool>;

        #[rust_name = "qvariant_to_qstringlist"]
        unsafe fn qvariantAs(
            variant: &QVariant,
            example: *mut QList_QString,
        ) -> Result<QList_QString>;

        #[rust_name = "qstringlist_to_qvariant"]
        unsafe fn asQVariant(obj: &QList_QString) -> Result<QVariant>;

        #[rust_name = "qvariant_convertible_to_qvariantlist"]
        unsafe fn qvariantConvertibleTo(
            obj: &QVariant,
            example: *mut QList_QVariant,
        ) -> Result<bool>;

        #[rust_name = "qvariant_to_qvariantlist"]
        unsafe fn qvariantAs(
            variant: &QVariant,
            example: *mut QList_QVariant,
        ) -> Result<QList_QVariant>;

        #[rust_name = "qvariantlist_to_qvariant"]
        unsafe fn asQVariant(obj: &QList_QVariant) -> Result<QVariant>;

        #[rust_name = "qvariant_convertible_to_qlist_i32"]
        unsafe fn qvariantConvertibleTo(obj: &QVariant, example: *mut QList_i32) -> Result<bool>;

        #[rust_name = "qvariant_to_qlist_i32"]
        unsafe fn qvariantAs(variant: &QVariant, example: *mut QList_i32) -> Result<QList_i32>;

        #[rust_name = "qlist_i32_to_qvariant"]
        unsafe fn asQVariant(obj: &QList_i32) -> Result<QVariant>;

        #[rust_name = "qvariant_convertible_to_qlist_f32"]
        unsafe fn qvariantConvertibleTo(obj: &QVariant, example: *mut QList_f32) -> Result<bool>;

        #[rust_name = "qvariant_to_qlist_f32"]
        unsafe fn qvariantAs(variant: &QVariant, example: *mut QList_f32) -> Result<QList_f32>;

        #[rust_name = "qlist_f32_to_qvariant"]
        unsafe fn asQVariant(obj: &QList_f32) -> Result<QVariant>;

        #[rust_name = "qvariant_convertible_to_qsharedpointer_qobject"]
        unsafe fn qvariantConvertibleTo(
            obj: &QVariant,
            example: *mut QSharedPointer_QObject,
        ) -> Result<bool>;

        #[rust_name = "qvariant_to_qsharedpointer_qobject"]
        unsafe fn qvariantAsUniquePtr(
            variant: &QVariant,
            example: *mut QSharedPointer_QObject,
        ) -> Result<UniquePtr<QSharedPointer_QObject>>;

        #[rust_name = "qsharedpointer_qobject_to_qvariant"]
        unsafe fn asQVariant(obj: &QSharedPointer_QObject) -> Result<QVariant>;

        #[rust_name = "qvariant_convertible_to_qweakpointer_qobject"]
        unsafe fn qvariantConvertibleTo(
            obj: &QVariant,
            example: *mut QWeakPointer_QObject,
        ) -> Result<bool>;

        #[rust_name = "qvariant_to_qweakpointer_qobject"]
        unsafe fn qvariantAsUniquePtr(
            variant: &QVariant,
            example: *mut QWeakPointer_QObject,
        ) -> Result<UniquePtr<QWeakPointer_QObject>>;

        #[rust_name = "qweakpointer_qobject_to_qvariant"]
        unsafe fn asQVariant(obj: &QWeakPointer_QObject) -> Result<QVariant>;
    }
}

use ffi::*;
pub use ffi::{qvariant_type_id, qvariant_type_name, QList_QVariant};

pub fn qvariant_convertible_to_qobject_ptr(obj: &QVariant) -> Result<bool, cxx::Exception> {
    unsafe { ffi::qvariant_convertible_to_qobject_ptr(obj, std::ptr::null_mut()) }
}

pub fn qvariant_to_qobject_ptr(obj: &QVariant) -> Result<*mut QObject, cxx::Exception> {
    unsafe { ffi::qvariant_to_qobject_ptr(obj, std::ptr::null_mut()) }
}

pub fn qobject_ptr_to_qvariant(obj: &*mut QObject) -> Result<QVariant, cxx::Exception> {
    unsafe { ffi::qobject_ptr_to_qvariant(obj) }
}

pub fn qvariant_convertible_to_qvariantmap(obj: &QVariant) -> Result<bool, cxx::Exception> {
    unsafe { ffi::qvariant_convertible_to_qvariantmap(obj, std::ptr::null_mut()) }
}

pub fn qvariant_to_qvariantmap(obj: &QVariant) -> Result<QMap_QString_QVariant, cxx::Exception> {
    unsafe { ffi::qvariant_to_qvariantmap(obj, std::ptr::null_mut()) }
}

pub fn qvariantmap_to_qvariant(obj: &QMap_QString_QVariant) -> Result<QVariant, cxx::Exception> {
    unsafe { ffi::qvariantmap_to_qvariant(obj) }
}

pub fn qvariant_convertible_to_qstringlist(obj: &QVariant) -> Result<bool, cxx::Exception> {
    unsafe { ffi::qvariant_convertible_to_qstringlist(obj, std::ptr::null_mut()) }
}

pub fn qvariant_to_qstringlist(obj: &QVariant) -> Result<QList_QString, cxx::Exception> {
    unsafe { ffi::qvariant_to_qstringlist(obj, std::ptr::null_mut()) }
}

pub fn qstringlist_to_qvariant(obj: &QList_QString) -> Result<QVariant, cxx::Exception> {
    unsafe { ffi::qstringlist_to_qvariant(obj) }
}

pub fn qvariant_convertible_to_qvariantlist(obj: &QVariant) -> Result<bool, cxx::Exception> {
    unsafe { ffi::qvariant_convertible_to_qvariantlist(obj, std::ptr::null_mut()) }
}

pub fn qvariant_to_qvariantlist(obj: &QVariant) -> Result<QList_QVariant, cxx::Exception> {
    unsafe { ffi::qvariant_to_qvariantlist(obj, std::ptr::null_mut()) }
}

pub fn qvariantlist_to_qvariant(obj: &QList_QVariant) -> Result<QVariant, cxx::Exception> {
    unsafe { ffi::qvariantlist_to_qvariant(obj) }
}

pub fn qvariant_convertible_to_qlist_i32(obj: &QVariant) -> Result<bool, cxx::Exception> {
    unsafe { ffi::qvariant_convertible_to_qlist_i32(obj, std::ptr::null_mut()) }
}

pub fn qvariant_to_qlist_i32(obj: &QVariant) -> Result<QList_i32, cxx::Exception> {
    unsafe { ffi::qvariant_to_qlist_i32(obj, std::ptr::null_mut()) }
}

pub fn qlist_i32_to_qvariant(obj: &QList_i32) -> Result<QVariant, cxx::Exception> {
    unsafe { ffi::qlist_i32_to_qvariant(obj) }
}

pub fn qvariant_convertible_to_qlist_f32(obj: &QVariant) -> Result<bool, cxx::Exception> {
    unsafe { ffi::qvariant_convertible_to_qlist_f32(obj, std::ptr::null_mut()) }
}

pub fn qvariant_to_qlist_f32(obj: &QVariant) -> Result<QList_f32, cxx::Exception> {
    unsafe { ffi::qvariant_to_qlist_f32(obj, std::ptr::null_mut()) }
}

pub fn qlist_f32_to_qvariant(obj: &QList_f32) -> Result<QVariant, cxx::Exception> {
    unsafe { ffi::qlist_f32_to_qvariant(obj) }
}

pub fn qvariant_convertible_to_qsharedpointer_qobject(
    obj: &QVariant,
) -> Result<bool, cxx::Exception> {
    unsafe { ffi::qvariant_convertible_to_qsharedpointer_qobject(obj, std::ptr::null_mut()) }
}

pub fn qvariant_to_qsharedpointer_qobject(
    obj: &QVariant,
) -> Result<cxx::UniquePtr<QSharedPointer_QObject>, cxx::Exception> {
    unsafe { ffi::qvariant_to_qsharedpointer_qobject(obj, std::ptr::null_mut()) }
}

pub fn qsharedpointer_qobject_to_qvariant(
    obj: &QSharedPointer_QObject,
) -> Result<QVariant, cxx::Exception> {
    unsafe { ffi::qsharedpointer_qobject_to_qvariant(obj) }
}

pub fn qvariant_convertible_to_qweakpointer_qobject(
    obj: &QVariant,
) -> Result<bool, cxx::Exception> {
    unsafe { ffi::qvariant_convertible_to_qweakpointer_qobject(obj, std::ptr::null_mut()) }
}

pub fn qvariant_to_qweakpointer_qobject(
    obj: &QVariant,
) -> Result<cxx::UniquePtr<QWeakPointer_QObject>, cxx::Exception> {
    unsafe { ffi::qvariant_to_qweakpointer_qobject(obj, std::ptr::null_mut()) }
}

pub fn qweakpointer_qobject_to_qvariant(
    obj: &QWeakPointer_QObject,
) -> Result<QVariant, cxx::Exception> {
    unsafe { ffi::qweakpointer_qobject_to_qvariant(obj) }
}
