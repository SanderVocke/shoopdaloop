use std::pin::Pin;

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qobject.h");
        include!("cxx-qt-lib/qstring.h");
        include!("cxx-qt-lib-shoop/qthread.h");
        type QObject;
        type QString = cxx_qt_lib::QString;
        type QThread = crate::qthread::QThread;

        include!("cxx-qt-lib/qmap.h");
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;

        #[rust_name = "qobject_set_parent"]
        unsafe fn qobjectSetParent(item: *mut QObject, parent: *mut QObject) -> Result<()>;

        #[rust_name = "qobject_parent"]
        unsafe fn qobjectParent(item: &QObject) -> Result<*mut QObject>;

        #[rust_name = "qobject_thread"]
        unsafe fn qobjectThread(item: &QObject) -> Result<*mut QThread>;

        #[rust_name = "qobject_class_name"]
        unsafe fn qobjectClassName(obj: &QObject) -> Result<&str>;

        #[rust_name = "qobject_property_bool"]
        unsafe fn qobjectPropertyBool(obj: &QObject, name: String) -> Result<bool>;

        #[rust_name = "qobject_property_int"]
        unsafe fn qobjectPropertyInt(obj: &QObject, name: String) -> Result<i32>;

        #[rust_name = "qobject_property_float"]
        unsafe fn qobjectPropertyFloat(obj: &QObject, name: String) -> Result<f32>;

        #[rust_name = "qobject_property_string"]
        unsafe fn qobjectPropertyString(obj: &QObject, name: String) -> Result<QString>;

        #[rust_name = "qobject_object_name"]
        unsafe fn qobjectObjectName(obj: &QObject) -> Result<String>;

        #[rust_name = "qobject_set_object_name"]
        unsafe fn qobjectSetObjectName(obj: *mut QObject, name: String) -> Result<()>;

        #[rust_name = "qobject_move_to_thread"]
        unsafe fn qobjectMoveToThread(obj: *mut QObject, thread: *mut QThread) -> Result<bool>;

        #[rust_name = "qobject_property_qvariant"]
        unsafe fn qobjectPropertyQVariant(obj: &QObject, name: String) -> Result<QVariant>;

        #[rust_name = "qobject_set_property_int"]
        unsafe fn qobjectSetProperty(
            obj: *mut QObject,
            property: String,
            value: &i32,
        ) -> Result<()>;

        #[rust_name = "qobject_set_property_bool"]
        unsafe fn qobjectSetProperty(
            obj: *mut QObject,
            property: String,
            value: &bool,
        ) -> Result<()>;

        #[rust_name = "qobject_find_child"]
        unsafe fn qobjectFindChild(obj: *mut QObject, name: String) -> Result<*mut QObject>;

        include!("cxx-qt-lib-shoop/register_qml_type.h");

        #[rust_name = "qobject_register_qml_singleton_instance"]
        unsafe fn register_qml_singleton_instance(
            instance: *mut QObject,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        ) -> Result<()>;
    }
}

pub use ffi::QObject;
use ffi::*;

// Should be implemented by each QObject derivative explicitly
pub trait AsQObject {
    // To be implemented per type
    unsafe fn mut_qobject_ptr(&mut self) -> *mut QObject;
    unsafe fn ref_qobject_ptr(&self) -> *const QObject;

    unsafe fn pin_mut_qobject_ptr(self: Pin<&mut Self>) -> *mut QObject {
        let obj = self.get_unchecked_mut();
        Self::mut_qobject_ptr(obj)
    }

    unsafe fn qobject_ref(self: &Self) -> &QObject {
        let obj = self.ref_qobject_ptr();
        &*obj
    }

    unsafe fn qobject_mut(self: &mut Self) -> &mut QObject {
        let obj = self.mut_qobject_ptr();
        if obj.is_null() {
            panic!("qobject_mut: null pointer");
        }
        &mut *obj
    }
}

