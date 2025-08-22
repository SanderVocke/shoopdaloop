use crate::cxx_qt_shoop::qobj_async_task_bridge::ffi::*;
use crate::cxx_qt_shoop::qobj_async_task_bridge::AsyncTaskRust;
use common::logging::macros::*;
use cxx_qt::CxxQtType;
use cxx_qt_lib_shoop::qjsvalue::qvariant_call_as_callable_qjsvalue;
use cxx_qt_lib_shoop::{connection_types, invokable::invoke, qobject::AsQObject, qtimer::QTimer};
use std::{pin::Pin, slice};
shoop_log_unit!("Frontend.AsyncTask");

impl AsyncTask {
    pub fn self_rust_ptr(&self) -> *const AsyncTaskRust {
        self.rust() as *const AsyncTaskRust
    }

    pub fn finish_dummy(self: Pin<&mut Self>) {
        let self_ptr = self.self_rust_ptr();
        debug!("{self_ptr:?}: finish dummy");
        self.notify_done(true);
    }

    // To be called by users on the Rust side. Will execute the given closure
    // on a dedicated temporary thread, then notify the async task for follow-up.
    pub fn exec_concurrent_rust_then_finish<F>(mut self: Pin<&mut Self>, concurrent_fn: F)
    where
        F: FnOnce() -> Result<(), anyhow::Error> + Send + 'static,
    {
        let self_ptr = self.self_rust_ptr();
        debug!("{self_ptr:?}: start exec rust");

        struct SelfPtr {
            ptr: *mut QObject,
        }
        unsafe impl Send for SelfPtr {}

        let self_ptr = SelfPtr {
            ptr: unsafe { self.as_mut().pin_mut_qobject_ptr() },
        };

        let _ = std::thread::spawn(move || unsafe {
            let ptr = self_ptr;
            let mut success = true;
            if let Err(e) = concurrent_fn() {
                error!("async task failed: {e}");
                success = false;
            }
            if let Err(e) = invoke::<_, (), _>(
                &mut *ptr.ptr,
                "notify_done(bool)",
                connection_types::BLOCKING_QUEUED_CONNECTION,
                &(success),
            ) {
                error!("Failed to notify after async task: {e}");
            }
        });
    }

    pub fn notify_done(mut self: Pin<&mut AsyncTask>, success: bool) {
        let self_ptr = self.self_rust_ptr();
        debug!("{self_ptr:?}: notify done (success: {success})");

        self.as_mut().rust_mut().success = success;
        if self.active {
            self.as_mut().rust_mut().active = false;
            unsafe {
                self.as_mut().active_changed(false);
            }
        } else {
            warn!("Notify done on already inactive task.");
        }
        self.as_mut().done_impl();
    }

    pub fn done_impl(mut self: Pin<&mut AsyncTask>) {
        let self_ptr = self.self_rust_ptr();
        debug!("{self_ptr:?}: done impl");

        unsafe {
            let self_ptr = self.as_mut().pin_mut_qobject_ptr();
            let delete_self = || {
                if let Err(e) = invoke::<_, (), _>(
                    &mut *self_ptr,
                    "deleteLater()",
                    connection_types::QUEUED_CONNECTION,
                    &(),
                ) {
                    error!("Could not delete async task: {e}");
                }
            };

            if self.delete_when_done {
                delete_self();
            } else if !self.maybe_qml_callable.is_null() {
                if let Err(e) = qvariant_call_as_callable_qjsvalue(&self.maybe_qml_callable) {
                    error!("Could not call callable: {e}");
                }
                delete_self();
            } else {
                // no post-actions set. Set a timer to create an error
                // if this remains the case
                let self_qobj = self.as_mut().pin_mut_qobject_ptr();
                let timer_mut_ref = &mut *self.timer;
                let timer_slice = slice::from_raw_parts_mut(timer_mut_ref, 1);
                let mut timer: Pin<&mut QTimer> = Pin::new_unchecked(&mut timer_slice[0]);
                timer.as_mut().set_interval(3000);
                timer.as_mut().set_single_shot(false);
                if let Err(e) = timer
                    .as_mut()
                    .connect_timeout(self_qobj, "not_deleted_error()")
                {
                    error!("Could not connect to timer: {e}");
                }
                timer.as_mut().start();
            }
        }
    }

    pub fn then_delete(mut self: Pin<&mut AsyncTask>) {
        self.as_mut().rust_mut().delete_when_done = true;
        if !self.active {
            self.done_impl();
        }
    }

    pub fn then(mut self: Pin<&mut AsyncTask>, func: QVariant) {
        self.as_mut().rust_mut().maybe_qml_callable = func;
        if !self.active {
            self.done_impl();
        }
    }

    pub fn initialize_impl(mut self: Pin<&mut AsyncTask>) {
        let self_ptr = unsafe { self.as_mut().pin_mut_qobject_ptr() };
        debug!("{:?}: created", self.as_mut().self_rust_ptr());
        let timer = unsafe { cxx_qt_lib_shoop::qtimer::make_raw_with_parent(self_ptr) };
        self.as_mut().rust_mut().timer = timer;
    }

    pub fn not_deleted_error(self: Pin<&mut AsyncTask>) {
        error!("timeout expired after completion, no follow-up registered")
    }
}

impl Drop for AsyncTaskRust {
    fn drop(&mut self) {
        if self.active {
            error!("dropped while still active!");
        }
        let self_ptr = self as *const Self;
        debug!("{self_ptr:?}: drop");
    }
}

pub fn register_qml_type(module_name: &str, type_name: &str) {
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    unsafe {
        register_qml_type_async_task(std::ptr::null_mut(), &mut mdl, 1, 0, &mut tp);
    }
}
