use cxx::{type_id, ExternType};
use cxx_qt;
use std::pin::Pin;

#[cxx_qt::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qtimer.h");
        type QTimer = super::QTimerRust;
        type QObject = crate::cxx_qt_lib_shoop::qobject::QObject;

        #[rust_name = "qtimer_set_single_shot"]
        fn qtimerSetSingleShot(timer : Pin<&mut QTimer>, single_shot : bool);

        #[rust_name = "qtimer_set_interval"]
        fn qtimerSetInterval(timer : Pin<&mut QTimer>, interval : i32);

        #[rust_name = "qtimer_start"]
        fn qtimerStart(timer : Pin<&mut QTimer>);

        #[rust_name = "qtimer_stop"]
        fn qtimerStop(timer : Pin<&mut QTimer>);

        #[rust_name = "qtimer_connect_timeout"]
        unsafe fn qtimerConnectTimeout(timer : Pin<&mut QTimer>, receiver : *mut QObject, slot : String) -> Result<()>;

        include!("cxx-qt-shoop/make_raw.h");
        #[rust_name = "make_raw_qtimer_with_parent"]
        unsafe fn make_raw_with_one_arg(parent : *mut QObject) -> *mut QTimer;
    }
}

pub use ffi::QTimer;
pub use ffi::make_raw_qtimer_with_parent as make_raw_with_parent;
use ffi::QObject;

#[repr(C)]
pub struct QTimerRust {
}

unsafe impl ExternType for QTimer {
    type Id = type_id!("QTimer");
    type Kind = cxx::kind::Opaque;
}

impl QTimer {
    pub fn set_single_shot(self : Pin<&mut Self>, single_shot : bool) {
        ffi::qtimer_set_single_shot(self, single_shot);
    }

    pub fn set_interval(self : Pin<&mut Self>, interval : i32) {
        ffi::qtimer_set_interval(self, interval);
    }

    pub fn start(self : Pin<&mut Self>) {
        ffi::qtimer_start(self);
    }

    pub fn stop(self : Pin<&mut Self>) {
        ffi::qtimer_stop(self);
    }

    pub fn connect_timeout(self : Pin<&mut Self>, receiver : *mut QObject, slot : String) -> Result<(), cxx::Exception> {
        unsafe { ffi::qtimer_connect_timeout(self, receiver, slot) }
    }
}
