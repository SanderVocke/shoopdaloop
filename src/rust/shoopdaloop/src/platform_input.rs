use shoop_app_api::{IoTaskState, IoTaskStatus, TaskId};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Weak,
};

#[derive(Default)]
pub struct OutputSlot {
    active: Weak<()>,
}
impl OutputSlot {
    pub fn reserve(&mut self) -> anyhow::Result<Arc<()>> {
        anyhow::ensure!(
            self.active.upgrade().is_none(),
            "A file save is still finishing; retry when it completes"
        );
        let lease = Arc::new(());
        self.active = Arc::downgrade(&lease);
        Ok(lease)
    }
}

#[derive(Default)]
pub struct InputSlot {
    active: Option<Weak<State>>,
}
struct State {
    task: TaskId,
    cancelled: AtomicBool,
    #[cfg(target_arch = "wasm32")]
    abort: std::sync::Mutex<Option<futures_util::future::AbortHandle>>,
}
pub struct InputLease(Arc<State>);
impl InputSlot {
    pub fn reserve(&mut self, task: TaskId) -> anyhow::Result<InputLease> {
        anyhow::ensure!(
            self.active.as_ref().and_then(Weak::upgrade).is_none(),
            "The previous file input is still closing; retry when it finishes"
        );
        let state = Arc::new(State {
            task,
            cancelled: AtomicBool::new(false),
            #[cfg(target_arch = "wasm32")]
            abort: std::sync::Mutex::new(None),
        });
        self.active = Some(Arc::downgrade(&state));
        Ok(InputLease(state))
    }
    pub fn reconcile(&self, task: Option<&IoTaskState>) {
        if let Some(state) = self.active.as_ref().and_then(Weak::upgrade) {
            if task.is_none_or(|task| task.id != state.task || task.status != IoTaskStatus::Running)
            {
                state.cancelled.store(true, Ordering::Release);
                #[cfg(target_arch = "wasm32")]
                if let Some(abort) = state
                    .abort
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .as_ref()
                {
                    abort.abort();
                }
            }
        }
    }
}
impl Drop for InputSlot {
    fn drop(&mut self) {
        self.reconcile(None);
    }
}
impl InputLease {
    #[cfg(target_arch = "wasm32")]
    pub fn registration(&self) -> futures_util::future::AbortRegistration {
        let (handle, registration) = futures_util::future::AbortHandle::new_pair();
        if self.check().is_err() {
            handle.abort();
        }
        *self.0.abort.lock().unwrap_or_else(|e| e.into_inner()) = Some(handle);
        registration
    }
    pub fn check(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.0.cancelled.load(Ordering::Acquire),
            "file input cancelled"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[shoop_wasm_test_support::shoop_test]
    fn output_handoff_keeps_its_slot_until_the_platform_writer_finishes() {
        let mut slot = OutputSlot::default();
        let lease = slot.reserve().unwrap();
        assert!(slot.reserve().is_err());
        drop(lease);
        assert!(slot.reserve().is_ok());
    }
    #[shoop_wasm_test_support::shoop_test]
    fn cancellation_keeps_the_slot_reserved_until_the_reader_releases_it() {
        let mut slot = InputSlot::default();
        let lease = slot.reserve(TaskId::from_raw(1)).unwrap();
        assert!(slot.reserve(TaskId::from_raw(2)).is_err());
        lease.check().unwrap();
        slot.reconcile(None);
        assert!(lease.check().is_err());
        assert!(slot.reserve(TaskId::from_raw(2)).is_err());
        drop(lease);
        let next = slot.reserve(TaskId::from_raw(2)).unwrap();
        next.check().unwrap();
    }

    #[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
    #[shoop_wasm_test_support::shoop_test]
    async fn browser_url_input_completes_at_stream_eof_and_cancellation_wakes_the_reader() {
        let mut slot = InputSlot::default();
        let task_id = TaskId::from_raw(9);
        let lease = slot.reserve(task_id).unwrap();
        let input = crate::read_browser_session_url(
            "data:application/octet-stream;base64,aGVsbG8=",
            crate::SessionDownload {
                task_id,
                name: "input.shoop".into(),
                bytes: Vec::new(),
            },
            &lease,
        )
        .await
        .unwrap();
        assert!(
            matches!(input, shoop_app_api::AppIntent::CompleteIoInput { task_id: id, bytes, .. } if id == task_id && &**bytes == b"hello")
        );
        let registration = lease.registration();
        slot.reconcile(None);
        assert!(futures_util::future::Abortable::new(
            futures_util::future::pending::<()>(),
            registration
        )
        .await
        .is_err());
        assert!(lease.check().is_err());
    }
}
