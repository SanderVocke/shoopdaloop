use common::logging::macros::*;
shoop_log_unit!("Frontend.FindParentItem");

use crate::cxx_qt_lib_shoop::qobject::AsQObject;
pub use crate::cxx_qt_shoop::qobj_find_parent_item_bridge::constants::*;
pub use crate::cxx_qt_shoop::qobj_find_parent_item_bridge::ffi::make_unique_find_parent_item as make_unique;
pub use crate::cxx_qt_shoop::qobj_find_parent_item_bridge::FindParentItem;
use crate::cxx_qt_shoop::qobj_find_parent_item_bridge::*;

use crate::cxx_qt_lib_shoop::qobject::qobject_object_name;
use crate::cxx_qt_lib_shoop::qobject::{qobject_class_name, qobject_property_bool};
use crate::cxx_qt_lib_shoop::qquickitem;
use crate::cxx_qt_lib_shoop::qquickitem::{qquickitem_to_qobject_ref, AsQQuickItem};
use ffi::{QObject, QQuickItem};
use std::{pin::Pin, ptr::null_mut};

use cxx_qt::CxxQtType;

use crate::cxx_qt_lib_shoop;

type Predicate = dyn Fn(*mut QQuickItem) -> bool;
type BoxedPredicate = Box<Predicate>;

unsafe fn find_parent_item(
    item: *mut QQuickItem,
    predicate: &BoxedPredicate,
    first: bool,
) -> *mut QQuickItem {
    unsafe fn fmt_object(obj: *mut QQuickItem) -> String {
        let obj: *mut QObject = qquickitem::qquickitem_to_qobject_mut(obj);
        let class_name = qobject_class_name(obj.as_ref().unwrap()).unwrap_or("(unknown)");
        let object_name =
            qobject_object_name(obj.as_ref().unwrap()).unwrap_or("(unknown)".to_string());
        let full_object_name = match object_name.as_str() {
            "" => "(no name)".to_string(),
            _ => object_name,
        };
        return format!("{}: {} @ {:p}", full_object_name, class_name, obj);
    }

    if first {
        trace!("find_parent_item:");
    }
    if item.is_null() {
        trace!("  -> null");
        return null_mut();
    }
    if predicate(item) {
        trace!("  -> {} (match)", fmt_object(item));
        return item;
    }
    trace!("  -> {} (no match)", fmt_object(item));
    let parent =
        crate::cxx_qt_lib_shoop::qquickitem::qquickitem_parent_item(item.as_ref().unwrap());
    return find_parent_item(parent, predicate, false);
}

impl FindParentItem {
    fn initialize_impl_with_result(
        mut self: Pin<&mut FindParentItem>,
    ) -> Result<(), anyhow::Error> {
        fn rescan(mut obj: Pin<&mut FindParentItem>) {
            debug!("Parent changed, rescanning.");
            obj.as_mut().rescan();
        }

        self.as_mut()
            .on_parent_changed(|o, _| {
                rescan(o);
            })
            .release();
        self.as_mut()
            .on_item_bool_property_to_check_changed(rescan)
            .release();

        Ok(())
    }

    pub fn initialize_impl(self: Pin<&mut FindParentItem>) {
        match FindParentItem::initialize_impl_with_result(self) {
            Ok(()) => (),
            Err(e) => {
                error!("Unable to initialize: {:?}", e);
            }
        }
    }

