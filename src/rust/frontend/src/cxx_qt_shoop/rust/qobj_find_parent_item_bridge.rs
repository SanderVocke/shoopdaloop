use common::logging::macros::*;
shoop_log_unit!("Frontend.FindParentItem");

pub mod constants {
    pub const PROP_FOUND_ITEM : &str = "foundItem";
    pub const PROP_ITEM_BOOL_PROPERTY_TO_CHECK : &str = "itemBoolPropertyToCheck";
    pub const PROP_FOUND_ITEM_WITH_TRUE_CHECKED_PROPERTY : &str = "foundItemWithTrueCheckedProperty";
    pub const PROP_FOUND_ITEM_HAS_TRUE_CHECKED_PROPERTY : &str = "foundItemHasTrueCheckedProperty";

    pub const SIGNAL_PARENT_CHANGED : &str = "onParentChanged()";
    pub const SIGNAL_FOUND_ITEM_CHANGED : &str = "foundItemChanged()";
    pub const SIGNAL_ITEM_BOOL_PROPERTY_TO_CHECK_CHANGED : &str = "itemBoolPropertyToCheckChanged()";
    pub const SIGNAL_FOUND_ITEM_WITH_TRUE_CHECKED_PROPERTY_CHANGED : &str = "foundItemWithTrueCheckedPropertyChanged()";
    pub const SIGNAL_FOUND_ITEM_HAS_TRUE_CHECKED_PROPERTY_CHANGED : &str = "foundItemHasTrueCheckedPropertyChanged()";

    pub const INVOKABLE_RESCAN : &str = "rescan()";
    pub const INVOKABLE_UPDATE_FOUND_ITEM_BOOL_PROPERTY : &str = "updateFoundItemBoolProperty()";
}

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qquickitem.h");
        type QQuickItem = crate::cxx_qt_lib_shoop::qquickitem::QQuickItem;
        type QObject = crate::cxx_qt_lib_shoop::qobject::QObject;

        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[base = QQuickItem]
        #[qproperty(*mut QQuickItem, found_item)]
        #[qproperty(QString, item_bool_property_to_check)]
        #[qproperty(*mut QQuickItem, found_item_with_true_checked_property)]
        #[qproperty(bool, found_item_has_true_checked_property)]
        type FindParentItem = super::FindParentItemRust;

        pub fn initialize_impl(self : Pin<&mut FindParentItem>);

        #[inherit]
        #[qsignal]
        unsafe fn parent_changed(self : Pin<&mut FindParentItem>, parent : *mut QQuickItem);

        #[qinvokable]
        unsafe fn rescan(self : Pin<&mut FindParentItem>);

        #[qinvokable]
        unsafe fn update_found_item_bool_property(self : Pin<&mut FindParentItem>);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qquickitem.h");

        #[rust_name="qquickitem_from_ref_find_parent_item"]
        unsafe fn qquickitemFromRef(obj : &FindParentItem) -> &QQuickItem;
        #[rust_name="qquickitem_from_ptr_find_parent_item"]
        unsafe fn qquickitemFromPtr(obj : *mut FindParentItem) -> *mut QQuickItem;

        include!("cxx-qt-shoop/make_unique.h");
        #[rust_name = "make_unique_find_parent_item"]
        fn make_unique() -> UniquePtr<FindParentItem>;

        include!("cxx-qt-lib-shoop/connect.h");
        #[rust_name = "connect_to_find_parent_item"]
        unsafe fn connect(sender : *mut QQuickItem,
                   signal : String,
                   receiver : *mut FindParentItem,
                   slot : String) -> Result<()>;
    }

    impl cxx_qt::Constructor<(*mut QQuickItem,), NewArguments=()> for FindParentItem {}
    impl cxx_qt::Constructor<(), NewArguments=()> for FindParentItem {}
}

use cxx_qt_lib::QString;
pub use ffi::FindParentItem;
pub use ffi::make_unique_find_parent_item as make_unique;
use crate::cxx_qt_lib_shoop::qquickitem::IsQQuickItem;
use ffi::QQuickItem;
use std::{pin::Pin, ptr::null_mut};

type Predicate = dyn Fn(*mut QQuickItem) -> bool;
type BoxedPredicate = Box<Predicate>;

pub struct FindParentItemRust {
    pub found_item : *mut QQuickItem,
    pub item_bool_property_to_check : QString,
    pub found_item_with_true_checked_property : *mut QQuickItem,
    pub found_item_has_true_checked_property : bool,
    pub find_predicate : Option<BoxedPredicate>,
}

impl Default for FindParentItemRust {
    fn default() -> FindParentItemRust {
        FindParentItemRust {
            found_item : null_mut(),
            item_bool_property_to_check : QString::from(""),
            found_item_with_true_checked_property : null_mut(),
            found_item_has_true_checked_property : false,
            find_predicate : None,
        }
    }
}

impl cxx_qt::Constructor<(*mut QQuickItem,)> for FindParentItem {
    type BaseArguments = (*mut QQuickItem,); // Will be passed to the base class constructor
    type InitializeArguments = (); // Will be passed to the "initialize" function
    type NewArguments = (); // Will be passed to the "new" function

    fn route_arguments(args: (*mut QQuickItem,)) -> (
        Self::NewArguments,
        Self::BaseArguments,
        Self::InitializeArguments
    ) { ((), args, ()) }

    fn new(_args : ()) -> FindParentItemRust { FindParentItemRust::default() }
    fn initialize(self : Pin<&mut FindParentItem>, _args : ()) { FindParentItem::initialize_impl(self); }
}

impl cxx_qt::Constructor<()> for FindParentItem {
    type BaseArguments = (); // Will be passed to the base class constructor
    type InitializeArguments = (); // Will be passed to the "initialize" function
    type NewArguments = (); // Will be passed to the "new" function

    fn route_arguments(_args: ()) -> (
        Self::NewArguments,
        Self::BaseArguments,
        Self::InitializeArguments
    ) { ((), (), ()) }

    fn new(_args : ()) -> FindParentItemRust { FindParentItemRust::default() }
    fn initialize(self : Pin<&mut FindParentItem>, _args : ()) { FindParentItem::initialize_impl(self); }
}

impl crate::cxx_qt_lib_shoop::qquickitem::AsQQuickItem for FindParentItem {
    unsafe fn mut_qquickitem_ptr (&mut self) -> *mut QQuickItem { ffi::qquickitem_from_ptr_find_parent_item(self as *mut Self) }
    unsafe fn ref_qquickitem_ptr (& self) -> *const QQuickItem { ffi::qquickitem_from_ref_find_parent_item(self) as *const QQuickItem }
}

impl IsQQuickItem for FindParentItem {}
