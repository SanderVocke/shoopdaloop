use common::logging::macros::*;
shoop_log_unit!("Frontend.GenericTestItem");

pub mod constants {
    pub const PROP_BOOL_PROP : &str = "boolProp";

    pub const SIGNAL_BOOL_PROP_CHANGED: &str = "boolPropChanged()";
}

#[cxx_qt::bridge(cxx_file_stem="qobj_generic_test_item")]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qquickitem.h");
        type QQuickItem = crate::cxx_qt_lib_shoop::qquickitem::QQuickItem;
        type QObject = crate::cxx_qt_lib_shoop::qobject::QObject;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[base = "QQuickItem"]
        #[qproperty(bool, bool_prop)]
        type GenericTestItem = super::GenericTestItemRust;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qquickitem.h");

        #[rust_name="qquickitem_from_ref_generic_test_item"]
        unsafe fn qquickitemFromRef(obj : &GenericTestItem) -> &QQuickItem;
        #[rust_name="qquickitem_from_ptr_generic_test_item"]
        unsafe fn qquickitemFromPtr(obj : *mut GenericTestItem) -> *mut QQuickItem;

        include!("cxx-qt-shoop/make_unique.h");
        #[rust_name = "make_unique_generic_test_item"]
        fn make_unique() -> UniquePtr<GenericTestItem>;
    }

    impl cxx_qt::Constructor<(*mut QQuickItem,), NewArguments=()> for GenericTestItem {}
    impl cxx_qt::Constructor<(), NewArguments=()> for GenericTestItem {}
}

pub use ffi::GenericTestItem;
pub use ffi::make_unique_generic_test_item as make_unique;
use crate::cxx_qt_lib_shoop::qquickitem::{AsQQuickItem, IsQQuickItem};
use ffi::QQuickItem;

#[derive(Default)]
pub struct GenericTestItemRust {
    bool_prop : bool,
}

impl cxx_qt::Constructor<(*mut QQuickItem,)> for GenericTestItem {
    type BaseArguments = (*mut QQuickItem,); // Will be passed to the base class constructor
    type InitializeArguments = (); // Will be passed to the "initialize" function
    type NewArguments = (); // Will be passed to the "new" function

    fn route_arguments(args: (*mut QQuickItem,)) -> (
        Self::NewArguments,
        Self::BaseArguments,
        Self::InitializeArguments
    ) { ((), args, ()) }

    fn new(_args : ()) -> GenericTestItemRust { GenericTestItemRust::default() }
}

impl cxx_qt::Constructor<()> for GenericTestItem {
    type BaseArguments = (); // Will be passed to the base class constructor
    type InitializeArguments = (); // Will be passed to the "initialize" function
    type NewArguments = (); // Will be passed to the "new" function

    fn route_arguments(_args: ()) -> (
        Self::NewArguments,
        Self::BaseArguments,
        Self::InitializeArguments
    ) { ((), (), ()) }

    fn new(_args : ()) -> GenericTestItemRust { GenericTestItemRust::default() }
}

impl AsQQuickItem for GenericTestItem {
    unsafe fn mut_qquickitem_ptr (&mut self) -> *mut QQuickItem { ffi::qquickitem_from_ptr_generic_test_item(self as *mut Self) }
    unsafe fn ref_qquickitem_ptr (& self) -> *const QQuickItem { ffi::qquickitem_from_ref_generic_test_item(self) as *const QQuickItem }
}

impl IsQQuickItem for GenericTestItem {}