    pub unsafe fn rescan_with_result(mut self: Pin<&mut Self>) -> Result<(), anyhow::Error> {
        let quick = self.as_mut().pin_mut_qquickitem_ptr();
        let rust = self.as_mut().rust_mut();
        let maybe_find_predicate = rust.find_predicate.as_ref();
        if matches!(maybe_find_predicate, None) {
            debug!("No predicate set for FindParentItem. Cannot rescan.");
            return Ok(());
        }
        let find_predicate = maybe_find_predicate.unwrap();
        let quick_parent_ptr =
            crate::cxx_qt_lib_shoop::qquickitem::qquickitem_parent_item(quick.as_ref().unwrap());
        let item = find_parent_item(quick_parent_ptr, find_predicate, true);
        self.as_mut().set_found_item(item);
        self.as_mut().update_found_item_bool_property();
        if !item.is_null() && !self.item_bool_property_to_check().is_empty() {
            cxx_qt_lib_shoop::connect::connect(
                item.as_mut().unwrap(),
                String::from(format!("{}Changed()", self.item_bool_property_to_check())),
                self.as_mut().pin_mut_qobject_ptr().as_mut().unwrap(),
                String::from(constants::INVOKABLE_UPDATE_FOUND_ITEM_BOOL_PROPERTY),
                cxx_qt_lib_shoop::connection_types::DIRECT_CONNECTION,
            )?;
        }
        debug!(
            "Rescanning done. Non-null parent item found: {}. Bool property: {}",
            !item.is_null(),
            self.as_mut().found_item_has_true_checked_property()
        );

        Ok(())
    }

    pub fn rescan(self: Pin<&mut Self>) {
        unsafe {
            match self.rescan_with_result() {
                Ok(()) => (),
                Err(e) => {
                    error!("Unable to rescan: {:?}", e);
                }
            }
        }
    }

    pub unsafe fn update_found_item_bool_property(mut self: Pin<&mut Self>) {
        let item = *self.found_item();
        let value: bool = if !item.is_null() && !self.item_bool_property_to_check().is_empty() {
            match qobject_property_bool(
                qquickitem_to_qobject_ref(item.as_ref().unwrap()),
                self.item_bool_property_to_check().to_string(),
            ) {
                Ok(v) => v,
                Err(e) => {
                    error!(
                        "Failed to get bool property {}: {:?}",
                        self.item_bool_property_to_check().to_string(),
                        e
                    );
                    false
                }
            }
        } else {
            false
        };
        self.as_mut()
            .set_found_item_has_true_checked_property(value);
        self.as_mut()
            .set_found_item_with_true_checked_property(if value { item } else { null_mut() });
    }

    pub fn set_find_predicate(mut self: Pin<&mut Self>, predicate: BoxedPredicate) {
        self.as_mut().rust_mut().find_predicate = Some(predicate);
        self.as_mut().rescan();
    }
}

#[cfg(test)]
mod tests {
    use crate::cxx_qt_lib_shoop::qquickitem::IsQQuickItem;
    use crate::cxx_qt_shoop::test::qobj_generic_test_item;
    use cxx_qt_lib::QString;

    use super::*;

    #[test]
    fn test_init() {
        let obj = make_unique();
        assert!(obj.found_item().is_null());
        assert!(obj.found_item_with_true_checked_property().is_null());
        assert_eq!(*obj.found_item_has_true_checked_property(), false);
    }

    #[test]
    fn test_no_parent() {
        let mut obj = make_unique();
        obj.as_mut().unwrap().rescan();
        assert!(obj.found_item().is_null());
        assert!(obj.found_item_with_true_checked_property().is_null());
        assert_eq!(*obj.found_item_has_true_checked_property(), false);
    }

    #[test]
    fn test_parent_no_predicate() {
        let mut obj = make_unique();

        unsafe {
            let mut parent = qobj_generic_test_item::make_unique();
            let parent_ptr = parent.as_mut().unwrap().pin_mut_qquickitem_ptr();
            obj.as_mut().unwrap().set_parent_item(parent_ptr);

            obj.as_mut().unwrap().rescan();

            assert!(obj.found_item().is_null());
            assert!(obj.found_item_with_true_checked_property().is_null());
            assert_eq!(*obj.found_item_has_true_checked_property(), false);
        }
    }

    #[test]
    fn test_parent_no_bool_selected() {
        let mut obj = make_unique();
        obj.as_mut().unwrap().set_find_predicate(Box::new(|_| true));

        unsafe {
            let mut parent = qobj_generic_test_item::make_unique();
            let parent_ptr = parent.as_mut().unwrap().pin_mut_qquickitem_ptr();
            obj.as_mut().unwrap().set_parent_item(parent_ptr);

            obj.as_mut().unwrap().rescan();

            assert_eq!(*obj.found_item(), parent_ptr);
            assert!(obj.found_item_with_true_checked_property().is_null());
            assert_eq!(*obj.found_item_has_true_checked_property(), false);
        }
    }

