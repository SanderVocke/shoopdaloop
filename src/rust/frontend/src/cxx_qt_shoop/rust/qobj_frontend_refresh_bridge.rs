use common::logging::macros::*;
shoop_log_unit!("Frontend.Refresh");

pub const DEFAULT_FALLBACK_REFRESH_INTERVAL_MS: i32 = 25;

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qtimer.h");
        type QTimer = cxx_qt_lib_shoop::qtimer::QTimer;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qproperty(i32, fallback_interval_ms, READ, WRITE=set_fallback_interval_ms)]
        type FrontendRefresh = super::FrontendRefreshRust;

        #[qsignal]
        fn refresh(self: Pin<&mut FrontendRefresh>);

        #[qinvokable]
        pub fn request_refresh(self: Pin<&mut FrontendRefresh>);

        #[qinvokable]
        pub fn do_queued_refresh(self: Pin<&mut FrontendRefresh>);

        #[qinvokable]
        pub fn refresh_now(self: Pin<&mut FrontendRefresh>);

        #[qinvokable]
        pub fn timer_tick(self: Pin<&mut FrontendRefresh>);

        #[qinvokable]
        pub fn set_fallback_interval_ms(self: Pin<&mut FrontendRefresh>, interval_ms: i32);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qobject.h");
        include!("cxx-qt-lib-shoop/make_unique.h");

        #[rust_name = "make_unique_frontend_refresh"]
        fn make_unique() -> UniquePtr<FrontendRefresh>;

        #[rust_name = "frontend_refresh_qobject_from_ptr"]
        unsafe fn qobjectFromPtr(obj: *mut FrontendRefresh) -> *mut QObject;

        #[rust_name = "frontend_refresh_qobject_from_ref"]
        fn qobjectFromRef(obj: &FrontendRefresh) -> &QObject;
    }

    impl cxx_qt::Constructor<()> for FrontendRefresh {}
}

use cxx_qt_lib_shoop::qobject::AsQObject;
pub use ffi::FrontendRefresh;

impl AsQObject for FrontendRefresh {
    unsafe fn mut_qobject_ptr(&mut self) -> *mut ffi::QObject {
        ffi::frontend_refresh_qobject_from_ptr(self as *mut Self)
    }

    unsafe fn ref_qobject_ptr(&self) -> *const ffi::QObject {
        ffi::frontend_refresh_qobject_from_ref(self) as *const ffi::QObject
    }
}

pub struct FrontendRefreshRust {
    pub fallback_timer: *mut ffi::QTimer,
    pub fallback_interval_ms: i32,
    pub refresh_queued: bool,
}

impl Default for FrontendRefreshRust {
    fn default() -> Self {
        Self {
            fallback_timer: std::ptr::null_mut(),
            fallback_interval_ms: DEFAULT_FALLBACK_REFRESH_INTERVAL_MS,
            refresh_queued: false,
        }
    }
}

impl cxx_qt::Constructor<()> for FrontendRefresh {
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

    fn new(_args: ()) -> FrontendRefreshRust {
        FrontendRefreshRust::default()
    }

    fn initialize(self: core::pin::Pin<&mut Self>, _: Self::InitializeArguments) {
        FrontendRefresh::initialize_impl(self);
    }
}
