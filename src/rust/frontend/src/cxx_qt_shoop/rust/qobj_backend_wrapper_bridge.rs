use common::logging::macros::*;
shoop_log_unit!("Frontend.BackendWrapper");

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qquickitem.h");
        type QQuickItem = crate::cxx_qt_lib_shoop::qquickitem::QQuickItem;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[base = QQuickItem]
        #[qproperty(bool, ready)]
        type BackendWrapper = super::BackendWrapperRust;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qquickitem.h");

        #[rust_name="qquickitem_from_ref_backend_wrapper"]
        unsafe fn qquickitemFromRef(obj : &BackendWrapper) -> &QQuickItem;

        #[rust_name="qquickitem_from_ptr_backend_wrapper"]
        unsafe fn qquickitemFromPtr(obj : *mut BackendWrapper) -> *mut QQuickItem;

        include!("cxx-qt-shoop/make_unique.h");

        #[rust_name = "make_unique_backend_wrapper"]
        fn make_unique() -> UniquePtr<BackendWrapper>;

        include!("cxx-qt-shoop/qobject_classname.h");
        #[rust_name = "qobject_class_name_backend_wrapper"]
        fn qobject_class_name(obj : &BackendWrapper) -> Result<&str>;
    }

    impl cxx_qt::Constructor<(*mut QQuickItem,), NewArguments=(*mut QQuickItem,)> for BackendWrapper {}
    impl cxx_qt::Constructor<(), NewArguments=()> for BackendWrapper {}
}

use ffi::*;
pub use ffi::BackendWrapper;
pub struct BackendWrapperRust {
    ready : bool,
}

impl Default for BackendWrapperRust {
    fn default() -> BackendWrapperRust {
        BackendWrapperRust {
            ready : false
        }
    }
}

impl crate::cxx_qt_lib_shoop::qquickitem::AsQQuickItem for BackendWrapper {
    unsafe fn mut_qquickitem_ptr (&mut self) -> *mut QQuickItem { qquickitem_from_ptr_backend_wrapper(self as *mut Self) }
    unsafe fn ref_qquickitem_ptr (& self) -> *const QQuickItem { qquickitem_from_ref_backend_wrapper(self) as *const QQuickItem }
}

impl crate::cxx_qt_lib_shoop::qquickitem::IsQQuickItem for BackendWrapper {}

impl cxx_qt::Constructor<(*mut QQuickItem,)> for BackendWrapper {
    type BaseArguments = (*mut QQuickItem,); // Will be passed to the base class constructor
    type InitializeArguments = (); // Will be passed to the "initialize" function
    type NewArguments = (*mut QQuickItem,); // Will be passed to the "new" function

    fn route_arguments(args: (*mut QQuickItem,)) -> (
        Self::NewArguments,
        Self::BaseArguments,
        Self::InitializeArguments
    ) {
        (args, args, ())
    }

    fn new(_parent : (*mut QQuickItem,)) -> BackendWrapperRust {
        BackendWrapperRust::default()
    }
}


impl cxx_qt::Constructor<()> for BackendWrapper {
    type BaseArguments = (); // Will be passed to the base class constructor
    type InitializeArguments = (); // Will be passed to the "initialize" function
    type NewArguments = (); // Will be passed to the "new" function

    fn route_arguments(_args: ()) -> (
        Self::NewArguments,
        Self::BaseArguments,
        Self::InitializeArguments
    ) {
        ((), (), ())
    }

    fn new(_args: ()) -> BackendWrapperRust {
        BackendWrapperRust::default()
    }
}