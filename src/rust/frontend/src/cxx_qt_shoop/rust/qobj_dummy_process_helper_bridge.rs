use cxx_qt_lib;

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qquickitem.h");
        type QQuickItem = crate::cxx_qt_lib_shoop::qquickitem::QQuickItem;
        type QObject = crate::cxx_qt_lib_shoop::qobject::QObject;

        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QQuickItem]
        #[qproperty(f32, wait_start)]
        #[qproperty(f32, wait_interval)]
        #[qproperty(i32, n_iters)]
        #[qproperty(i32, samples_per_iter)]
        #[qproperty(QVariant, backend)]
        #[qproperty(bool, active)]
        type DummyProcessHelper = super::DummyProcessHelperRust;

        #[qinvokable]
        pub fn start(self: Pin<&mut DummyProcessHelper>);

        #[qinvokable]
        pub fn finish(self: Pin<&mut DummyProcessHelper>);
    }

    unsafe extern "C++" {
        #[rust_name="qquickitem_from_ref_dummy_process_helper"]
        unsafe fn qquickitemFromRef(obj : &DummyProcessHelper) -> &QQuickItem;

        #[rust_name="qquickitem_from_ptr_dummy_process_helper"]
        unsafe fn qquickitemFromPtr(obj : *mut DummyProcessHelper) -> *mut QQuickItem;

        include!("cxx-qt-lib-shoop/invoke.h");

        include!("cxx-qt-shoop/make_unique.h");
        #[rust_name = "make_unique_dummy_process_helper"]
        fn make_unique() -> UniquePtr<DummyProcessHelper>;

        include!("cxx-qt-shoop/register_qml_type.h");
        #[rust_name = "register_qml_type_dummy_process_helper"]
        fn register_qml_type(inference_example: &DummyProcessHelper,
                             module_name: &mut String,
                             version_major: i64, version_minor: i64,
                             type_name: &mut String);
    }

    impl cxx_qt::Constructor<(*mut QQuickItem,), NewArguments=(*mut QQuickItem,)> for DummyProcessHelper {}
    impl cxx_qt::Constructor<(), NewArguments=()> for DummyProcessHelper {}
}

use ffi::*;
pub use ffi::DummyProcessHelper;

pub struct DummyProcessHelperRust {
    wait_start: f32,
    wait_interval: f32,
    n_iters: i32,
    samples_per_iter: i32,
    backend: QVariant,
    active: bool,
}

impl Default for DummyProcessHelperRust {
    fn default() -> DummyProcessHelperRust {
        DummyProcessHelperRust {
            wait_start: 0.0,
            wait_interval: 0.0,
            n_iters: 0,
            samples_per_iter: 0,
            backend: QVariant::default(),
            active: false,
        }
    }
}

impl crate::cxx_qt_lib_shoop::qquickitem::AsQQuickItem for DummyProcessHelper {
    unsafe fn mut_qquickitem_ptr(&mut self) -> *mut QQuickItem {
        qquickitem_from_ptr_dummy_process_helper(self as *mut Self)
    }
    unsafe fn ref_qquickitem_ptr(&self) -> *const QQuickItem {
        qquickitem_from_ref_dummy_process_helper(self) as *const QQuickItem
    }
}

impl crate::cxx_qt_lib_shoop::qquickitem::IsQQuickItem for DummyProcessHelper {}

impl cxx_qt::Constructor<(*mut QQuickItem,)> for DummyProcessHelper {
    type BaseArguments = (*mut QQuickItem,);
    type InitializeArguments = ();
    type NewArguments = (*mut QQuickItem,);

    fn route_arguments(args: (*mut QQuickItem,)) -> (
        Self::NewArguments,
        Self::BaseArguments,
        Self::InitializeArguments
    ) {
        (args, args, ())
    }

    fn new(_parent: (*mut QQuickItem,)) -> DummyProcessHelperRust {
        DummyProcessHelperRust::default()
    }
}

impl cxx_qt::Constructor<()> for DummyProcessHelper {
    type BaseArguments = ();
    type InitializeArguments = ();
    type NewArguments = ();

    fn route_arguments(_args: ()) -> (
        Self::NewArguments,
        Self::BaseArguments,
        Self::InitializeArguments
    ) {
        ((), (), ())
    }

    fn new(_args: ()) -> DummyProcessHelperRust {
        DummyProcessHelperRust::default()
    }
}
