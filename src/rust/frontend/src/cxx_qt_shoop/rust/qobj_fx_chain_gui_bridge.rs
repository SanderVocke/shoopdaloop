use common::logging::macros::*;

shoop_log_unit!("Frontend.FXChain");

#[cxx_qt::bridge]
pub mod ffi {
    extern "C++" {
        #[doc(hidden)]
        #[namespace = ""]
        type QObject = cxx_qt::QObject;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qquickitem.h");
        type QQuickItem = cxx_qt_lib_shoop::qquickitem::QQuickItem;
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QQuickItem]
        // Engine -> frontend properties
        #[qproperty(bool, initialized, READ, WRITE=set_initialized, NOTIFY=initialized_changed)]
        #[qproperty(bool, ui_visible, READ=get_ui_visible, NOTIFY=ui_visible_changed)]
        #[qproperty(bool, ready, READ=get_ready, NOTIFY=ready_changed)]
        #[qproperty(bool, active, READ=get_active, NOTIFY=active_changed)]
        // Frontend -> Backend properties
        #[qproperty(*mut QObject, backend, READ, WRITE=set_backend, NOTIFY=backend_changed)]
        #[qproperty(QString, title, READ=get_title, WRITE=set_title, NOTIFY=title_changed)]
        #[qproperty(i32, chain_type, READ=get_chain_type, WRITE=set_chain_type, NOTIFY=chain_type_changed)]
        type FXChainGui = super::FXChainGuiRust;

        #[qinvokable]
        pub fn set_backend(self: Pin<&mut FXChainGui>, backend: *mut QObject);

        #[qinvokable]
        pub fn set_title(self: Pin<&mut FXChainGui>, title: QString);

        #[qinvokable]
        pub fn set_chain_type(self: Pin<&mut FXChainGui>, chain_type: i32);

        #[qinvokable]
        pub fn push_ui_visible(self: Pin<&mut FXChainGui>, ui_visible: bool);

        #[qinvokable]
        pub fn push_active(self: Pin<&mut FXChainGui>, active: bool);

        #[qinvokable]
        pub fn get_state_str(self: Pin<&mut FXChainGui>) -> QString;

        #[qinvokable]
        pub fn restore_state(self: Pin<&mut FXChainGui>, state_str: QString);

        #[qinvokable]
        pub fn get_ui_visible(self: Pin<&mut FXChainGui>) -> bool;

        #[qinvokable]
        pub fn get_ready(self: Pin<&mut FXChainGui>) -> bool;

        #[qinvokable]
        pub fn get_active(self: Pin<&mut FXChainGui>) -> bool;

        #[qinvokable]
        pub fn get_chain_type(self: Pin<&mut FXChainGui>) -> i32;

        #[qinvokable]
        pub fn get_title(self: Pin<&mut FXChainGui>) -> QString;

        #[qinvokable]
        pub fn set_initialized(self: Pin<&mut FXChainGui>, initialized: bool);

        #[qinvokable]
        pub fn update(self: Pin<&mut FXChainGui>);

        #[qinvokable]
        pub fn maybe_initialize_backend(self: Pin<&mut FXChainGui>) -> bool;

        #[qinvokable]
        pub fn deinit(self: Pin<&mut FXChainGui>);

        #[qsignal]
        pub unsafe fn state_changed(
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
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qobject.h");
        include!("cxx-qt-lib-shoop/qquickitem.h");

        #[rust_name = "qquickitem_from_ref_fx_chain_gui"]
        unsafe fn qquickitemFromRef(obj: &FXChainGui) -> &QQuickItem;

        #[rust_name = "qquickitem_from_ptr_fx_chain_gui"]
        unsafe fn qquickitemFromPtr(obj: *mut FXChainGui) -> *mut QQuickItem;

        #[rust_name = "from_qobject_ref_fx_chain_gui"]
        unsafe fn fromQObjectRef(obj: &QObject, output: *mut *const FXChainGui);

        #[rust_name = "from_qobject_mut_fx_chain_gui"]
        unsafe fn fromQObjectMut(obj: Pin<&mut QObject>, output: *mut *mut FXChainGui);

        #[rust_name = "fx_chain_gui_qobject_from_ptr"]
        unsafe fn qobjectFromPtr(obj: *mut FXChainGui) -> *mut QObject;

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
    impl cxx_qt::Constructor<()> for FXChainGui {}
}

pub use ffi::FXChainGui;
use ffi::*;
use shoop_engine::app_backend::FXChain as BackendFXChain;
use shoop_engine::{FXChainState, FXChainType};

pub struct FXChainGuiRust {
    // Properties
    pub initialized: bool,
    pub backend: *mut QObject,

    // Other
    pub backend_chain_wrapper: Option<BackendFXChain>,
    pub prev_state: FXChainState,
    pub chain_type: Option<FXChainType>,
    pub title: Option<String>,
}

impl Default for FXChainGuiRust {
    fn default() -> FXChainGuiRust {
        FXChainGuiRust {
            initialized: false,
            backend: std::ptr::null_mut(),
            title: None,
            chain_type: None,
            backend_chain_wrapper: None,
            prev_state: FXChainState {
                ready: 0,
                active: 1,
                visible: 0,
            },
        }
    }
}

impl cxx_qt_lib_shoop::qobject::FromQObject for FXChainGui {
    unsafe fn ptr_from_qobject_ref(obj: &cxx_qt::QObject) -> *const Self {
        let mut output: *const Self = std::ptr::null();
        from_qobject_ref_fx_chain_gui(obj, &mut output as *mut *const Self);
        output
    }

    unsafe fn ptr_from_qobject_mut(obj: std::pin::Pin<&mut cxx_qt::QObject>) -> *mut Self {
        let mut output: *mut Self = std::ptr::null_mut();
        from_qobject_mut_fx_chain_gui(obj, &mut output as *mut *mut Self);
        output
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

impl cxx_qt_lib_shoop::qquickitem::IsQQuickItem for FXChainGui {}

impl cxx_qt::Constructor<(*mut QQuickItem,)> for FXChainGui {
    type BaseArguments = (*mut QQuickItem,);
    type InitializeArguments = ();
    type NewArguments = (*mut QQuickItem,);

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

impl cxx_qt::Constructor<()> for FXChainGui {
    type BaseArguments = ();
    type InitializeArguments = ();
    type NewArguments = ();

    fn route_arguments(
        args: (),
    ) -> (
        Self::NewArguments,
        Self::BaseArguments,
        Self::InitializeArguments,
    ) {
        (args, args, ())
    }

    fn new(_parent: ()) -> FXChainGuiRust {
        FXChainGuiRust::default()
    }

    fn initialize(self: std::pin::Pin<&mut Self>, _: Self::InitializeArguments) {
        FXChainGui::initialize_impl(self);
    }
}
