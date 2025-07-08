use once_cell::sync::Lazy;
use crate::cxx_qt_shoop::qobj_update_thread_bridge::UpdateThread;

struct UpdateThreadWrapper {
    thread : cxx::UniquePtr<UpdateThread>
}

impl Default for UpdateThreadWrapper {
    fn default() -> Self {
        Self {
            thread : UpdateThread::make_unique()
        }
    }
}

unsafe impl Send for UpdateThreadWrapper {}
unsafe impl Sync for UpdateThreadWrapper {}

static mut ENGINE_UPDATE_THREAD : Lazy<UpdateThreadWrapper> = Lazy::new(|| {
    UpdateThreadWrapper::default()
});

pub fn get_engine_update_thread() -> &'static mut UpdateThread {
    unsafe {
        let ptr : *mut UpdateThread = Lazy::force_mut(&mut ENGINE_UPDATE_THREAD).thread.as_mut_ptr();
        &mut *ptr
    }
}

pub fn init() {
    unsafe { let _ = Lazy::force(&ENGINE_UPDATE_THREAD); }
}