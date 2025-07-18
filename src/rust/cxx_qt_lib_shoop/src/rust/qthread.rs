use crate::qobject::QObject;
use cxx::{type_id, ExternType};
use cxx_qt;
use std::pin::Pin;

#[cxx_qt::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qthread.h");
        include!("cxx-qt-lib-shoop/qobject.h");
        type QThread = super::QThreadRust;
        type QObject = crate::qobject::QObject;

        #[rust_name = "qthread_start"]
        fn qthreadStart(thread: Pin<&mut QThread>);

        #[rust_name = "qthread_exit"]
        fn qthreadExit(thread: Pin<&mut QThread>);

        #[rust_name = "qthread_is_running"]
        fn qthreadIsRunning(thread: Pin<&mut QThread>) -> bool;

        include!("cxx-qt-lib-shoop/make_raw.h");
        #[rust_name = "make_raw_qthread_with_parent"]
        unsafe fn make_raw_with_one_arg(parent: *mut QObject) -> *mut QThread;

        #[rust_name = "make_raw_qthread"]
        unsafe fn make_raw() -> *mut QThread;

        #[rust_name = "delete_raw_qthread"]
        unsafe fn delete_raw(thread: *mut QThread);

        #[rust_name = "_qobject_from_ptr_qthread"]
        unsafe fn qobjectFromPtr(thread: *mut QThread) -> *mut QObject;

        #[rust_name = "qthread_connect_started"]
        unsafe fn qthreadConnectStarted(
            thread: Pin<&mut QThread>,
            receiver: *mut QObject,
            slot: String,
        ) -> Result<()>;
    }
}

pub use ffi::make_raw_qthread_with_parent as make_raw_with_parent;
pub use ffi::QThread;

#[repr(C)]
pub struct QThreadRust {}

unsafe impl ExternType for QThread {
    type Id = type_id!("QThread");
    type Kind = cxx::kind::Opaque;
}

impl QThread {
    pub fn start(self: Pin<&mut Self>) {
        ffi::qthread_start(self);
    }

    pub fn exit(self: Pin<&mut Self>) {
        ffi::qthread_exit(self);
    }

    pub fn is_running(self: Pin<&mut Self>) -> bool {
        ffi::qthread_is_running(self)
    }

    pub fn make_raw_with_parent(parent: *mut QObject) -> *mut QThread {
        unsafe { ffi::make_raw_qthread_with_parent(parent) }
    }

    pub fn make_raw() -> *mut QThread {
        unsafe { ffi::make_raw_qthread() }
    }

    pub fn delete_raw(thread: *mut QThread) {
        unsafe { ffi::delete_raw_qthread(thread) }
    }

    pub fn connect_started(
        self: Pin<&mut Self>,
        receiver: *mut QObject,
        slot: String,
    ) -> Result<(), cxx::Exception> {
        unsafe { ffi::qthread_connect_started(self, receiver, slot) }
    }
}
