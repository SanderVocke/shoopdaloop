use common::logging::macros::*;
shoop_log_unit!("Frontend.CompositeLoop");

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

        include!("cxx-qt-lib/qlist.h");
        type QList_QVariant = cxx_qt_lib::QList<cxx_qt_lib::QVariant>;

        include!("cxx-qt-lib-shoop/invoke.h");
        #[rust_name = "invoke_with_return_variantmap"]
        unsafe fn invoke_with_return(obj : *mut QObject, method : String) -> Result<QMap_QString_QVariant>;

        include!("cxx-qt-lib-shoop/metatype.h");
        #[rust_name = "compositeloop_metatype_name"]
        unsafe fn meta_type_name(obj: &CompositeLoop) -> Result<&str>;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[base = QQuickItem]
        #[qproperty(*mut QObject, sync_loop)]
        #[qproperty(QVariant, schedule)]
        #[qproperty(QVariant, running_loops)]
        #[qproperty(i32, iteration)]
        #[qproperty(bool, play_after_record)]
        #[qproperty(bool, sync_mode_active)]
        #[qproperty(i32, mode)]
        #[qproperty(i32, next_mode)]
        #[qproperty(i32, next_transition_delay)]
        #[qproperty(i32, n_cycles)]
        #[qproperty(i32, length)]
        #[qproperty(QString, kind)]
        #[qproperty(i32, sync_position)]
        #[qproperty(i32, sync_length)]
        #[qproperty(i32, position)]
        type CompositeLoop = super::CompositeLoopRust;

        pub fn initialize_impl(self : Pin<&mut CompositeLoop>);

        #[qinvokable]
        pub fn update(self : Pin<&mut CompositeLoop>);

        #[inherit]
        #[qsignal]
        unsafe fn parent_changed(self : Pin<&mut CompositeLoop>, parent : *mut QQuickItem);

        #[qsignal]
        fn cycled(self : Pin<&mut CompositeLoop>, cycle_nr: i32);

    }

    impl cxx_qt::Constructor<(*mut QQuickItem,), NewArguments=(*mut QQuickItem,)> for CompositeLoop {}
    impl cxx_qt::Constructor<(), NewArguments=()> for CompositeLoop {}
}

pub use ffi::CompositeLoop;
use ffi::*;
use cxx::UniquePtr;
use crate::cxx_qt_shoop::qobj_find_parent_item::FindParentItem;
use crate::cxx_qt_shoop::fn_find_backend_wrapper;
use crate::cxx_qt_lib_shoop::qquickitem::IsQQuickItem;

pub struct CompositeLoopRust {
    pub sync_loop: *mut QObject,
    pub schedule: QVariant,
    pub running_loops: QVariant,
    pub iteration: i32,
    pub play_after_record: bool,
    pub sync_mode_active: bool,
    pub mode: i32,
    pub next_mode: i32,
    pub next_transition_delay: i32,
    pub n_cycles: i32,
    pub length: i32,
    pub kind: String,
    pub sync_position: i32,
    pub sync_length: i32,
    pub position: i32,
}

impl Default for CompositeLoopRust {
    fn default() -> CompositeLoopRust {
        CompositeLoopRust {
            sync_loop: std::ptr::null_mut(),
            schedule: QVariant::default(),
            running_loops: QVariant::default(),
            iteration: 0,
            play_after_record: false,
            sync_mode_active: false,
            mode: 0,
            next_mode: 0,
            next_transition_delay: -1,
            n_cycles: 0,
            length: 0,
            kind: String::from("regular"),
            sync_position: 0,
            sync_length: 0,
            position: 0,
        }
    }
}

impl crate::cxx_qt_lib_shoop::qquickitem::AsQQuickItem for CompositeLoop {
    unsafe fn mut_qquickitem_ptr(&mut self) -> *mut QQuickItem {
        qquickitem_from_ptr_autoconnect(self as *mut Self)
    }
    unsafe fn ref_qquickitem_ptr(&self) -> *const QQuickItem {
        qquickitem_from_ref_autoconnect(self) as *const QQuickItem
    }
}

impl IsQQuickItem for CompositeLoop {}

impl cxx_qt::Constructor<(*mut QQuickItem,)> for CompositeLoop {
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

    fn new(_parent: (*mut QQuickItem,)) -> CompositeLoopRust {
        CompositeLoopRust::default()
    }

    fn initialize(self: core::pin::Pin<&mut Self>, _: Self::InitializeArguments) {
        CompositeLoop::initialize_impl(self);
    }
}

impl cxx_qt::Constructor<()> for CompositeLoop {
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

    fn new(_args: ()) -> CompositeLoopRust {
        CompositeLoopRust::default()
    }

    fn initialize(self: core::pin::Pin<&mut Self>, _: Self::InitializeArguments) {
        CompositeLoop::initialize_impl(self);
    }
}
