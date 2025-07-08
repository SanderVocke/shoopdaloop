use cxx_qt::CxxQtType;

use crate::cxx_qt_lib_shoop::qobject::AsQObject;
use crate::cxx_qt_shoop::qobj_update_thread_bridge::UpdateThread;
use crate::cxx_qt_shoop::qobj_update_thread_bridge::ffi::*;  
use std::slice;
use core::pin::Pin;
use std::time::{Instant, Duration};

use common::logging::macros::*;
shoop_log_unit!("Frontend.UpdateThread");

const BACKUP_UPDATE_INTERVAL_MS : u64 = 25;
const BACKUP_ELAPSED_THRESHOLD : Duration
    = Duration::from_millis((BACKUP_UPDATE_INTERVAL_MS as f32 * 0.8) as u64);

impl UpdateThread {
    pub fn initilialize_impl(mut self: core::pin::Pin<&mut Self>) {
        unsafe {
            let self_qobject = self.as_mut().pin_mut_qobject_ptr();
            let mut rust = self.as_mut().rust_mut();

            let thread_mut_ref = &mut *rust.thread;
            let thread_slice = slice::from_raw_parts_mut(thread_mut_ref, 1);
            let mut thread : Pin<&mut QThread> = Pin::new_unchecked(&mut thread_slice[0]);
        
            let timer_mut_ref = &mut *rust.backup_timer;
            let timer_slice = slice::from_raw_parts_mut(timer_mut_ref, 1);
            let mut timer : Pin<&mut QTimer> = Pin::new_unchecked(&mut timer_slice[0]);
            if !timer.as_mut().move_to_thread(rust.thread.as_mut().unwrap()).unwrap() {
                error!("Failed to move update timer to thread");
            }

            // Start the backup timer at about 40Hz. The idea is that an external source 
            // should be triggering our updates (e.g. screen refresh rate), but the backup
            // timer will take over in case of stalling.
            // TODO: implement the screen refresh
            timer.as_mut().set_interval(25);
            timer.as_mut().set_single_shot(false);
            timer.as_mut().connect_timeout(self_qobject, "timer_tick()".to_string()).unwrap();
            thread.as_mut().connect_started(timer.as_mut().qobject_from_ptr(), "start()".to_string()).unwrap();
            thread.as_mut().start();
        }
    }

    pub fn trigger_update(mut self: Pin<&mut Self>) {
        let mut rust = self.as_mut().rust_mut();
        rust.last_updated = Some(Instant::now());
        self.update();
    }

    pub fn timer_tick(self: Pin<&mut Self>) {
        let last_updated = self.as_ref().last_updated;
        let update : bool =
            last_updated.is_none() ||
            Instant::now().duration_since(last_updated.unwrap()) > BACKUP_ELAPSED_THRESHOLD;
        if update {
            self.trigger_update();
        }
    }

    pub fn make_unique() -> cxx::UniquePtr<UpdateThread> {
        crate::cxx_qt_shoop::qobj_update_thread_bridge::ffi::make_unique_update_thread()
    }

    pub fn get_thread(self: Pin<&mut UpdateThread>) -> *mut QThread {
        self.as_ref().thread
    }
}