use crate::cxx_qt_shoop::qobj_async_task_bridge::ffi::async_task_qobject_from_ref;
use crate::cxx_qt_shoop::{qobj_async_task_bridge::AsyncTask, qobj_async_tasks_bridge::ffi::*};
use common::logging::macros::*;
use cxx_qt::CxxQtType;
use cxx_qt_lib_shoop::invokable::invoke;
use cxx_qt_lib_shoop::qobject::AsQObject;
use cxx_qt_lib_shoop::{
    connect::connect_or_report,
    connection_types,
    qobject::{qobject_property_bool, FromQObject},
};
use std::pin::Pin;
shoop_log_unit!("Frontend.AsyncTask");

impl AsyncTasks {

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
                self.active_changed(cur_active);
            }
        }
    }

    pub fn get_active(self: &AsyncTasks) -> bool {
        self.calc_active()
    }

    pub fn then(self: Pin<&mut AsyncTasks>, func: QVariant) {
        todo!();
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
}
