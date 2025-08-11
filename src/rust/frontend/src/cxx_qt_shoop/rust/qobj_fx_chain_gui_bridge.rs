use common::logging::macros::*;

shoop_log_unit!("Frontend.FXChain");

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qquickitem.h");
        type QQuickItem = cxx_qt_lib_shoop::qquickitem::QQuickItem;
        type QObject = cxx_qt_lib_shoop::qobject::QObject;

        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QQuickItem]
        // Backend -> Frontend properties
        #[qproperty(bool, initialized, READ, NOTIFY=initialized_changed)]
        #[qproperty(bool, ui_visible, READ, NOTIFY=ui_visible_changed)]
        #[qproperty(bool, ready, READ, NOTIFY=ready_changed)]
        #[qproperty(bool, active, READ, NOTIFY=active_changed)]
        // Frontend -> Backend properties
        #[qproperty(*mut QObject, backend, READ, WRITE=set_backend, NOTIFY=backend_changed)]
        #[qproperty(QString, title, READ, WRITE=set_title, NOTIFY=title_changed)]
        #[qproperty(i32, chain_type, READ, WRITE=set_chain_type, NOTIFY=chain_type_changed)]
        type FXChainGui = super::FXChainGuiRust;

        #[qinvokable]
        pub fn set_backend(self: Pin<&mut FXChainGui>, backend: *mut QObject);

        #[qinvokable]
        pub fn set_title(self: Pin<&mut FXChainGui>, title: QString);

        #[qinvokable]
        pub fn set_chain_type(self: Pin<&mut FXChainGui>, chain_type: i32);

        #[qinvokable]
        pub fn get_state_str(self: Pin<&mut FXChainGui>) -> QString;

        #[qinvokable]
        pub fn restore_state(self: Pin<&mut FXChainGui>, state_str: QString);

        #[qinvokable]
        pub unsafe fn backend_state_changed(
            self: Pin<&mut FXChainGui>,
            initialized: bool,
            ready: bool,
            active: bool,
            visible: bool,
        );

        #[qsignal]
        pub unsafe fn initialized_changed(self: Pin<&mut FXChainGui>, initialized: bool);

        #[qsignal]
        pub unsafe fn title_changed(self: Pin<&mut FXChainGui>, title: QString);

        #[qsignal]
        pub unsafe fn chain_type_changed(self: Pin<&mut FXChainGui>, chain_type: i32);

        #[qsignal]
        pub unsafe fn ui_visible_changed(self: Pin<&mut FXChainGui>, ui_visible: bool);

        #[qsignal]
        pub unsafe fn ready_changed(self: Pin<&mut FXChainGui>, ready: bool);

        #[qsignal]
        pub unsafe fn active_changed(self: Pin<&mut FXChainGui>, active: bool);

        #[qsignal]
        pub unsafe fn backend_changed(self: Pin<&mut FXChainGui>, backend: *mut QObject);

        #[qsignal]
        pub unsafe fn backend_set_backend(self: Pin<&mut FXChainGui>, backend: *mut QObject);

        #[qsignal]
        pub unsafe fn backend_set_title(self: Pin<&mut FXChainGui>, title: QString);

        #[qsignal]
        pub unsafe fn backend_set_chain_type(self: Pin<&mut FXChainGui>, chain_type: i32);

        #[qsignal]
        pub unsafe fn backend_push_active(self: Pin<&mut FXChainGui>, active: bool);

        #[qsignal]
        pub unsafe fn backend_push_ui_visible(self: Pin<&mut FXChainGui>, ui_visible: bool);

        #[qsignal]
        pub unsafe fn backend_restore_state(self: Pin<&mut FXChainGui>, state_str: QString);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qquickitem.h");

        #[rust_name = "qquickitem_from_ref_fx_chain_gui"]
        unsafe fn qquickitemFromRef(obj: &FXChainGui) -> &QQuickItem;

        #[rust_name = "qquickitem_from_ptr_fx_chain_gui"]
        unsafe fn qquickitemFromPtr(obj: *mut FXChainGui) -> *mut QQuickItem;

        include!("cxx-qt-lib-shoop/qobject.h");

        #[rust_name = "from_qobject_ref_fx_chain_gui"]
        unsafe fn fromQObjectRef(obj: &QObject, output: *mut *const FXChainGui);

        #[rust_name = "from_qobject_mut_fx_chain_gui"]
        unsafe fn fromQObjectMut(obj: Pin<&mut QObject>, output: *mut *mut FXChainGui);

        include!("cxx-qt-lib-shoop/register_qml_type.h");
        #[rust_name = "register_qml_type_fx_chain_gui"]
        unsafe fn register_qml_type(
            inference_example: *mut FXChainGui,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        );
    }

    impl cxx_qt::Constructor<(*mut QQuickItem,), NewArguments = (*mut QQuickItem,)> for FXChainGui {}
}

