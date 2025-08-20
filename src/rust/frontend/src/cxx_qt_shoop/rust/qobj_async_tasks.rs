use crate::cxx_qt_shoop::qobj_async_task_bridge::ffi::async_task_qobject_from_ref;
use crate::cxx_qt_shoop::{qobj_async_task_bridge::AsyncTask, qobj_async_tasks_bridge::ffi::*};
use crate::cxx_qt_shoop::qobj_async_tasks_bridge::AsyncTasksRust;
use common::logging::macros::*;
use cxx_qt::CxxQtType;
use cxx_qt_lib_shoop::invokable::invoke;
use cxx_qt_lib_shoop::qobject::AsQObject;
use cxx_qt_lib_shoop::qtimer::QTimer;
use cxx_qt_lib_shoop::{
    connect::connect_or_report,
    connection_types,
    qobject::{qobject_property_bool, FromQObject},
};
use std::pin::Pin;
use std::slice;
shoop_log_unit!("Frontend.AsyncTasks");

impl AsyncTasks {
    pub fn initialize_impl(mut self: Pin<&mut AsyncTasks>) {
        let self_ptr = unsafe { self.as_mut().pin_mut_qobject_ptr() };
        let timer = unsafe { cxx_qt_lib_shoop::qtimer::make_raw_with_parent(self_ptr) };
        self.as_mut().rust_mut().timer = timer;
    }

    fn calc_active(&self) -> bool {
        self.n_done < self.n_tracking
    }

    pub fn notify_task_active_changed(mut self: Pin<&mut AsyncTasks>, task_active: bool) {
        let prev_active = self.calc_active();
        if task_active {
            self.as_mut().rust_mut().n_done -= 1;
        } else {
            self.as_mut().rust_mut().n_done += 1;
        }
        let cur_active = self.calc_active();
        let statusstr = if task_active {
            "became active"
        } else {
            "finished"
        };
        let n_active = self.n_tracking - self.n_done;
        let n_tracking = self.n_tracking;
        debug!("Subtask: {statusstr}. Active: {n_active}/{n_tracking}.");
        if cur_active != prev_active {
            unsafe {
                self.as_mut().active_changed(cur_active);
            }
            if !cur_active {
                self.as_mut().done_impl();
            }
        }
    }

    pub fn get_active(self: &AsyncTasks) -> bool {
        self.calc_active()
    }

    pub fn then_delete(mut self: Pin<&mut AsyncTasks>) {
        self.as_mut().rust_mut().delete_when_done = true;
        if !self.calc_active() {
            self.done_impl();
        }
    }

    pub fn then(mut self: Pin<&mut AsyncTasks>, func: QVariant) {
        self.as_mut().rust_mut().maybe_qml_callable = func;
        if !self.calc_active() {
            self.done_impl();
        }
    }

    pub fn done_impl(mut self: Pin<&mut AsyncTasks>) {
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
            } else if self.maybe_qml_callable.is_valid() {
                // TODO: call the callable
                todo!();
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

    pub fn add_subtask(mut self: Pin<&mut AsyncTasks>, subtask_qobj: *mut QObject) {
        let prev_active = self.calc_active();
        self.as_mut().rust_mut().n_tracking += 1;
        let subtask_active =
            if let Ok(false) = unsafe { qobject_property_bool(&*subtask_qobj, "active") } {
                true
            } else {
                false
            };
        if subtask_active {
            self.as_mut().rust_mut().n_done += 1;
        }
        let cur_active = self.calc_active();
        if prev_active != cur_active {
            unsafe {
                self.as_mut().active_changed(cur_active);
            }
        }
        unsafe {
            connect_or_report(
                &*subtask_qobj,
                "active_changed(bool)",
                &*self.as_mut().pin_mut_qobject_ptr(),
                "notify_task_active_changed(bool)",
                connection_types::QUEUED_CONNECTION,
            );
        }

        let n_active = self.n_tracking - self.n_done;
        let n_tracking = self.n_tracking;
        debug!("Added task with active: {subtask_active}. Total active: {n_active}/{n_tracking}");
    }

    pub fn not_deleted_error(self: Pin<&mut AsyncTasks>) {
        error!("timeout expired after completion, no follow-up registered")
    }
}

impl Drop for AsyncTasksRust {
    fn drop(&mut self) {
        if self.n_done < self.n_tracking {
            error!("dropped while still active!");
        }
    }
}
