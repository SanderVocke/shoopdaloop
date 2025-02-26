use common::logging::macros::*;
shoop_log_unit!("Frontend.TestBackendWrapper");

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qquickitem.h");
        type QQuickItem = crate::cxx_qt_lib_shoop::qquickitem::QQuickItem;
        type QObject = crate::cxx_qt_lib_shoop::qobject::QObject;

        include!("cxx-qt-lib/qlist.h");
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
        type QList_QVariant = cxx_qt_lib::QList<QVariant>;

        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[base = QQuickItem]
        #[qproperty(bool, ready)]
        type TestBackendWrapper = super::TestBackendWrapperRust;

        pub fn initialize_impl(self : Pin<&mut TestBackendWrapper>);

        #[qinvokable]
        fn find_external_ports(self : Pin<&mut TestBackendWrapper>,
                               maybe_regex : QString,
                               port_direction : i32,
                               dataType : i32) -> QList_QVariant;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qquickitem.h");

        #[rust_name="qquickitem_from_ref_test_backend_wrapper"]
        unsafe fn qquickitemFromRef(obj : &TestBackendWrapper) -> &QQuickItem;
        #[rust_name="qquickitem_from_ptr_test_backend_wrapper"]
        unsafe fn qquickitemFromPtr(obj : *mut TestBackendWrapper) -> *mut QQuickItem;

        include!("cxx-qt-shoop/make_unique.h");
        #[rust_name = "make_unique_test_backend_wrapper"]
        fn make_unique() -> UniquePtr<TestBackendWrapper>;
    }

    impl cxx_qt::Constructor<(*mut QQuickItem,), NewArguments=()> for TestBackendWrapper {}
    impl cxx_qt::Constructor<(), NewArguments=()> for TestBackendWrapper {}
}

pub use ffi::TestBackendWrapper;
pub use ffi::make_unique_test_backend_wrapper as make_unique;
use crate::cxx_qt_lib_shoop::qquickitem::{AsQQuickItem, IsQQuickItem};
use ffi::*;
use crate::cxx_qt_shoop::type_external_port_descriptor::ExternalPortDescriptor;
use crate::cxx_qt_shoop::qobj_signature_backend_wrapper;

#[derive(Default)]
pub struct TestBackendWrapperRust {
    pub ready: bool,
    pub mock_external_ports: Vec<ExternalPortDescriptor>,
}

impl cxx_qt::Constructor<(*mut QQuickItem,)> for TestBackendWrapper {
    type BaseArguments = (*mut QQuickItem,); // Will be passed to the base class constructor
    type InitializeArguments = (); // Will be passed to the "initialize" function
    type NewArguments = (); // Will be passed to the "new" function

    fn route_arguments(args: (*mut QQuickItem,)) -> (
        Self::NewArguments,
        Self::BaseArguments,
        Self::InitializeArguments
    ) { ((), args, ()) }

    fn new(_args : ()) -> TestBackendWrapperRust { TestBackendWrapperRust::default() }
    fn initialize(self: core::pin::Pin<&mut Self>, _: Self::InitializeArguments) { TestBackendWrapper::initialize_impl(self); }
}

impl cxx_qt::Constructor<()> for TestBackendWrapper {
    type BaseArguments = (); // Will be passed to the base class constructor
    type InitializeArguments = (); // Will be passed to the "initialize" function
    type NewArguments = (); // Will be passed to the "new" function

    fn route_arguments(_args: ()) -> (
        Self::NewArguments,
        Self::BaseArguments,
        Self::InitializeArguments
    ) { ((), (), ()) }

    fn new(_args : ()) -> TestBackendWrapperRust { TestBackendWrapperRust::default() }
    fn initialize(self: core::pin::Pin<&mut Self>, _: Self::InitializeArguments) { TestBackendWrapper::initialize_impl(self); }
}

impl AsQQuickItem for TestBackendWrapper {
    unsafe fn mut_qquickitem_ptr (&mut self) -> *mut QQuickItem { ffi::qquickitem_from_ptr_test_backend_wrapper(self as *mut Self) }
    unsafe fn ref_qquickitem_ptr (& self) -> *const QQuickItem { ffi::qquickitem_from_ref_test_backend_wrapper(self) as *const QQuickItem }
}

impl IsQQuickItem for TestBackendWrapper {}