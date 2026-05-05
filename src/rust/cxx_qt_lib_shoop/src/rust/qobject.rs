use anyhow::anyhow;
use std::pin::Pin;

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qobject.h");
        include!("cxx-qt-lib/qstring.h");
        include!("cxx-qt-lib-shoop/qthread.h");
        type ShoopQObject;
        type QString = cxx_qt_lib::QString;
        type QThread = crate::qthread::QThread;

        include!("cxx-qt-lib/qmap.h");
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;

        include!("cxx-qt-lib/qlist.h");
        type QList_QVariant = cxx_qt_lib::QList<QVariant>;

        #[rust_name = "qobject_set_parent"]
        unsafe fn qobjectSetParent(item: *mut ShoopQObject, parent: *mut ShoopQObject) -> Result<()>;

        #[rust_name = "qobject_parent"]
        unsafe fn qobjectParent(item: &ShoopQObject) -> Result<*mut ShoopQObject>;

        #[rust_name = "qobject_thread"]
        unsafe fn qobjectThread(item: &ShoopQObject) -> Result<*mut QThread>;

        #[rust_name = "qobject_class_name"]
        unsafe fn qobjectClassName(obj: &ShoopQObject) -> Result<&str>;

        #[rust_name = "qobject_property_bool"]
        unsafe fn qobjectPropertyBool(obj: &ShoopQObject, name: String) -> Result<bool>;

        #[rust_name = "qobject_property_int"]
        unsafe fn qobjectPropertyInt(obj: &ShoopQObject, name: String) -> Result<i32>;

        #[rust_name = "qobject_property_float"]
        unsafe fn qobjectPropertyFloat(obj: &ShoopQObject, name: String) -> Result<f32>;

        #[rust_name = "qobject_property_string"]
        unsafe fn qobjectPropertyString(obj: &ShoopQObject, name: String) -> Result<QString>;

        #[rust_name = "qobject_property_qobject"]
        unsafe fn qobjectPropertyQObject(obj: &ShoopQObject, name: String) -> Result<*mut ShoopQObject>;

        #[rust_name = "qobject_object_name"]
        unsafe fn qobjectObjectName(obj: &ShoopQObject) -> Result<String>;

        #[rust_name = "qobject_set_object_name"]
        unsafe fn qobjectSetObjectName(obj: *mut ShoopQObject, name: String) -> Result<()>;

        #[rust_name = "qobject_move_to_thread"]
        unsafe fn qobjectMoveToThread(obj: *mut ShoopQObject, thread: *mut QThread) -> Result<bool>;

        #[rust_name = "qobject_property_qvariant"]
        unsafe fn qobjectPropertyQVariant(obj: &ShoopQObject, name: String) -> Result<QVariant>;

        #[rust_name = "qobject_set_property_int"]
        unsafe fn qobjectSetProperty(
            obj: *mut ShoopQObject,
            property: String,
            value: &i32,
        ) -> Result<()>;

        #[rust_name = "qobject_set_property_bool"]
        unsafe fn qobjectSetProperty(
            obj: *mut ShoopQObject,
            property: String,
            value: &bool,
        ) -> Result<()>;

        #[rust_name = "qobject_set_property_qvariantlist"]
        unsafe fn qobjectSetProperty(
            obj: *mut ShoopQObject,
            property: String,
            value: &QList_QVariant,
        ) -> Result<()>;

        #[rust_name = "qobject_find_child"]
        unsafe fn qobjectFindChild(obj: *mut ShoopQObject, name: String) -> Result<*mut ShoopQObject>;

        include!("cxx-qt-lib-shoop/register_qml_type.h");

        #[rust_name = "qobject_register_qml_singleton_instance"]
        unsafe fn register_qml_singleton_instance(
            instance: *mut ShoopQObject,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        ) -> Result<()>;

        #[rust_name = "qobject_meta_type_name"]
        unsafe fn qobjectMetaTypeName(obj: &ShoopQObject) -> Result<String>;

        #[rust_name = "qobject_has_property"]
        unsafe fn qobjectHasProperty(obj: &ShoopQObject, property: String) -> Result<bool>;
    }
}

pub use ffi::ShoopQObject;
use ffi::*;

// Should be implemented by each QObject derivative explicitly
pub trait AsQObject {
    // To be implemented per type
    unsafe fn mut_qobject_ptr(&mut self) -> *mut ShoopQObject;
    unsafe fn ref_qobject_ptr(&self) -> *const ShoopQObject;

