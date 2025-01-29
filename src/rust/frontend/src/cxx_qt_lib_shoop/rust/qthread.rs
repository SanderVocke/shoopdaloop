use cxx::{type_id, ExternType};
use cxx_qt;
use std::pin::Pin;

#[cxx_qt::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qthread.h");
        type QThread = super::QThreadRust;
        type QObject = crate::cxx_qt_lib_shoop::qobject::QObject;

        #[rust_name = "qthread_start"]
        fn qthreadStart(thread: Pin<&mut QThread>);

        #[rust_name = "qthread_quit"]
        fn qthreadQuit(thread: Pin<&mut QThread>);

        #[rust_name = "qthread_wait"]
        fn qthreadWait(thread: Pin<&mut QThread>) -> bool;

        include!("cxx-qt-shoop/make_raw.h");
        #[rust_name = "make_raw_qthread_with_parent"]
        unsafe fn make_raw_with_one_arg(parent: *mut QObject) -> *mut QThread;
    }
}

pub use ffi::QThread;
pub use ffi::make_raw_qthread_with_parent as make_raw_with_parent;
use ffi::QObject;

#[repr(C)]
pub struct QThreadRust {
}

unsafe impl ExternType for QThread {
    type Id = type_id!("QThread");
    type Kind = cxx::kind::Opaque;
}

impl QThread {
    pub fn start(self: Pin<&mut Self>) {
        ffi::qthread_start(self);
    }

    pub fn quit(self: Pin<&mut Self>) {
        ffi::qthread_quit(self);
    }

    pub fn wait(self: Pin<&mut Self>) -> bool {
        ffi::qthread_wait(self)
    }
}
