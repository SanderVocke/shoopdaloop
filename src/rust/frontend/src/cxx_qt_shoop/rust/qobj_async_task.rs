use crate::cxx_qt_shoop::qobj_async_task_bridge::ffi::*;
use common::logging::macros::*;
use cxx_qt::CxxQtType;
use cxx_qt_lib_shoop::{connection_types, invokable::invoke, qobject::AsQObject};
use std::pin::Pin;
shoop_log_unit!("Frontend.AsyncTask");

impl AsyncTask {
    pub fn exec_concurrent_then_finish<F>(mut self: Pin<&mut Self>, concurrent_fn: F)
    where
        F: FnOnce() -> Result<(), anyhow::Error> + Send + 'static,
    {
        struct SelfPtr { ptr: *mut QObject }
        unsafe impl Send for SelfPtr {}

        let self_ptr = SelfPtr{ ptr: unsafe { self.as_mut().pin_mut_qobject_ptr() } };

        let _ = std::thread::spawn(move || -> Result<(), anyhow::Error> {
            unsafe {
                let ptr = self_ptr;
                concurrent_fn()?;
                invoke::<_, (), _>(
                    &mut *ptr.ptr,
                    "notify_done()",
                    connection_types::BLOCKING_QUEUED_CONNECTION,
                    &(),
                )?;
                invoke::<_, (), _>(
                    &mut *ptr.ptr,
                    "deleteLater()",
                    connection_types::QUEUED_CONNECTION,
                    &(),
                )?;
            }
            Ok(())
        });
    }

    pub fn notify_done(mut self: Pin<&mut AsyncTask>) {
        if self.active {
            self.as_mut().rust_mut().active = false;
            unsafe {
                self.as_mut().active_changed(false);
            }
        } else {
            warn!("Notify done on already inactive task.");
        }
    }

    pub fn then(self: Pin<&mut AsyncTask>, func: QVariant) {
        todo!();
    }
}