    unsafe fn pin_mut_qobject_ptr(self: Pin<&mut Self>) -> *mut ShoopQObject {
        let obj = self.get_unchecked_mut();
        Self::mut_qobject_ptr(obj)
    }

    unsafe fn qobject_ref(self: &Self) -> &ShoopQObject {
        let obj = self.ref_qobject_ptr();
        &*obj
    }

    unsafe fn qobject_mut(self: &mut Self) -> &mut ShoopQObject {
        let obj = self.mut_qobject_ptr();
        if obj.is_null() {
            panic!("qobject_mut: null pointer");
        }
        &mut *obj
    }
}

pub trait FromQObject {
    // to be implemented per type
    unsafe fn ptr_from_qobject_ref(obj: &ShoopQObject) -> *const Self;
    unsafe fn ptr_from_qobject_mut(obj: Pin<&mut ShoopQObject>) -> *mut Self;

    unsafe fn from_qobject_ref(obj: &ShoopQObject) -> Result<&Self, anyhow::Error> {
        let ptr = Self::ptr_from_qobject_ref(obj);
        if ptr.is_null() {
            return Err(anyhow!("failed conversion"));
        }
        Ok(&*ptr)
    }

    unsafe fn from_qobject_mut(obj: Pin<&mut ShoopQObject>) -> Result<Pin<&mut Self>, anyhow::Error> {
        let ptr = Self::ptr_from_qobject_mut(obj);
        if ptr.is_null() {
            return Err(anyhow!("failed conversion"));
        }
        let pin = std::pin::Pin::new_unchecked(&mut *ptr);
        Ok(pin)
    }

    unsafe fn from_qobject_ref_ptr<'a>(obj: *const ShoopQObject) -> Result<&'a Self, anyhow::Error> {
        Self::from_qobject_ref(&*obj)
    }

    unsafe fn from_qobject_mut_ptr<'a>(
        obj: *mut ShoopQObject,
    ) -> Result<Pin<&'a mut Self>, anyhow::Error> {
        Self::from_qobject_mut(std::pin::Pin::new_unchecked(&mut *obj))
    }
}

pub unsafe fn qobject_parent(obj: &ShoopQObject) -> Result<*mut ShoopQObject, cxx::Exception> {
    ffi::qobject_parent(obj)
}
pub unsafe fn qobject_set_parent(
    obj: *mut ShoopQObject,
    parent: *mut ShoopQObject,
) -> Result<(), cxx::Exception> {
    ffi::qobject_set_parent(obj, parent)
}
pub unsafe fn qobject_thread(obj: &ShoopQObject) -> Result<*mut QThread, cxx::Exception> {
    ffi::qobject_thread(obj)
}
pub unsafe fn qobject_class_name(obj: &ShoopQObject) -> Result<&str, cxx::Exception> {
    ffi::qobject_class_name(obj)
}
pub unsafe fn qobject_meta_type_name(obj: &ShoopQObject) -> Result<String, cxx::Exception> {
    ffi::qobject_meta_type_name(obj)
}
pub unsafe fn qobject_property_bool(obj: &ShoopQObject, name: &str) -> Result<bool, cxx::Exception> {
    ffi::qobject_property_bool(obj, name.to_string())
}
pub unsafe fn qobject_property_int(obj: &ShoopQObject, name: &str) -> Result<i32, cxx::Exception> {
    ffi::qobject_property_int(obj, name.to_string())
}
pub unsafe fn qobject_property_float(obj: &ShoopQObject, name: &str) -> Result<f32, cxx::Exception> {
    ffi::qobject_property_float(obj, name.to_string())
}
pub unsafe fn qobject_property_qobject(
    obj: &ShoopQObject,
    name: &str,
) -> Result<*mut ShoopQObject, cxx::Exception> {
    ffi::qobject_property_qobject(obj, name.to_string())
}
pub unsafe fn qobject_property_string(
    obj: &ShoopQObject,
    name: &str,
) -> Result<QString, cxx::Exception> {
    ffi::qobject_property_string(obj, name.to_string())
}
pub unsafe fn qobject_property_qvariant(
    obj: &ShoopQObject,
    name: &str,
) -> Result<QVariant, cxx::Exception> {
    ffi::qobject_property_qvariant(obj, name.to_string())
}
pub unsafe fn qobject_object_name(obj: &ShoopQObject) -> Result<String, cxx::Exception> {
    ffi::qobject_object_name(obj)
}
pub unsafe fn qobject_set_object_name(obj: *mut ShoopQObject, name: &str) -> Result<(), cxx::Exception> {
    ffi::qobject_set_object_name(obj, name.to_string())
}
pub unsafe fn qobject_move_to_thread(
    obj: *mut ShoopQObject,
    thread: *mut QThread,
) -> Result<bool, cxx::Exception> {
    ffi::qobject_move_to_thread(obj, thread)
}
pub unsafe fn qobject_set_property_int(
    obj: *mut ShoopQObject,
    property: &str,
    value: &i32,
) -> Result<(), cxx::Exception> {
    ffi::qobject_set_property_int(obj, property.to_string(), value)
}
pub unsafe fn qobject_set_property_bool(
    obj: *mut ShoopQObject,
    property: &str,
    value: &bool,
) -> Result<(), cxx::Exception> {
    ffi::qobject_set_property_bool(obj, property.to_string(), value)
}
pub unsafe fn qobject_set_property_qvariantlist(
    obj: *mut ShoopQObject,
    property: &str,
    value: &QList_QVariant,
) -> Result<(), cxx::Exception> {
    ffi::qobject_set_property_qvariantlist(obj, property.to_string(), value)
}
pub unsafe fn qobject_find_child(
    obj: *mut ShoopQObject,
    name: &str,
) -> Result<*mut ShoopQObject, cxx::Exception> {
    ffi::qobject_find_child(obj, name.to_string())
}
pub unsafe fn qobject_register_qml_singleton_instance(
    instance: *mut ShoopQObject,
    module_name: &str,
    version_major: i64,
    version_minor: i64,
    type_name: &str,
) -> Result<(), cxx::Exception> {
    ffi::qobject_register_qml_singleton_instance(
        instance,
        &mut module_name.to_string(),
        version_major,
        version_minor,
        &mut type_name.to_string(),
    )
}
pub unsafe fn qobject_has_property(obj: &ShoopQObject, property: &str) -> Result<bool, cxx::Exception> {
    ffi::qobject_has_property(obj, property.to_string())
}

