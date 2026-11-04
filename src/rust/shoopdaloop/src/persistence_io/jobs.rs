use super::*;
#[cfg(target_arch = "wasm32")]
mod browser;
#[cfg(target_arch = "wasm32")]
pub(super) use browser::{CodecExecutor, CodecJob};

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Mutex, OnceLock,
    };
    pub(crate) struct CodecJob {
        receiver: mpsc::Receiver<Result<Value, String>>,
        cancelled: Arc<AtomicBool>,
    }
    pub(crate) struct CodecExecutor {
        current: Option<Arc<AtomicBool>>,
    }
    struct WorkItem {
        work: Work,
        sender: mpsc::SyncSender<Result<Value, String>>,
        cancelled: Arc<AtomicBool>,
        span: tracing::Span,
    }
    fn sender() -> Result<mpsc::SyncSender<WorkItem>> {
        static POOL: OnceLock<Mutex<Option<mpsc::SyncSender<WorkItem>>>> = OnceLock::new();
        let mut pool = POOL
            .get_or_init(|| Mutex::new(None))
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(sender) = pool.as_ref() {
            return Ok(sender.clone());
        }
        let (sender, receiver) = mpsc::sync_channel::<WorkItem>(8);
        let receiver = Arc::new(Mutex::new(receiver));
        for index in 0..2 {
            let receiver = Arc::clone(&receiver);
            std::thread::Builder::new()
                .name(format!("session-codec-{index}"))
                .spawn(move || loop {
                    let item = { receiver.lock().unwrap_or_else(|e| e.into_inner()).recv() };
                    let Ok(WorkItem {
                        work,
                        sender,
                        cancelled,
                        span,
                    }) = item
                    else {
                        break;
                    };
                    let _span = span.entered();
                    let result = if cancelled.load(Ordering::Acquire) {
                        Err("file task cancelled".into())
                    } else {
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            work.run_with_cancel(|| cancelled.load(Ordering::Acquire))
                        }))
                        .map_err(|_| "file codec failed unexpectedly".to_owned())
                        .and_then(|result| result.map_err(|e| e.to_string()))
                    };
                    let result = if cancelled.load(Ordering::Acquire) {
                        Err("file task cancelled".into())
                    } else {
                        result
                    };
                    let _ = sender.send(result);
                })?;
        }
        *pool = Some(sender.clone());
        Ok(sender)
    }
    impl CodecExecutor {
        pub fn new() -> Self {
            Self { current: None }
        }
        pub fn reset(&mut self) {
            if let Some(flag) = self.current.take() {
                flag.store(true, Ordering::Release);
            }
        }
        pub fn start(&mut self, work: Work) -> Result<CodecJob> {
            let (tx, rx) = mpsc::sync_channel(1);
            let cancelled = Arc::new(AtomicBool::new(false));
            sender()?
                .try_send(WorkItem {
                    work,
                    sender: tx,
                    cancelled: Arc::clone(&cancelled),
                    span: tracing::debug_span!("frontend.persistence.native_codec"),
                })
                .map_err(|error| match error {
                    mpsc::TrySendError::Full(_) => {
                        anyhow!("File processing is busy; retry when pending tasks finish")
                    }
                    mpsc::TrySendError::Disconnected(_) => {
                        anyhow!("File processing stopped; restart to retry")
                    }
                })?;
            self.current = Some(Arc::clone(&cancelled));
            Ok(CodecJob {
                receiver: rx,
                cancelled,
            })
        }
    }
    impl Drop for CodecExecutor {
        fn drop(&mut self) {
            self.reset();
        }
    }
    impl Drop for CodecJob {
        fn drop(&mut self) {
            self.cancelled.store(true, Ordering::Release);
        }
    }
    impl CodecJob {
        pub fn poll(&mut self) -> Option<Result<Value, String>> {
            match self.receiver.try_recv() {
                Ok(value) => Some(value),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(_) => Some(Err("file codec worker stopped without a result".into())),
            }
        }
    }
}
#[cfg(not(target_arch = "wasm32"))]
pub(super) use native::{CodecExecutor, CodecJob};
