use anyhow::anyhow;
use std::pin::Pin;

// Re-export cxx_qt::QObject as our QObject type
pub use cxx_qt::QObject;

#[cxx::bridge]
pub mod ffi {
    unsafe extern "C++" {
        // Include Qt's QObject header - this gives us the QObject type
        include!(<QtCore/QObject>);
        // Include our helper header - this gives us the helper functions
        include!("cxx-qt-lib-shoop/qobject.h");

        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib-shoop/qthread.h");
        type QThread = crate::qthread::QThread;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qmap.h");
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;

        include!("cxx-qt-lib/qlist.h");
        type QList_QVariant = cxx_qt_lib::QList<QVariant>;

        // Use a different name to avoid conflict with cxx_qt::QObject
        #[rust_name = "QObjectHelper"]
        type QObject;

        #[rust_name = "qobject_set_parent"]
        unsafe fn qobjectSetParent(
            item: *mut QObjectHelper,
            parent: *mut QObjectHelper,
        ) -> Result<()>;

        #[rust_name = "qobject_parent"]
        unsafe fn qobjectParent(item: &QObjectHelper) -> Result<*mut QObjectHelper>;

        #[rust_name = "qobject_thread"]
        unsafe fn qobjectThread(item: &QObjectHelper) -> Result<*mut QThread>;

        #[rust_name = "qobject_class_name"]
        unsafe fn qobjectClassName(obj: &QObjectHelper) -> Result<&str>;

        #[rust_name = "qobject_property_bool"]
        unsafe fn qobjectPropertyBool(obj: &QObjectHelper, name: String) -> Result<bool>;

        #[rust_name = "qobject_property_int"]
        unsafe fn qobjectPropertyInt(obj: &QObjectHelper, name: String) -> Result<i32>;

        #[rust_name = "qobject_property_float"]
        unsafe fn qobjectPropertyFloat(obj: &QObjectHelper, name: String) -> Result<f32>;

        #[rust_name = "qobject_property_string"]
        unsafe fn qobjectPropertyString(obj: &QObjectHelper, name: String) -> Result<QString>;

        #[rust_name = "qobject_property_qobject"]
        unsafe fn qobjectPropertyQObject(
            obj: &QObjectHelper,
            name: String,
        ) -> Result<*mut QObjectHelper>;

        #[rust_name = "qobject_object_name"]
        unsafe fn qobjectObjectName(obj: &QObjectHelper) -> Result<String>;

        #[rust_name = "qobject_set_object_name"]
        unsafe fn qobjectSetObjectName(obj: *mut QObjectHelper, name: String) -> Result<()>;

        #[rust_name = "qobject_move_to_thread"]
        unsafe fn qobjectMoveToThread(
            obj: *mut QObjectHelper,
            thread: *mut QThread,
        ) -> Result<bool>;

        #[rust_name = "qobject_property_qvariant"]
        unsafe fn qobjectPropertyQVariant(obj: &QObjectHelper, name: String) -> Result<QVariant>;

        #[rust_name = "qobject_set_property_int"]
        unsafe fn qobjectSetProperty(
            obj: *mut QObjectHelper,
            property: String,
            value: &i32,
        ) -> Result<()>;

        #[rust_name = "qobject_set_property_bool"]
        unsafe fn qobjectSetProperty(
            obj: *mut QObjectHelper,
            property: String,
            value: &bool,
        ) -> Result<()>;

        #[rust_name = "qobject_set_property_qvariantlist"]
        unsafe fn qobjectSetProperty(
            obj: *mut QObjectHelper,
            property: String,
            value: &QList_QVariant,
        ) -> Result<()>;

        #[rust_name = "qobject_find_child"]
        unsafe fn qobjectFindChild(
            obj: *mut QObjectHelper,
            name: String,
        ) -> Result<*mut QObjectHelper>;

        include!("cxx-qt-lib-shoop/register_qml_type.h");

        #[rust_name = "qobject_register_qml_singleton_instance"]
        unsafe fn register_qml_singleton_instance(
            instance: *mut QObjectHelper,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        ) -> Result<()>;

        #[rust_name = "qobject_meta_type_name"]
        unsafe fn qobjectMetaTypeName(obj: &QObjectHelper) -> Result<String>;

        #[rust_name = "qobject_has_property"]
        unsafe fn qobjectHasProperty(obj: &QObjectHelper, property: String) -> Result<bool>;
    }
}

