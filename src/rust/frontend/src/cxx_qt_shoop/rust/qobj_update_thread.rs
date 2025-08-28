use cxx_qt::CxxQtType;

use crate::cxx_qt_shoop::qobj_update_thread_bridge::ffi::*;
use crate::cxx_qt_shoop::qobj_update_thread_bridge::{
    UpdateThread, DEFAULT_BACKUP_UPDATE_INTERVAL_MS,
};
use core::pin::Pin;
use cxx_qt_lib_shoop::qobject::ffi::qobject_move_to_thread;
use cxx_qt_lib_shoop::qobject::AsQObject;
use cxx_qt_lib_shoop::{connection_types, invokable};
use std::slice;
use std::time::{Duration, Instant};

use common::logging::macros::*;
shoop_log_unit!("Frontend.UpdateThread");

fn update_interval_to_elapsed_threshold(interval_ms: f32) -> Duration {
    Duration::from_millis(interval_ms as u64)
}

impl UpdateThread {
    pub fn initilialize_impl(mut self: core::pin::Pin<&mut Self>) {
        unsafe {
            let self_qobject = self.as_mut().pin_mut_qobject_ptr();
            let mut rust = self.as_mut().rust_mut();

            rust.backup_timer_elapsed_threshold =
                update_interval_to_elapsed_threshold(DEFAULT_BACKUP_UPDATE_INTERVAL_MS as f32);

            let thread_mut_ref = &mut *rust.thread;
            let thread_slice = slice::from_raw_parts_mut(thread_mut_ref, 1);
            let mut thread: Pin<&mut QThread> = Pin::new_unchecked(&mut thread_slice[0]);

            let timer_mut_ref = &mut *rust.backup_timer;
            let timer_slice = slice::from_raw_parts_mut(timer_mut_ref, 1);
            let mut timer: Pin<&mut QTimer> = Pin::new_unchecked(&mut timer_slice[0]);
            if !timer
                .as_mut()
                .move_to_thread(rust.thread.as_mut().unwrap())
                .unwrap()
            {
                error!("Failed to move update timer to thread");
            }

            // Start the backup timer at about 40Hz. The idea is that an external source
            // should be triggering our updates (e.g. screen refresh rate), but the backup
            // timer will take over in case of stalling.
            // TODO: implement the screen refresh
            timer
                .as_mut()
                .set_interval(DEFAULT_BACKUP_UPDATE_INTERVAL_MS);
            timer.as_mut().set_single_shot(false);
            timer
                .as_mut()
                .connect_timeout(self_qobject, "timer_tick()")
                .unwrap();
            thread
                .as_mut()
                .connect_started(timer.as_mut().qobject_from_ptr(), "start()")
                .unwrap();
            thread
                .as_mut()
                .connect_started(self_qobject, "update_thread_started()")
                .unwrap();
            thread.as_mut().start();

            qobject_move_to_thread(self_qobject, rust.thread).unwrap();
        }
    }

    pub fn frontend_frame_swapped(self: Pin<&mut UpdateThread>) {
        if *self.as_ref().trigger_update_on_frame_swapped() {
            let elapsed = self.trigger_update();
            trace!("Updated (frame swapped) - took {elapsed:?}");
        }
    }

    pub fn trigger_update(mut self: Pin<&mut Self>) -> Duration {
        let mut rust = self.as_mut().rust_mut();
        let start = Instant::now();
        rust.last_updated = Some(start);
        self.update();
        start.elapsed()
    }

    pub fn trigger_update_if_enough_time_elapsed(self: Pin<&mut Self>) -> Option<Duration> {
        let last_updated = self.as_ref().last_updated;
        let do_update: bool = last_updated.is_none()
            || Instant::now().duration_since(last_updated.unwrap())
                > self.as_ref().backup_timer_elapsed_threshold;
        if do_update {
            return Some(self.trigger_update());
        } else {
            return None;
        }
    }

    pub fn timer_tick(self: Pin<&mut Self>) {
        if let Some(elapsed) = self.trigger_update_if_enough_time_elapsed() {
            trace!("Updated (timer expired) - took {elapsed:?}");
        }
    }

    pub fn set_backup_timer_interval_ms(mut self: Pin<&mut UpdateThread>, interval_ms: i32) {
        let mut rust = self.as_mut().rust_mut();
        rust.backup_timer_elapsed_threshold =
            update_interval_to_elapsed_threshold(interval_ms as f32);
        rust.backup_timer_interval_ms = interval_ms;
        unsafe {
            let timer_mut_ref = &mut *rust.backup_timer;
            let timer_slice = slice::from_raw_parts_mut(timer_mut_ref, 1);
            let timer: Pin<&mut QTimer> = Pin::new_unchecked(&mut timer_slice[0]);
            match invokable::invoke::<_, (), _>(
                &mut *timer.qobject_from_ptr(),
                "start(int)",
                connection_types::QUEUED_CONNECTION,
                &interval_ms,
            ) {
                Ok(_) => (),
                Err(e) => error!("Failed to restart backup timer with interval. Error: {}", e),
            }
        }
    }

    pub fn make_unique() -> cxx::UniquePtr<UpdateThread> {
        crate::cxx_qt_shoop::qobj_update_thread_bridge::ffi::make_unique_update_thread()
    }

    pub fn get_thread(self: Pin<&mut UpdateThread>) -> *mut QThread {
        self.as_ref().thread
    }

    pub fn update_thread_started(self: Pin<&mut UpdateThread>) {
        crashhandling::registered_threads::register_thread("backend_update");
    }
}
