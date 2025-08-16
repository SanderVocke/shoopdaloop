use common::logging::macros::*;

shoop_log_unit!("Frontend.FXChain");

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qobject.h");
        type QObject = cxx_qt_lib_shoop::qobject::QObject;

        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    unsafe extern "RustQt" {
        #[qobject]
        // Backend -> Frontend properties
        #[qproperty(bool, initialized, READ, WRITE=set_initialized, NOTIFY=initialized_changed)]
        #[qproperty(bool, ui_visible, READ=get_ui_visible, NOTIFY=ui_visible_changed)]
        #[qproperty(bool, ready, READ=get_ready, NOTIFY=ready_changed)]
        #[qproperty(bool, active, READ=get_active, NOTIFY=active_changed)]
        // Frontend -> Backend properties
        #[qproperty(*mut QObject, backend, READ, WRITE=set_backend, NOTIFY=backend_changed)]
        #[qproperty(QString, title, READ=get_title, WRITE=set_title, NOTIFY=title_changed)]
        #[qproperty(i32, chain_type, READ=get_chain_type, WRITE=set_chain_type, NOTIFY=chain_type_changed)]
        type FXChainBackend = super::FXChainBackendRust;

        #[qinvokable]
        pub fn set_backend(self: Pin<&mut FXChainBackend>, backend: *mut QObject);

        #[qinvokable]
        pub fn set_title(self: Pin<&mut FXChainBackend>, title: QString);

        #[qinvokable]
        pub fn set_chain_type(self: Pin<&mut FXChainBackend>, chain_type: i32);

        #[qinvokable]
        pub fn push_ui_visible(self: Pin<&mut FXChainBackend>, ui_visible: bool);

        #[qinvokable]
        pub fn push_active(self: Pin<&mut FXChainBackend>, active: bool);

        #[qinvokable]
        pub fn get_state_str(self: Pin<&mut FXChainBackend>) -> QString;

        #[qinvokable]
        pub fn restore_state(self: Pin<&mut FXChainBackend>, state_str: QString);

        #[qinvokable]
        pub fn get_ui_visible(self: Pin<&mut FXChainBackend>) -> bool;

        #[qinvokable]
        pub fn get_ready(self: Pin<&mut FXChainBackend>) -> bool;

        #[qinvokable]
        pub fn get_active(self: Pin<&mut FXChainBackend>) -> bool;

        #[qinvokable]
        pub fn get_chain_type(self: Pin<&mut FXChainBackend>) -> i32;

        #[qinvokable]
        pub fn get_title(self: Pin<&mut FXChainBackend>) -> QString;

        #[qinvokable]
        pub fn set_initialized(self: Pin<&mut FXChainBackend>, initialized: bool);

        #[qinvokable]
        pub fn update(self: Pin<&mut FXChainBackend>);

        #[qinvokable]
        pub fn maybe_initialize_backend(self: Pin<&mut FXChainBackend>) -> bool;

        #[qsignal]
        pub unsafe fn state_changed(
            self: Pin<&mut FXChainBackend>,
            initialized: bool,
            ready: bool,
            active: bool,
            visible: bool,
        );

        #[qsignal]
        pub unsafe fn initialized_changed(self: Pin<&mut FXChainBackend>, initialized: bool);

        #[qsignal]
        pub unsafe fn title_changed(self: Pin<&mut FXChainBackend>, title: QString);

        #[qsignal]
        pub unsafe fn chain_type_changed(self: Pin<&mut FXChainBackend>, chain_type: i32);

        #[qsignal]
        pub unsafe fn ui_visible_changed(self: Pin<&mut FXChainBackend>, ui_visible: bool);

        #[qsignal]
        pub unsafe fn ready_changed(self: Pin<&mut FXChainBackend>, ready: bool);

        #[qsignal]
        pub unsafe fn active_changed(self: Pin<&mut FXChainBackend>, active: bool);

        #[qsignal]
        pub unsafe fn backend_changed(self: Pin<&mut FXChainBackend>, backend: *mut QObject);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qobject.h");

        #[rust_name = "from_qobject_ref_fx_chain_backend"]
        unsafe fn fromQObjectRef(obj: &QObject, output: *mut *const FXChainBackend);

        #[rust_name = "from_qobject_mut_fx_chain_backend"]
        unsafe fn fromQObjectMut(obj: Pin<&mut QObject>, output: *mut *mut FXChainBackend);

        #[rust_name = "fx_chain_backend_qobject_from_ptr"]
        unsafe fn qobjectFromPtr(obj: *mut FXChainBackend) -> *mut QObject;

        include!("cxx-qt-lib-shoop/make_raw.h");
        #[rust_name = "make_raw_fx_chain_backend"]
        unsafe fn make_raw() -> *mut FXChainBackend;
    }
}

use backend_bindings::{FXChain as BackendFXChain, FXChainState, FXChainType};
pub use ffi::FXChainBackend;
use ffi::*;

pub struct FXChainBackendRust {
    // Properties
    pub initialized: bool,
    pub backend: *mut QObject,

    // Other
    pub backend_chain_wrapper: Option<BackendFXChain>,
    pub prev_state: FXChainState,
    pub chain_type: Option<FXChainType>,
    pub title: Option<String>,
}

impl Default for FXChainBackendRust {
    fn default() -> FXChainBackendRust {
        FXChainBackendRust {
            initialized: false,
            backend: std::ptr::null_mut(),
            title: None,
            chain_type: None,
            backend_chain_wrapper: None,
            prev_state: FXChainState::default(),
        }
    }
}

impl cxx_qt_lib_shoop::qobject::FromQObject for FXChainBackend {
    unsafe fn ptr_from_qobject_ref(obj: &cxx_qt_lib_shoop::qobject::QObject) -> *const Self {
        let mut output: *const Self = std::ptr::null();
        from_qobject_ref_fx_chain_backend(obj, &mut output as *mut *const Self);
        output
    }

    unsafe fn ptr_from_qobject_mut(
        obj: std::pin::Pin<&mut cxx_qt_lib_shoop::qobject::QObject>,
    ) -> *mut Self {
        let mut output: *mut Self = std::ptr::null_mut();
        from_qobject_mut_fx_chain_backend(obj, &mut output as *mut *mut Self);
        output
    }
}