use ffi::{QList_QVariant, QObjectHelper, QString, QThread, QVariant};

// Trait for converting any QObject-derived type to QObject pointer
// This complements cxx_qt's Upcast trait but provides raw pointer access
pub trait AsQObject {
    // To be implemented per type - returns raw QObject pointer
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

// Trait for converting from QObject to a specific derived type (qobject_cast)
pub trait FromQObject {
    // to be implemented per type
    unsafe fn ptr_from_qobject_ref(obj: &QObject) -> *const Self;
    unsafe fn ptr_from_qobject_mut(obj: Pin<&mut QObject>) -> *mut Self;

    unsafe fn from_qobject_ref(obj: &QObject) -> Result<&Self, anyhow::Error> {
        let ptr = Self::ptr_from_qobject_ref(obj);
        if ptr.is_null() {
            return Err(anyhow!("failed conversion"));
        }
        Ok(&*ptr)
    }

    unsafe fn from_qobject_mut(obj: Pin<&mut QObject>) -> Result<Pin<&mut Self>, anyhow::Error> {
        let ptr = Self::ptr_from_qobject_mut(obj);
        if ptr.is_null() {
            return Err(anyhow!("failed conversion"));
        }
        let pin = std::pin::Pin::new_unchecked(&mut *ptr);
        Ok(pin)
    }

    unsafe fn from_qobject_ref_ptr<'a>(obj: *const QObject) -> Result<&'a Self, anyhow::Error> {
        Self::from_qobject_ref(&*obj)
    }

    unsafe fn from_qobject_mut_ptr<'a>(
        obj: *mut QObject,
    ) -> Result<Pin<&'a mut Self>, anyhow::Error> {
        Self::from_qobject_mut(std::pin::Pin::new_unchecked(&mut *obj))
    }
}

// Helper functions that wrap the FFI calls
// These work with cxx_qt::QObject which is identical to QObjectHelper at the C++ level

pub unsafe fn qobject_parent(obj: &QObject) -> Result<*mut QObject, cxx::Exception> {
    // Cast QObject reference to QObjectHelper reference (they're the same C++ type)
    let obj_helper: &QObjectHelper = &*(obj as *const QObject as *const QObjectHelper);
    let parent = ffi::qobject_parent(obj_helper)?;
    // Cast back
    Ok(parent as *mut QObject)
}

pub unsafe fn qobject_set_parent(
    obj: *mut QObject,
    parent: *mut QObject,
) -> Result<(), cxx::Exception> {
    ffi::qobject_set_parent(obj as *mut QObjectHelper, parent as *mut QObjectHelper)
}

pub unsafe fn qobject_thread(obj: &QObject) -> Result<*mut QThread, cxx::Exception> {
    let obj_helper: &QObjectHelper = &*(obj as *const QObject as *const QObjectHelper);
    ffi::qobject_thread(obj_helper)
}

pub unsafe fn qobject_class_name(obj: &QObject) -> Result<&str, cxx::Exception> {
    let obj_helper: &QObjectHelper = &*(obj as *const QObject as *const QObjectHelper);
    ffi::qobject_class_name(obj_helper)
}

pub unsafe fn qobject_meta_type_name(obj: &QObject) -> Result<String, cxx::Exception> {
    let obj_helper: &QObjectHelper = &*(obj as *const QObject as *const QObjectHelper);
    ffi::qobject_meta_type_name(obj_helper)
}

pub unsafe fn qobject_property_bool(obj: &QObject, name: &str) -> Result<bool, cxx::Exception> {
    let obj_helper: &QObjectHelper = &*(obj as *const QObject as *const QObjectHelper);
    ffi::qobject_property_bool(obj_helper, name.to_string())
}

pub unsafe fn qobject_property_int(obj: &QObject, name: &str) -> Result<i32, cxx::Exception> {
    let obj_helper: &QObjectHelper = &*(obj as *const QObject as *const QObjectHelper);
    ffi::qobject_property_int(obj_helper, name.to_string())
}

