use common::logging::macros::*;
use std::time::{Duration, Instant};
shoop_log_unit!("Frontend.UpdateThread");

pub const DEFAULT_BACKUP_UPDATE_INTERVAL_MS: i32 = 25;
#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qthread.h");
        type QThread = cxx_qt_lib_shoop::qthread::QThread;

        include!("cxx-qt-lib-shoop/qtimer.h");
        type QTimer = cxx_qt_lib_shoop::qtimer::QTimer;

        include!("cxx-qt-lib-shoop/qobject.h");
        type QObject = cxx_qt_lib_shoop::qobject::QObject;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qproperty(bool, trigger_update_on_frame_swapped)]
        #[qproperty(i32, backup_timer_interval_ms, READ, WRITE=set_backup_timer_interval_ms)]
        type UpdateThread = super::UpdateThreadRust;

        #[qsignal]
        fn update(self: Pin<&mut UpdateThread>);

        #[qinvokable]
        pub fn timer_tick(self: Pin<&mut UpdateThread>);

        #[qinvokable]
        pub fn get_thread(self: Pin<&mut UpdateThread>) -> *mut QThread;

        #[qinvokable]
        pub fn set_backup_timer_interval_ms(self: Pin<&mut UpdateThread>, interval_ms: i32);

        #[qinvokable]
        pub fn frontend_frame_swapped(self: Pin<&mut UpdateThread>);

        #[qinvokable]
        pub fn update_thread_started(self: Pin<&mut UpdateThread>);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/make_unique.h");

        #[rust_name = "make_unique_update_thread"]
        fn make_unique() -> UniquePtr<UpdateThread>;

        include!("cxx-qt-lib-shoop/qobject.h");
        #[rust_name = "update_thread_qobject_from_ptr"]
        unsafe fn qobjectFromPtr(obj: *mut UpdateThread) -> *mut QObject;

        #[rust_name = "update_thread_qobject_from_ref"]
        fn qobjectFromRef(obj: &UpdateThread) -> &QObject;
    }

    impl cxx_qt::Constructor<()> for UpdateThread {}
}

use cxx_qt_lib_shoop::qobject::AsQObject;
pub use ffi::UpdateThread;

impl AsQObject for UpdateThread {
    unsafe fn mut_qobject_ptr(&mut self) -> *mut ffi::QObject {
        ffi::update_thread_qobject_from_ptr(self as *mut Self)
    }

    unsafe fn ref_qobject_ptr(&self) -> *const ffi::QObject {
        ffi::update_thread_qobject_from_ref(self) as *const ffi::QObject
    }
}

pub struct UpdateThreadRust {
    pub thread: *mut ffi::QThread,
    pub backup_timer: *mut ffi::QTimer,
    pub last_updated: Option<Instant>,
    pub backup_timer_elapsed_threshold: Duration,
    pub trigger_update_on_frame_swapped: bool,
    pub backup_timer_interval_ms: i32,
}

impl Default for UpdateThreadRust {
    fn default() -> UpdateThreadRust {
        UpdateThreadRust {
            thread: ffi::QThread::make_raw(),
            backup_timer: ffi::QTimer::make_raw(),
            last_updated: None,
            backup_timer_elapsed_threshold: Duration::default(),
            trigger_update_on_frame_swapped: true,
            backup_timer_interval_ms: 25,
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

    fn route_arguments(
        _args: (),
    ) -> (
        Self::NewArguments,
        Self::BaseArguments,
        Self::InitializeArguments,
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
