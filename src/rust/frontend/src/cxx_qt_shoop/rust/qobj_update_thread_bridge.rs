use common::logging::macros::*;
shoop_log_unit!("Frontend.UpdateThread");

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qthread.h");
        type QThread = crate::cxx_qt_lib_shoop::qthread::QThread;

        include!("cxx-qt-lib-shoop/qtimer.h");
        type QTimer = crate::cxx_qt_lib_shoop::qtimer::QTimer;

        include!("cxx-qt-lib-shoop/qobject.h");
        type QObject = crate::cxx_qt_lib_shoop::qobject::QObject;
    }

    unsafe extern "RustQt" {
        #[qobject]
        type UpdateThread = super::UpdateThreadRust;
    }

    unsafe extern "RustQt" {
        #[qsignal]
        fn update(self: Pin<&mut UpdateThread>);

        #[qinvokable]
        pub fn trigger_update(self: Pin<&mut UpdateThread>);

        #[qinvokable]
        pub fn get_thread(self: Pin<&mut UpdateThread>) -> *mut QThread;
    }

    unsafe extern "C++" {
        include!("cxx-qt-shoop/make_unique.h");

        #[rust_name = "make_unique_update_thread"]
        fn make_unique() -> UniquePtr<UpdateThread>;

        include!("cxx-qt-lib-shoop/qobject.h");
        #[rust_name = "update_thread_qobject_from_ptr"]
        unsafe fn qobjectFromPtr(obj : *mut UpdateThread) -> *mut QObject;

        #[rust_name = "update_thread_qobject_from_ref"]
        fn qobjectFromRef(obj : &UpdateThread) -> &QObject;
    }

    impl cxx_qt::Constructor<()> for UpdateThread {}
}

pub use ffi::UpdateThread;
use crate::cxx_qt_lib_shoop::qobject::AsQObject;

impl AsQObject for UpdateThread {
    unsafe fn mut_qobject_ptr(&mut self) -> *mut ffi::QObject {
        ffi::update_thread_qobject_from_ptr(self as *mut Self)
    }

    unsafe fn ref_qobject_ptr(&self) -> *const ffi::QObject {
        ffi::update_thread_qobject_from_ref(self) as *const ffi::QObject
    }
}

pub struct UpdateThreadRust {
    pub thread : *mut ffi::QThread,
    pub backup_timer : *mut ffi::QTimer,
}

impl Default for UpdateThreadRust {
    fn default() -> UpdateThreadRust {
        UpdateThreadRust {
            thread : ffi::QThread::make_raw(),
            backup_timer : ffi::QTimer::make_raw(),
        }
    }
}

impl Drop for UpdateThreadRust {
    fn drop(&mut self) {
        if !self.thread.is_null() {
            // TODO: delete timer
            ffi::QThread::delete_raw(self.thread);
        }
    }
}

impl cxx_qt::Constructor<()> for UpdateThread {
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

    fn new(_args: ()) -> UpdateThreadRust {
        UpdateThreadRust::default()
    }

    fn initialize(self: core::pin::Pin<&mut Self>, _: Self::InitializeArguments) {
        UpdateThread::initilialize_impl(self);
    }
}