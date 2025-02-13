use cxx_qt::Constructor;
use cxx_qt::QObject;
use cxx_qt::QVariant;
use cxx_qt::QString;
use cxx_qt::QQuickItem;

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qquickitem.h");
        type QQuickItem = crate::cxx_qt_lib_shoop::qquickitem::QQuickItem;

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

        #[qsignal]
        pub fn wait_start_changed(self: Pin<&mut DummyProcessHelper>);

        #[qsignal]
        pub fn wait_interval_changed(self: Pin<&mut DummyProcessHelper>);

        #[qsignal]
        pub fn n_iters_changed(self: Pin<&mut DummyProcessHelper>);

        #[qsignal]
        pub fn samples_per_iter_changed(self: Pin<&mut DummyProcessHelper>);

        #[qsignal]
        pub fn backend_changed(self: Pin<&mut DummyProcessHelper>);

        #[qsignal]
        pub fn active_changed(self: Pin<&mut DummyProcessHelper>);

        #[qinvokable]
        pub fn start(self: Pin<&mut DummyProcessHelper>);
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