    #[test]
    fn test_nested_parent_by_ptr() {
        unsafe {
            let mut parent = qobj_generic_test_item::make_unique();
            let parent_ptr = parent.as_mut().unwrap().pin_mut_qquickitem_ptr();
            let mut parent_parent = qobj_generic_test_item::make_unique();
            let parent_parent_ptr = parent_parent.as_mut().unwrap().pin_mut_qquickitem_ptr();
            parent.as_mut().unwrap().set_parent_item(parent_parent_ptr);

            let mut obj = make_unique();
            obj.as_mut()
                .unwrap()
                .set_find_predicate(Box::new(move |p| p == parent_parent_ptr.clone()));
            obj.as_mut().unwrap().set_parent_item(parent_ptr);

            obj.as_mut().unwrap().rescan();

            assert_eq!(*obj.found_item(), parent_parent_ptr);
            assert!(obj.found_item_with_true_checked_property().is_null());
            assert_eq!(*obj.found_item_has_true_checked_property(), false);
        }
    }

    #[test]
    fn test_parent_bool_false() {
        let mut obj = make_unique();
        obj.as_mut().unwrap().set_find_predicate(Box::new(|_| true));

        unsafe {
            let mut parent = qobj_generic_test_item::make_unique();
            let parent_ptr = parent.as_mut().unwrap().pin_mut_qquickitem_ptr();

            obj.as_mut()
                .unwrap()
                .set_item_bool_property_to_check(QString::from("bool_prop"));
            obj.as_mut().unwrap().set_parent_item(parent_ptr);

            obj.as_mut().unwrap().rescan();

            assert_eq!(*obj.found_item(), parent_ptr);
            assert!(obj.found_item_with_true_checked_property().is_null());
            assert_eq!(*obj.found_item_has_true_checked_property(), false);
        }
    }

    #[test]
    fn test_parent_bool_true() {
        let mut obj = make_unique();
        obj.as_mut().unwrap().set_find_predicate(Box::new(|_| true));

        unsafe {
            let mut parent = qobj_generic_test_item::make_unique();
            let parent_ptr = parent.as_mut().unwrap().pin_mut_qquickitem_ptr();
            parent.as_mut().unwrap().set_bool_prop(true);

            obj.as_mut()
                .unwrap()
                .set_item_bool_property_to_check(QString::from("bool_prop"));
            obj.as_mut().unwrap().set_parent_item(parent_ptr);

            obj.as_mut().unwrap().rescan();

            assert_eq!(*obj.found_item(), parent_ptr);
            assert_eq!(*obj.found_item_with_true_checked_property(), parent_ptr);
            assert_eq!(*obj.found_item_has_true_checked_property(), true);
        }
    }

    #[test]
    fn test_parent_bool_change() {
        let mut obj = make_unique();
        obj.as_mut().unwrap().set_find_predicate(Box::new(|_| true));

        unsafe {
            let mut parent = qobj_generic_test_item::make_unique();
            let parent_ptr = parent.as_mut().unwrap().pin_mut_qquickitem_ptr();

            obj.as_mut()
                .unwrap()
                .set_item_bool_property_to_check(QString::from("bool_prop"));
            obj.as_mut().unwrap().set_parent_item(parent_ptr);
            obj.as_mut().unwrap().rescan();

            assert_eq!(*obj.found_item(), parent_ptr);
            assert!(obj.found_item_with_true_checked_property().is_null());
            assert_eq!(*obj.found_item_has_true_checked_property(), false);

            parent.as_mut().unwrap().set_bool_prop(true);

            assert_eq!(*obj.found_item(), parent_ptr);
            assert_eq!(*obj.found_item_with_true_checked_property(), parent_ptr);
            assert_eq!(*obj.found_item_has_true_checked_property(), true);

            parent.as_mut().unwrap().set_bool_prop(false);

            assert_eq!(*obj.found_item(), parent_ptr);
            assert!(obj.found_item_with_true_checked_property().is_null());
            assert_eq!(*obj.found_item_has_true_checked_property(), false);
        }
    }
}