pub unsafe fn qobject_parent(obj: &QObject) -> Result<*mut QObject, cxx::Exception> {
    ffi::qobject_parent(obj)
}
pub unsafe fn qobject_set_parent(
    obj: *mut QObject,
    parent: *mut QObject,
) -> Result<(), cxx::Exception> {
    ffi::qobject_set_parent(obj, parent)
}
pub unsafe fn qobject_thread(obj: &QObject) -> Result<*mut QThread, cxx::Exception> {
    ffi::qobject_thread(obj)
}
pub unsafe fn qobject_class_name(obj: &QObject) -> Result<&str, cxx::Exception> {
    ffi::qobject_class_name(obj)
}
pub unsafe fn qobject_property_bool(obj: &QObject, name: String) -> Result<bool, cxx::Exception> {
    ffi::qobject_property_bool(obj, name)
}
pub unsafe fn qobject_property_int(obj: &QObject, name: String) -> Result<i32, cxx::Exception> {
    ffi::qobject_property_int(obj, name)
}
pub unsafe fn qobject_property_float(obj: &QObject, name: String) -> Result<f32, cxx::Exception> {
    ffi::qobject_property_float(obj, name)
}
pub unsafe fn qobject_property_string(
    obj: &QObject,
    name: String,
) -> Result<QString, cxx::Exception> {
    ffi::qobject_property_string(obj, name)
}
pub unsafe fn qobject_property_qvariant(
    obj: &QObject,
    name: String,
) -> Result<QVariant, cxx::Exception> {
    ffi::qobject_property_qvariant(obj, name)
}
pub unsafe fn qobject_object_name(obj: &QObject) -> Result<String, cxx::Exception> {
    ffi::qobject_object_name(obj)
}
pub unsafe fn qobject_set_object_name(
    obj: *mut QObject,
    name: String,
) -> Result<(), cxx::Exception> {
    ffi::qobject_set_object_name(obj, name)
}
pub unsafe fn qobject_move_to_thread(
    obj: *mut QObject,
    thread: *mut QThread,
) -> Result<bool, cxx::Exception> {
    ffi::qobject_move_to_thread(obj, thread)
}
pub unsafe fn qobject_set_property_int(
    obj: *mut QObject,
    property: String,
    value: &i32,
) -> Result<(), cxx::Exception> {
    ffi::qobject_set_property_int(obj, property, value)
}
pub unsafe fn qobject_set_property_bool(
    obj: *mut QObject,
    property: String,
    value: &bool,
) -> Result<(), cxx::Exception> {
    ffi::qobject_set_property_bool(obj, property, value)
}
pub unsafe fn qobject_find_child(
    obj: *mut QObject,
    name: String,
) -> Result<*mut QObject, cxx::Exception> {
    ffi::qobject_find_child(obj, name)
}
pub unsafe fn qobject_register_qml_singleton_instance(
    instance: *mut QObject,
    module_name: &mut String,
    version_major: i64,
    version_minor: i64,
    type_name: &mut String,
) -> Result<(), cxx::Exception> {
    ffi::qobject_register_qml_singleton_instance(
        instance,
        module_name,
        version_major,
        version_minor,
        type_name,
    )
}

pub trait IsQObject: AsQObject {
    unsafe fn set_parent(self: Pin<&mut Self>, parent: *mut QObject) -> Result<(), cxx::Exception> {
        let self_item = self.pin_mut_qobject_ptr();
        qobject_set_parent(self_item, parent)
    }

    unsafe fn parent(self: Pin<&mut Self>) -> Result<*mut QObject, cxx::Exception> {
        let self_ptr = self.qobject_ref();
        qobject_parent(self_ptr)
    }

    unsafe fn class_name(self: &Self) -> Result<&str, cxx::Exception> {
        let obj = self.qobject_ref();
        qobject_class_name(obj)
    }

    unsafe fn property_bool(self: &Self, name: String) -> Result<bool, cxx::Exception> {
        let obj = self.qobject_ref();
        qobject_property_bool(obj, name)
    }

    unsafe fn property_int(self: &Self, name: String) -> Result<i32, cxx::Exception> {
        let obj = self.qobject_ref();
        qobject_property_int(obj, name)
    }

    unsafe fn property_string(self: &Self, name: String) -> Result<QString, cxx::Exception> {
        let obj = self.qobject_ref();
        qobject_property_string(obj, name)
    }

    unsafe fn property_qvariant(self: &Self, name: String) -> Result<QVariant, cxx::Exception> {
        let obj = self.qobject_ref();
        qobject_property_qvariant(obj, name)
    }

    unsafe fn object_name(self: &Self) -> Result<String, cxx::Exception> {
        let obj = self.qobject_ref();
        qobject_object_name(obj)
    }

    unsafe fn set_object_name(self: Pin<&mut Self>, name: String) -> Result<(), cxx::Exception> {
        let self_item = self.pin_mut_qobject_ptr();
        qobject_set_object_name(self_item, name)
    }

    unsafe fn set_property_int(
        self: Pin<&mut Self>,
        property: String,
        value: &i32,
    ) -> Result<(), cxx::Exception> {
        let self_item = self.pin_mut_qobject_ptr();
        qobject_set_property_int(self_item, property, value)
    }

    unsafe fn set_property_bool(
        self: Pin<&mut Self>,
        property: String,
        value: &bool,
    ) -> Result<(), cxx::Exception> {
        let self_item = self.pin_mut_qobject_ptr();
        qobject_set_property_bool(self_item, property, value)
    }

    unsafe fn find_child(
        self: Pin<&mut Self>,
        name: String,
    ) -> Result<*mut QObject, cxx::Exception> {
        let self_item = self.pin_mut_qobject_ptr();
        qobject_find_child(self_item, name)
    }
}
