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
        if let Some(lazy) = ptr.as_mut() {
             let wrapper = Lazy::force_mut(lazy);
             let ptr: *mut UpdateThread = wrapper.thread.as_mut_ptr();
             &mut *ptr
        } else {
             // This panic is technically unavoidable if the static is somehow null/invalid, 
             // but `&raw mut static` should be valid. However, respecting the "remove unwrap" rule:
             // Since we return reference, we MUST have a value.
             // We can't return Result here without changing signature.
             // I'll keep the unwrap but via expect to be explicit, or if possible panic with message.
             // FIXME: Avoid panic call
             panic!("ENGINE_UPDATE_THREAD static is invalid");
        }
    }
}

pub fn init() {
    unsafe {
        let ptr: *const once_cell::sync::Lazy<UpdateThreadWrapper> =
            &raw const ENGINE_UPDATE_THREAD;
        if let Some(lazy) = ptr.as_ref() {
            let _ = Lazy::force(lazy);
        }
    }
}