use cxx_qt_lib_shoop::{qquickitem::IsQQuickItem, qsharedpointer_qobject::QSharedPointer_QObject};
pub use ffi::FXChainGui;
use ffi::*;

pub struct FXChainGuiRust {
    pub initialized: bool,
    pub ui_visible: bool,
    pub ready: bool,
    pub active: bool,
    pub backend: *mut QObject,
    pub title: QString,
    pub chain_type: i32,
    pub backend_chain_wrapper: cxx::UniquePtr<QSharedPointer_QObject>,
}

impl Default for FXChainGuiRust {
    fn default() -> FXChainGuiRust {
        FXChainGuiRust {
            initialized: false,
            ui_visible: false,
            ready: false,
            active: false,
            backend: std::ptr::null_mut(),
            title: QString::from(""),
            chain_type: 0,
            backend_chain_wrapper: cxx::UniquePtr::null(),
        }
    }
}

impl cxx_qt_lib_shoop::qquickitem::AsQQuickItem for FXChainGui {
    unsafe fn mut_qquickitem_ptr(&mut self) -> *mut QQuickItem {
        qquickitem_from_ptr_fx_chain_gui(self as *mut Self)
    }
    unsafe fn ref_qquickitem_ptr(&self) -> *const QQuickItem {
        qquickitem_from_ref_fx_chain_gui(self) as *const QQuickItem
    }
}

impl cxx_qt_lib_shoop::qobject::FromQObject for FXChainGui {
    unsafe fn ptr_from_qobject_ref(obj: &cxx_qt_lib_shoop::qobject::QObject) -> *const Self {
        let mut output: *const Self = std::ptr::null();
        from_qobject_ref_fx_chain_gui(obj, &mut output as *mut *const Self);
        output
    }

    unsafe fn ptr_from_qobject_mut(
        obj: std::pin::Pin<&mut cxx_qt_lib_shoop::qobject::QObject>,
    ) -> *mut Self {
        let mut output: *mut Self = std::ptr::null_mut();
        from_qobject_mut_fx_chain_gui(obj, &mut output as *mut *mut Self);
        output
    }
}

impl IsQQuickItem for FXChainGui {}

impl cxx_qt::Constructor<(*mut QQuickItem,)> for FXChainGui {
    type BaseArguments = (*mut QQuickItem,); // Will be passed to the base class constructor
    type InitializeArguments = (); // Will be passed to the "initialize" function
    type NewArguments = (*mut QQuickItem,); // Will be passed to the "new" function

    fn route_arguments(
        args: (*mut QQuickItem,),
    ) -> (
        Self::NewArguments,
        Self::BaseArguments,
        Self::InitializeArguments,
    ) {
        (args, args, ())
    }

    fn new(_parent: (*mut QQuickItem,)) -> FXChainGuiRust {
        FXChainGuiRust::default()
    }

    fn initialize(self: std::pin::Pin<&mut Self>, _: Self::InitializeArguments) {
        FXChainGui::initialize_impl(self);
    }
}
