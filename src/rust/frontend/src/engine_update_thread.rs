use crate::cxx_qt_shoop::qobj_update_thread_bridge::UpdateThread;
use once_cell::sync::Lazy;

struct UpdateThreadWrapper {
    thread: cxx::UniquePtr<UpdateThread>,
}

impl Default for UpdateThreadWrapper {
    fn default() -> Self {
        Self {
            thread: UpdateThread::make_unique(),
        }
    }
}

unsafe impl Send for UpdateThreadWrapper {}
unsafe impl Sync for UpdateThreadWrapper {}

static mut ENGINE_UPDATE_THREAD: Lazy<UpdateThreadWrapper> =
    Lazy::new(|| UpdateThreadWrapper::default());

pub fn get_engine_update_thread() -> &'static mut UpdateThread {
    unsafe {
        let ptr: *mut once_cell::sync::Lazy<UpdateThreadWrapper> = &raw mut ENGINE_UPDATE_THREAD;
        let ptr: *mut UpdateThread = Lazy::force_mut(ptr.as_mut().unwrap()).thread.as_mut_ptr();
        &mut *ptr
    }
}

pub fn init() {
    unsafe {
        let ptr: *const once_cell::sync::Lazy<UpdateThreadWrapper> =
            &raw const ENGINE_UPDATE_THREAD;
        let _ = Lazy::force(ptr.as_ref().unwrap());
    }
}
