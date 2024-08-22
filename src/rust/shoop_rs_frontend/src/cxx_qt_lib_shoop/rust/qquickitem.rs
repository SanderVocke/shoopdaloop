use std::pin::Pin;
use crate::cxx_qt_lib_shoop::qobject::{IsQObject, AsQObject};

#[cxx_qt::bridge(cxx_file_stem="qquickitem")]
mod ffi {

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qquickitem.h");
        type QQuickItem;
        type QObject = crate::cxx_qt_lib_shoop::qobject::QObject;

        #[rust_name = "qquickitem_set_parent_item"]
        unsafe fn qquickitemSetParentItem(item : *mut QQuickItem, parent : *mut QQuickItem);

        #[rust_name = "qquickitem_parent_item"]
        unsafe fn qquickitemParentItem(item : &QQuickItem) -> *mut QQuickItem;

        #[rust_name = "qquickitem_to_qobject_ptr"]
        unsafe fn qquickitemToQobjectPtr(item : *mut QQuickItem) -> *mut QObject;

        #[rust_name = "qquickitem_to_qobject_ref"]
        fn qquickitemToQobjectRef(item : &QQuickItem) -> &QObject;
    }

    extern "RustQt" {
        #[qobject]
        type DummyQQuickItem = super::DummyQQuickItemRust;
    }
}

#[derive(Default)]
pub struct DummyQQuickItemRust {}

pub use ffi::QQuickItem;
use ffi::QObject;

// Should be implemented by each QQuickItem derivative explicitly
pub trait AsQQuickItem {
    // To be implemented per type
    unsafe fn mut_qquickitem_ptr (&mut self) -> *mut QQuickItem;
    unsafe fn ref_qquickitem_ptr (&self) -> *const QQuickItem;

    unsafe fn pin_mut_qquickitem_ptr (self: Pin<&mut Self>) -> *mut QQuickItem {
        let obj = self.get_unchecked_mut();
        Self::mut_qquickitem_ptr(obj)
    }

    unsafe fn qquickitem_ref (self: &Self) -> &QQuickItem {
        let obj = self.ref_qquickitem_ptr();
        &*obj
    }

    unsafe fn qquickitem_mut (self: &mut Self) -> &mut QQuickItem {
        let obj = self.mut_qquickitem_ptr();
        &mut *obj
    }
}

pub unsafe fn qquickitem_parent_item(item: &QQuickItem) -> *mut QQuickItem
    { ffi::qquickitem_parent_item(item) }

pub unsafe fn qquickitem_set_parent_item(item: *mut QQuickItem, parent: *mut QQuickItem)
    { ffi::qquickitem_set_parent_item(item, parent); }

pub unsafe fn qquickitem_to_qobject_ref(item : &QQuickItem) -> &QObject
    { ffi::qquickitem_to_qobject_ref(item) }

pub unsafe fn qquickitem_to_qobject_mut(item : *mut QQuickItem) -> *mut QObject
    { ffi::qquickitem_to_qobject_ptr(item) }

impl<T> AsQObject for T
where
    T: AsQQuickItem,
{
    unsafe fn mut_qobject_ptr (self: &mut Self) -> *mut QObject {
        ffi::qquickitem_to_qobject_ptr(self.mut_qquickitem_ptr())
    }

    unsafe fn ref_qobject_ptr (self: &Self) -> *const QObject {
        ffi::qquickitem_to_qobject_ref(&*self.ref_qquickitem_ptr() as &QQuickItem) as *const QObject
    }
}

impl AsQObject for QQuickItem {
    unsafe fn mut_qobject_ptr (self: &mut Self) -> *mut QObject {
        ffi::qquickitem_to_qobject_ptr(self)
    }

    unsafe fn ref_qobject_ptr (self: &Self) -> *const QObject {
        ffi::qquickitem_to_qobject_ref(self) as *const QObject
    }
}

pub trait IsQQuickItem : AsQQuickItem {
    unsafe fn set_parent_item(self : Pin<&mut Self>, parent : *mut QQuickItem) {
        let self_item = self.pin_mut_qquickitem_ptr();
        qquickitem_set_parent_item(self_item, parent);
    }

    unsafe fn parent_item(self : &Self) -> *mut QQuickItem {
        qquickitem_parent_item(self.qquickitem_ref())
    }
}

impl<T> IsQObject for T
where
    T: IsQQuickItem,
{}
