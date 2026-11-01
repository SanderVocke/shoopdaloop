//! Local composition. The service owns musical state; the client owns UI and Lua state.
use anyhow::{anyhow, Result};
use shoop_app::{ConnectionId, Service, ServiceOptions};
use shoop_app_client::{
    runtime::{Runtime as ClientRuntime, Startup},
    Client,
};
use shoop_app_protocol::{local, ClientId, HostId, SessionId};
use shoop_backend::Backend;
use shoop_egui::{AppIntent, AppSnapshot};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

pub use shoop_app_client::runtime::{
    ApplicationAudioPreview, ApplicationFileOutput, StartupScript,
};

#[derive(Clone, Debug)]
pub struct ApplicationStartOptions {
    pub startup_scripts: Vec<StartupScript>,
    pub master_auto_connect: bool,
    pub loop_smoothing_ms: u32,
    pub startup_warnings: Vec<String>,
}
impl Default for ApplicationStartOptions {
    fn default() -> Self {
        Self {
            startup_scripts: Vec::new(),
            master_auto_connect: true,
            loop_smoothing_ms: 3,
            startup_warnings: Vec::new(),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DispatchError {
    Full,
    Disconnected,
}
impl std::fmt::Display for DispatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Full => "application queue is full",
            Self::Disconnected => "session client is disconnected",
        })
    }
}
impl std::error::Error for DispatchError {}
impl From<shoop_app_client::Error> for DispatchError {
    fn from(error: shoop_app_client::Error) -> Self {
        match error {
            shoop_app_client::Error::Full => Self::Full,
            _ => Self::Disconnected,
        }
    }
}
static NEXT_LOCAL_ID: AtomicU64 = AtomicU64::new(1);
type FileOutputs = Arc<Mutex<VecDeque<ApplicationFileOutput>>>;

