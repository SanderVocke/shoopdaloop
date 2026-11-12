use super::*;

#[derive(Default)]
pub(super) struct Conversions {
    #[cfg(not(target_arch = "wasm32"))]
    pending: BTreeMap<ScriptId, native::Pending>,
}
impl Conversions {
    pub fn start(
        &mut self,
        manager: &mut ScriptManager,
        id: ScriptId,
        kind: ScriptKind,
        epoch: u64,
    ) -> anyhow::Result<()> {
        if kind == ScriptKind::Session {
            let (source, path, bundle, generation) = manager.conversion_source(id)?;
            if let Some(bundle) = bundle {
                return manager.commit_session_bundle(id, &source, generation, bundle);
            }
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(path) = path {
                anyhow::ensure!(
                    self.pending.len() < 8 && !self.pending.contains_key(&id),
                    "script conversion is already pending or busy"
                );
                self.pending
                    .insert(id, native::start(source, path, generation, epoch)?);
                return Ok(());
            }
            #[cfg(target_arch = "wasm32")]
            let _ = (path, epoch);
        }
        manager.convert_kind(id, kind)
    }
    pub fn cancel(&mut self, id: ScriptId) {
        #[cfg(not(target_arch = "wasm32"))]
        self.pending.remove(&id);
        #[cfg(target_arch = "wasm32")]
        let _ = id;
    }
    pub fn clear(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        self.pending.clear();
    }
    pub fn poll(&mut self, manager: &mut ScriptManager, epoch: u64) -> Vec<String> {
        let mut errors = Vec::new();
        #[cfg(not(target_arch = "wasm32"))]
        {
            let finished: Vec<_> = self
                .pending
                .iter()
                .filter_map(|(id, job)| {
                    if job.epoch != epoch {
                        return Some((*id, Err("script project changed during conversion".into())));
                    }
                    match job.result.try_recv() {
                        Ok(result) => Some((*id, result)),
                        Err(std::sync::mpsc::TryRecvError::Empty) => None,
                        Err(_) => Some((*id, Err("script resource worker stopped".into()))),
                    }
                })
                .collect();
            for (id, result) in finished {
                let job = self.pending.remove(&id).unwrap();
                if let Err(error) = result.map_err(anyhow::Error::msg).and_then(|bundle| {
                    manager.commit_session_bundle(id, &job.source, job.generation, bundle)
                }) {
                    errors.push(format!("Could not include script in session: {error}"));
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        let _ = (manager, epoch, &mut errors);
        errors
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use shoop_script_resources::ScriptResourceBundle;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Mutex, OnceLock,
    };
    type BundleResult = Result<Arc<ScriptResourceBundle>, String>;
    pub struct Pending {
        pub source: String,
        pub generation: u64,
        pub epoch: u64,
        pub result: mpsc::Receiver<BundleResult>,
        cancelled: Arc<AtomicBool>,
    }
    impl Drop for Pending {
        fn drop(&mut self) {
            self.cancelled.store(true, Ordering::Release);
        }
    }
    struct Work {
        source: String,
        path: String,
        cancelled: Arc<AtomicBool>,
        result: mpsc::SyncSender<BundleResult>,
    }
    pub fn start(
        source: String,
        path: String,
        generation: u64,
        epoch: u64,
    ) -> anyhow::Result<Pending> {
        static POOL: OnceLock<Mutex<Option<mpsc::SyncSender<Work>>>> = OnceLock::new();
        let mut pool = POOL
            .get_or_init(|| Mutex::new(None))
            .lock()
            .map_err(|_| anyhow::anyhow!("script resource pool failed"))?;
        if pool.is_none() {
            let (sender, receiver) = mpsc::sync_channel::<Work>(8);
            let receiver = Arc::new(Mutex::new(receiver));
            for index in 0..2 {
                let receiver = Arc::clone(&receiver);
                std::thread::Builder::new()
                    .name(format!("script-resources-{index}"))
                    .spawn(move || loop {
                        let Ok(job) = receiver
                            .lock()
                            .unwrap_or_else(|error| error.into_inner())
                            .recv()
                        else {
                            break;
                        };
                        if job.cancelled.load(Ordering::Acquire) {
                            continue;
                        }
                        let result = shoop_script_resources::capture_filesystem_bundle(
                            std::path::Path::new(&job.path),
                            Arc::<[u8]>::from(job.source.as_bytes()),
                            shoop_script_resources::ResourceLimits::default(),
                        )
                        .map(Arc::new)
                        .map_err(|error| error.to_string());
                        if !job.cancelled.load(Ordering::Acquire) {
                            let _ = job.result.send(result);
                        }
                    })?;
            }
            *pool = Some(sender);
        }
        let (sender, result) = mpsc::sync_channel(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        pool.as_ref()
            .unwrap()
            .try_send(Work {
                source: source.clone(),
                path,
                cancelled: Arc::clone(&cancelled),
                result: sender,
            })
            .map_err(|_| {
                anyhow::anyhow!(
                    "script resource workers are busy; retry after pending work finishes"
                )
            })?;
        Ok(Pending {
            source,
            generation,
            epoch,
            result,
            cancelled,
        })
    }
}