pub trait IsQObject: AsQObject {
    unsafe fn set_parent(self: Pin<&mut Self>, parent: *mut ShoopQObject) -> Result<(), cxx::Exception> {
        let self_item = self.pin_mut_qobject_ptr();
        qobject_set_parent(self_item, parent)
    }

    unsafe fn parent(self: Pin<&mut Self>) -> Result<*mut ShoopQObject, cxx::Exception> {
        let self_ptr = self.qobject_ref();
        qobject_parent(self_ptr)
    }

    unsafe fn class_name(self: &Self) -> Result<&str, cxx::Exception> {
        let obj = self.qobject_ref();
        qobject_class_name(obj)
    }

    unsafe fn property_bool(self: &Self, name: &str) -> Result<bool, cxx::Exception> {
        let obj = self.qobject_ref();
        qobject_property_bool(obj, name)
    }

    unsafe fn property_int(self: &Self, name: &str) -> Result<i32, cxx::Exception> {
        let obj = self.qobject_ref();
        qobject_property_int(obj, name)
    }

    unsafe fn property_string(self: &Self, name: &str) -> Result<QString, cxx::Exception> {
        let obj = self.qobject_ref();
        qobject_property_string(obj, name)
    }

    unsafe fn property_qvariant(self: &Self, name: &str) -> Result<QVariant, cxx::Exception> {
        let obj = self.qobject_ref();
        qobject_property_qvariant(obj, name)
    }

    unsafe fn object_name(self: &Self) -> Result<String, cxx::Exception> {
        let obj = self.qobject_ref();
        qobject_object_name(obj)
    }

    unsafe fn set_object_name(self: Pin<&mut Self>, name: &str) -> Result<(), cxx::Exception> {
        let self_item = self.pin_mut_qobject_ptr();
        qobject_set_object_name(self_item, name)
    }

    unsafe fn set_property_int(
        self: Pin<&mut Self>,
        property: &str,
        value: &i32,
    ) -> Result<(), cxx::Exception> {
        let self_item = self.pin_mut_qobject_ptr();
        qobject_set_property_int(self_item, property, value)
    }

    unsafe fn set_property_bool(
        self: Pin<&mut Self>,
        property: &str,
        value: &bool,
    ) -> Result<(), cxx::Exception> {
        let self_item = self.pin_mut_qobject_ptr();
        qobject_set_property_bool(self_item, property, value)
    }

    unsafe fn find_child(self: Pin<&mut Self>, name: &str) -> Result<*mut ShoopQObject, cxx::Exception> {
        let self_item = self.pin_mut_qobject_ptr();
        qobject_find_child(self_item, name)
    }
}