pub unsafe fn qobject_property_float(obj: &QObject, name: &str) -> Result<f32, cxx::Exception> {
    let obj_helper: &QObjectHelper = &*(obj as *const QObject as *const QObjectHelper);
    ffi::qobject_property_float(obj_helper, name.to_string())
}

pub unsafe fn qobject_property_qobject(
    obj: &QObject,
    name: &str,
) -> Result<*mut QObject, cxx::Exception> {
    let obj_helper: &QObjectHelper = &*(obj as *const QObject as *const QObjectHelper);
    let result = ffi::qobject_property_qobject(obj_helper, name.to_string())?;
    Ok(result as *mut QObject)
}

pub unsafe fn qobject_property_string(
    obj: &QObject,
    name: &str,
) -> Result<QString, cxx::Exception> {
    let obj_helper: &QObjectHelper = &*(obj as *const QObject as *const QObjectHelper);
    ffi::qobject_property_string(obj_helper, name.to_string())
}

pub unsafe fn qobject_property_qvariant(
    obj: &QObject,
    name: &str,
) -> Result<QVariant, cxx::Exception> {
    let obj_helper: &QObjectHelper = &*(obj as *const QObject as *const QObjectHelper);
    ffi::qobject_property_qvariant(obj_helper, name.to_string())
}

pub unsafe fn qobject_object_name(obj: &QObject) -> Result<String, cxx::Exception> {
    let obj_helper: &QObjectHelper = &*(obj as *const QObject as *const QObjectHelper);
    ffi::qobject_object_name(obj_helper)
}

pub unsafe fn qobject_set_object_name(obj: *mut QObject, name: &str) -> Result<(), cxx::Exception> {
    ffi::qobject_set_object_name(obj as *mut QObjectHelper, name.to_string())
}

pub unsafe fn qobject_move_to_thread(
    obj: *mut QObject,
    thread: *mut QThread,
) -> Result<bool, cxx::Exception> {
    ffi::qobject_move_to_thread(obj as *mut QObjectHelper, thread)
}

pub unsafe fn qobject_set_property_int(
    obj: *mut QObject,
    property: &str,
    value: &i32,
) -> Result<(), cxx::Exception> {
    ffi::qobject_set_property_int(obj as *mut QObjectHelper, property.to_string(), value)
}

pub unsafe fn qobject_set_property_bool(
    obj: *mut QObject,
    property: &str,
    value: &bool,
) -> Result<(), cxx::Exception> {
    ffi::qobject_set_property_bool(obj as *mut QObjectHelper, property.to_string(), value)
}

pub unsafe fn qobject_set_property_qvariantlist(
    obj: *mut QObject,
    property: &str,
    value: &QList_QVariant,
) -> Result<(), cxx::Exception> {
    ffi::qobject_set_property_qvariantlist(obj as *mut QObjectHelper, property.to_string(), value)
}

pub unsafe fn qobject_find_child(
    obj: *mut QObject,
    name: &str,
) -> Result<*mut QObject, cxx::Exception> {
    let result = ffi::qobject_find_child(obj as *mut QObjectHelper, name.to_string())?;
    Ok(result as *mut QObject)
}

pub unsafe fn qobject_register_qml_singleton_instance(
    instance: *mut QObject,
    module_name: &str,
    version_major: i64,
    version_minor: i64,
    type_name: &str,
) -> Result<(), cxx::Exception> {
    ffi::qobject_register_qml_singleton_instance(
        instance as *mut QObjectHelper,
        &mut module_name.to_string(),
        version_major,
        version_minor,
        &mut type_name.to_string(),
    )
}

pub unsafe fn qobject_has_property(obj: &QObject, property: &str) -> Result<bool, cxx::Exception> {
    let obj_helper: &QObjectHelper = &*(obj as *const QObject as *const QObjectHelper);
    ffi::qobject_has_property(obj_helper, property.to_string())
}

// Convenience trait combining AsQObject with helper methods
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

    unsafe fn find_child(self: Pin<&mut Self>, name: &str) -> Result<*mut QObject, cxx::Exception> {
        let self_item = self.pin_mut_qobject_ptr();
        qobject_find_child(self_item, name)
    }
}