pub struct CooperativeApplicationRuntime {
    service: Service,
    port: local::ServicePort,
    client: ClientRuntime,
    outputs: FileOutputs,
    initialized: bool,
}
impl CooperativeApplicationRuntime {
    #[cfg(test)]
    pub fn start(backend: Box<dyn Backend>) -> Result<Self> {
        Self::start_with_options(backend, ApplicationStartOptions::default())
    }
    #[cfg(test)]
    pub fn start_with_options(
        backend: Box<dyn Backend>,
        options: ApplicationStartOptions,
    ) -> Result<Self> {
        Self::start_with_options_and_midi(
            backend,
            options,
            Box::new(shoop_scripting::NullMidiService),
        )
    }
    pub fn start_with_options_and_midi(
        backend: Box<dyn Backend>,
        options: ApplicationStartOptions,
        midi: Box<dyn shoop_scripting::MidiControlService>,
    ) -> Result<Self> {
        let host = HostId(u128::from(NEXT_LOCAL_ID.fetch_add(1, Ordering::Relaxed)));
        let session = SessionId(u128::from(NEXT_LOCAL_ID.fetch_add(1, Ordering::Relaxed)));
        let actor = ClientId(u128::from(NEXT_LOCAL_ID.fetch_add(1, Ordering::Relaxed)));
        let service = Service::new(
            backend,
            ServiceOptions {
                host,
                session,
                master_auto_connect: options.master_auto_connect,
                loop_smoothing_ms: options.loop_smoothing_ms,
            },
        )?;
        let (client_port, port) = local::channel();
        let client = Client::new(actor, host, session).map_err(|error| anyhow!("{error:?}"))?;
        let mut client = ClientRuntime::new(
            client,
            Box::new(client_port),
            midi,
            Startup::Fresh(options.startup_scripts),
        )
        .map_err(|error| anyhow!("{error:?}"))?;
        for warning in options.startup_warnings {
            client.notify(warning);
        }
        Ok(Self {
            service,
            port,
            client,
            outputs: Arc::new(Mutex::new(VecDeque::new())),
            initialized: false,
        })
    }
    pub fn dispatch(&mut self, intent: AppIntent) -> Result<(), DispatchError> {
        self.client.dispatch(intent).map_err(Into::into)
    }
    pub fn snapshot(&self) -> Arc<AppSnapshot> {
        self.client.snapshot()
    }
    pub fn tick(&mut self, elapsed: Duration) {
        if self.service.is_ready() {
            self.initialized = true;
        }
        if self.initialized {
            self.service
                .pump_local(ConnectionId(1), &self.port, elapsed);
        } else {
            self.service.tick(elapsed);
        }
        if let Some(message) = self.service.failure() {
            self.client.fail(message.to_owned());
        }
        self.client.tick(elapsed);
        let mut outputs = self
            .outputs
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        while outputs.len() < 8 {
            let Some(output) = self.client.take_file_output() else {
                break;
            };
            outputs.push_back(output);
        }
    }
    #[cfg(any(target_arch = "wasm32", test))]
    pub fn take_file_output(&self) -> Option<ApplicationFileOutput> {
        self.outputs
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .pop_front()
    }
    #[cfg(any(target_arch = "wasm32", test))]
    pub fn take_audio_preview(&self) -> Option<ApplicationAudioPreview> {
        self.client.take_audio_preview()
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use shoop_app_protocol::QUEUE_CAPACITY;
    use shoop_egui::ScriptId;
    use std::{
        sync::{
            mpsc::{self, SyncSender},
            RwLock,
        },
        thread::{self, JoinHandle},
        time::Instant,
    };
    enum Message {
        Intent(AppIntent),
        Detach,
    }
    #[derive(Clone)]
    pub struct ApplicationHandle {
        sender: SyncSender<Message>,
        snapshot: Arc<RwLock<Arc<AppSnapshot>>>,
        outputs: FileOutputs,
    }
    impl ApplicationHandle {
        pub fn dispatch(&self, intent: AppIntent) -> Result<(), DispatchError> {
            self.sender
                .try_send(Message::Intent(intent))
                .map_err(|error| match error {
                    mpsc::TrySendError::Full(_) => DispatchError::Full,
                    mpsc::TrySendError::Disconnected(_) => DispatchError::Disconnected,
                })
        }
        pub fn snapshot(&self) -> Arc<AppSnapshot> {
            self.snapshot
                .read()
                .unwrap_or_else(|error| error.into_inner())
                .clone()
        }
        pub fn take_file_output(&self) -> Option<ApplicationFileOutput> {
            self.outputs
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .pop_front()
        }
        pub fn take_audio_preview(&self) -> Option<ApplicationAudioPreview> {
            None
        }
    }
    pub struct ApplicationRuntime {
        handle: ApplicationHandle,
        join: Option<JoinHandle<()>>,
        startup_ids: Vec<Option<ScriptId>>,
    }
    impl ApplicationRuntime {
        #[cfg(test)]
        pub fn start(backend: Box<dyn Backend + Send>) -> Result<Self> {
            Self::start_with_options(backend, ApplicationStartOptions::default())
        }
        pub fn start_with_options(
            backend: Box<dyn Backend + Send>,
            options: ApplicationStartOptions,
        ) -> Result<Self> {
            let (sender, receiver) = mpsc::sync_channel(QUEUE_CAPACITY);
            let (ready_tx, ready_rx) = mpsc::sync_channel(1);
            let snapshot = Arc::new(RwLock::new(Arc::new(AppSnapshot::default())));
            let outputs: FileOutputs = Arc::new(Mutex::new(VecDeque::new()));
            let handle = ApplicationHandle {
                sender,
                snapshot: Arc::clone(&snapshot),
                outputs: Arc::clone(&outputs),
            };
            let join = thread::Builder::new()
                .name("shoop-application".into())
                .spawn(move || {
                    let result = CooperativeApplicationRuntime::start_with_options_and_midi(
                        backend,
                        options,
                        Box::new(shoop_scripting::NativeMidiService::new()),
                    );
                    let mut runtime = match result {
                        Ok(runtime) => runtime,
                        Err(error) => {
                            let _ = ready_tx.send(Err(error.to_string()));
                            return;
                        }
                    };
                    runtime.outputs = outputs;
                    let mut ready_tx = Some(ready_tx);
                    let mut last = Instant::now();
                    loop {
                        for _ in 0..shoop_app_protocol::MAX_MESSAGES_PER_TURN {
                            match receiver.try_recv() {
                                Ok(Message::Intent(intent)) => {
                                    if let Err(error) = runtime.dispatch(intent) {
                                        runtime.client.notify(error.to_string());
                                    }
                                }
                                Ok(Message::Detach) | Err(mpsc::TryRecvError::Disconnected) => {
                                    let _ = runtime.client.detach();
                                    return;
                                }
                                Err(mpsc::TryRecvError::Empty) => break,
                            }
                        }
                        let now = Instant::now();
                        runtime.tick(now.duration_since(last));
                        last = now;
                        *snapshot.write().unwrap_or_else(|error| error.into_inner()) =
                            runtime.snapshot();
                        if runtime.client.state() == shoop_app_client::State::Lost {
                            if let Some(ready_tx) = ready_tx.take() {
                                let _ = ready_tx.send(Err(
                                    "session initialization failed; see application diagnostics"
                                        .to_owned(),
                                ));
                            }
                            return;
                        }
                        if runtime.client.state() == shoop_app_client::State::Ready {
                            if let Some(ready_tx) = ready_tx.take() {
                                if ready_tx
                                    .send(Ok(runtime.client.startup_script_ids().to_vec()))
                                    .is_err()
                                {
                                    return;
                                }
                            }
                        }
                        thread::sleep(Duration::from_millis(4));
                    }
                })?;
            match ready_rx.recv_timeout(Duration::from_secs(10)) {
                Ok(Ok(startup_ids)) => Ok(Self {
                    handle,
                    join: Some(join),
                    startup_ids,
                }),
                Ok(Err(message)) => {
                    let _ = join.join();
                    Err(anyhow!(message))
                }
                Err(error) => {
                    let _ = handle.sender.try_send(Message::Detach);
                    Err(anyhow!("session client startup failed: {error}"))
                }
            }
        }
        pub fn handle(&self) -> ApplicationHandle {
            self.handle.clone()
        }
        pub fn startup_script_ids(&self) -> &[Option<ScriptId>] {
            &self.startup_ids
        }
        pub fn shutdown_with_idle(&mut self, mut idle: impl FnMut()) {
            let Some(join) = self.join.take() else {
                return;
            };
            let _ = self.handle.sender.send(Message::Detach);
            while !join.is_finished() {
                idle();
                thread::sleep(Duration::from_millis(1));
            }
            let _ = join.join();
        }
    }
    impl Drop for ApplicationRuntime {
        fn drop(&mut self) {
            self.shutdown_with_idle(|| {});
        }
    }
}
#[cfg(not(target_arch = "wasm32"))]
pub use native::{ApplicationHandle, ApplicationRuntime};
