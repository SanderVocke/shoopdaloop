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
    ApplicationAudioPreview, ApplicationFileOutput, ApplicationFileRequest,
    ApplicationFileRequestKind, StartupScript,
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
#[derive(Default)]
struct FileMailbox {
    outputs: VecDeque<ApplicationFileOutput>,
    requests: VecDeque<ApplicationFileRequest>,
}
impl FileMailbox {
    fn collect(&mut self, runtime: &mut ClientRuntime) {
        while self.outputs.len() < 8 {
            let Some(output) = runtime.take_file_output() else {
                break;
            };
            self.outputs.push_back(output);
        }
        if self.requests.is_empty() {
            if let Some(request) = runtime.take_file_request() {
                self.requests.push_back(request);
            }
        }
    }
}
type FileOutputs = Arc<Mutex<FileMailbox>>;

pub struct CooperativeApplicationRuntime {
    service: Service,
    port: local::ServicePort,
    client: ClientRuntime,
    outputs: FileOutputs,
    initialized: bool,
}
impl CooperativeApplicationRuntime {
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
        .map_err(|error| anyhow!("{error:?}"))?
        .with_persistence_io(Box::new(crate::persistence_io::Provider::new()));
        for warning in options.startup_warnings {
            client.notify(warning);
        }
        Ok(Self {
            service,
            port,
            client,
            outputs: Arc::new(Mutex::new(FileMailbox::default())),
            initialized: false,
        })
    }
    #[cfg(target_arch = "wasm32")]
    pub fn is_ready(&self) -> bool {
        self.client.state() == shoop_app_client::State::Ready
    }
    #[cfg(target_arch = "wasm32")]
    pub fn dispatch(&mut self, intent: AppIntent) -> Result<(), DispatchError> {
        self.client.dispatch(intent).map_err(Into::into)
    }
    #[cfg(target_arch = "wasm32")]
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
        outputs.collect(&mut self.client);
    }
    #[cfg(target_arch = "wasm32")]
    pub fn take_file_output(&self) -> Option<ApplicationFileOutput> {
        self.outputs
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .outputs
            .pop_front()
    }
    #[cfg(target_arch = "wasm32")]
    pub fn take_file_request(&self) -> Option<ApplicationFileRequest> {
        self.outputs
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .requests
            .pop_front()
    }
    #[cfg(target_arch = "wasm32")]
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
    struct RemoteDelivery(shoop_host::connection::Connection);
    impl shoop_app_client::runtime::Delivery for RemoteDelivery {
        fn send(
            &mut self,
            message: shoop_app_protocol::ClientMessage,
        ) -> Result<(), shoop_app_client::Error> {
            self.0.send(message)
        }
        fn poll(&mut self, client: &mut Client) -> Result<usize, shoop_app_client::Error> {
            self.0.poll(client)
        }
    }
    enum SessionRuntime {
        Local(CooperativeApplicationRuntime),
        Remote(ClientRuntime, FileOutputs),
    }
    impl SessionRuntime {
        fn client(&self) -> &ClientRuntime {
            match self {
                Self::Local(runtime) => &runtime.client,
                Self::Remote(runtime, _) => runtime,
            }
        }
        fn client_mut(&mut self) -> &mut ClientRuntime {
            match self {
                Self::Local(runtime) => &mut runtime.client,
                Self::Remote(runtime, _) => runtime,
            }
        }
        fn set_outputs(&mut self, outputs: FileOutputs) {
            match self {
                Self::Local(runtime) => runtime.outputs = outputs,
                Self::Remote(_, current) => *current = outputs,
            }
        }
        fn dispatch(&mut self, intent: AppIntent) -> Result<(), DispatchError> {
            self.client_mut().dispatch(intent).map_err(Into::into)
        }
        fn snapshot(&self) -> Arc<AppSnapshot> {
            self.client().snapshot()
        }
        fn tick(&mut self, elapsed: Duration) {
            match self {
                Self::Local(runtime) => runtime.tick(elapsed),
                Self::Remote(runtime, outputs) => {
                    runtime.tick(elapsed);
                    let mut outputs = outputs.lock().unwrap_or_else(|error| error.into_inner());
                    outputs.collect(runtime);
                }
            }
        }
    }
    enum Message {
        Intent(AppIntent),
        PrepareShutdown(mpsc::Sender<Result<shoop_app_protocol::ClientMessage, String>>),
        ResumeControls,
        Detach,
    }
    #[derive(Clone, Debug)]
    pub struct ConnectionStatus {
        pub state: shoop_app_client::State,
        pub error: Option<String>,
    }
    #[derive(Clone)]
    pub struct ApplicationHandle {
        sender: SyncSender<Message>,
        snapshot: Arc<RwLock<Arc<AppSnapshot>>>,
        outputs: FileOutputs,
        status: Arc<RwLock<ConnectionStatus>>,
    }
    impl ApplicationHandle {
        pub fn dispatch(&self, intent: AppIntent) -> Result<(), DispatchError> {
            if self.status().state != shoop_app_client::State::Ready
                && !matches!(
                    intent,
                    AppIntent::CompleteIoOutput { .. } | AppIntent::FailIoTask { .. }
                )
            {
                return Err(DispatchError::Disconnected);
            }
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
        pub fn status(&self) -> ConnectionStatus {
            self.status
                .read()
                .unwrap_or_else(|error| error.into_inner())
                .clone()
        }
        pub fn prepare_shutdown(&self) -> Result<shoop_app_protocol::ClientMessage> {
            let (sender, receiver) = mpsc::channel();
            self.sender
                .try_send(Message::PrepareShutdown(sender))
                .map_err(|_| anyhow!("session client is busy or unavailable; retry shutdown"))?;
            receiver
                .recv_timeout(Duration::from_secs(3))
                .map_err(|_| anyhow!("session client did not reserve shutdown; retry"))?
                .map_err(anyhow::Error::msg)
        }
        pub fn resume_controls(&self) {
            let _ = self.sender.try_send(Message::ResumeControls);
        }
        pub fn take_file_output(&self) -> Option<ApplicationFileOutput> {
            self.outputs
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .outputs
                .pop_front()
        }
        pub fn take_file_request(&self) -> Option<ApplicationFileRequest> {
            self.outputs
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .requests
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
        pub fn start_with_options(
            backend: Box<dyn Backend + Send>,
            options: ApplicationStartOptions,
        ) -> Result<Self> {
            Self::start_thread(true, move || {
                CooperativeApplicationRuntime::start_with_options_and_midi(
                    backend,
                    options,
                    Box::new(shoop_scripting::NativeMidiService::new()),
                )
                .map(SessionRuntime::Local)
            })
        }
        pub fn attach(
            entry: shoop_host::registry::HostEntry,
            options: ApplicationStartOptions,
        ) -> Result<Self> {
            Self::start_thread(false, move || {
                let connection = shoop_host::connection::connect(&entry, Duration::from_secs(3))?;
                let client = Client::new(
                    ClientId(shoop_host::registry::identity()?),
                    entry.host,
                    entry.session,
                )
                .map_err(|error| anyhow!("{error:?}"))?;
                let mut runtime = ClientRuntime::new(
                    client,
                    Box::new(RemoteDelivery(connection)),
                    Box::new(shoop_scripting::NativeMidiService::new()),
                    Startup::IfFresh(options.startup_scripts),
                )
                .map_err(|error| anyhow!("{error:?}"))?
                .with_persistence_io(Box::new(crate::persistence_io::Provider::new()));
                for warning in entry.warnings {
                    runtime.notify(warning);
                }
                for warning in options.startup_warnings {
                    runtime.notify(warning);
                }
                Ok(SessionRuntime::Remote(
                    runtime,
                    Arc::new(Mutex::new(FileMailbox::default())),
                ))
            })
        }
        fn start_thread(
            wait_ready: bool,
            make: impl FnOnce() -> Result<SessionRuntime> + Send + 'static,
        ) -> Result<Self> {
            let (sender, receiver) = mpsc::sync_channel(QUEUE_CAPACITY);
            let (ready_tx, ready_rx) = mpsc::sync_channel(1);
            let snapshot = Arc::new(RwLock::new(Arc::new(AppSnapshot::default())));
            let outputs: FileOutputs = Arc::new(Mutex::new(FileMailbox::default()));
            let status = Arc::new(RwLock::new(ConnectionStatus {
                state: shoop_app_client::State::Attaching,
                error: None,
            }));
            let handle = ApplicationHandle {
                sender,
                snapshot: Arc::clone(&snapshot),
                outputs: Arc::clone(&outputs),
                status: Arc::clone(&status),
            };
            let join = thread::Builder::new()
                .name("shoop-application".into())
                .spawn(move || {
                    let result = make();
                    let mut runtime = match result {
                        Ok(runtime) => runtime,
                        Err(error) => {
                            *status.write().unwrap_or_else(|error| error.into_inner()) =
                                ConnectionStatus {
                                    state: shoop_app_client::State::Lost,
                                    error: Some(error.to_string()),
                                };
                            let _ = ready_tx.send(Err(error.to_string()));
                            return;
                        }
                    };
                    runtime.set_outputs(outputs);
                    let mut ready_tx = Some(ready_tx);
                    let mut last = Instant::now();
                    let started = last;
                    let mut pending_intent = None;
                    loop {
                        for _ in 0..shoop_app_protocol::MAX_MESSAGES_PER_TURN {
                            let message = match pending_intent.take() {
                                Some(intent) => Ok(Message::Intent(intent)),
                                None => receiver.try_recv(),
                            };
                            match message {
                                Ok(Message::Intent(intent)) => {
                                    match runtime.dispatch(intent.clone()) {
                                        Ok(()) => {}
                                        Err(DispatchError::Full) => {
                                            pending_intent = Some(intent);
                                            break;
                                        }
                                        Err(error) => {
                                            runtime.client_mut().notify(error.to_string())
                                        }
                                    }
                                }
                                Ok(Message::PrepareShutdown(sender)) => {
                                    let result =
                                        runtime.client_mut().prepare_shutdown().map_err(|error| {
                                            format!("Cannot close this host: {error:?}")
                                        });
                                    let _ = sender.send(result);
                                }
                                Ok(Message::ResumeControls) => {
                                    runtime.client_mut().resume_controls()
                                }
                                Ok(Message::Detach) | Err(mpsc::TryRecvError::Disconnected) => {
                                    let _ = runtime.client_mut().detach();
                                    return;
                                }
                                Err(mpsc::TryRecvError::Empty) => break,
                            }
                        }
                        let now = Instant::now();
                        runtime.tick(now.duration_since(last));
                        last = now;
                        if !wait_ready
                            && started.elapsed() > Duration::from_secs(15)
                            && matches!(
                                runtime.client().state(),
                                shoop_app_client::State::Attaching
                                    | shoop_app_client::State::Synchronizing
                            )
                        {
                            runtime.client_mut().fail(
                                "Session attachment timed out. Reattach explicitly to retry."
                                    .into(),
                            );
                        }
                        *snapshot.write().unwrap_or_else(|error| error.into_inner()) =
                            runtime.snapshot();
                        status
                            .write()
                            .unwrap_or_else(|error| error.into_inner())
                            .state = runtime.client().state();
                        if let Some(reason) = runtime.client().client().refusal() {
                            status
                                .write()
                                .unwrap_or_else(|error| error.into_inner())
                                .error = Some(format!("Host refused attachment: {reason:?}"));
                        }
                        if runtime.client().state() == shoop_app_client::State::Lost {
                            if let Some(ready_tx) = ready_tx.take() {
                                let _ = ready_tx.send(Err(
                                    "session initialization failed; see application diagnostics"
                                        .to_owned(),
                                ));
                            }
                        }
                        if runtime.client().state() == shoop_app_client::State::Ready {
                            if let Some(ready_tx) = ready_tx.take() {
                                let _ = ready_tx
                                    .send(Ok(runtime.client().startup_script_ids().to_vec()));
                            }
                        }
                        thread::sleep(Duration::from_millis(4));
                    }
                })?;
            if !wait_ready {
                return Ok(Self {
                    handle,
                    join: Some(join),
                    startup_ids: Vec::new(),
                });
            }
            match ready_rx.recv_timeout(Duration::from_secs(10)) {
                Ok(Ok(startup_ids)) => Ok(Self {
                    handle,
                    join: Some(join),
                    startup_ids,
                }),
                Ok(Err(message)) => {
                    let _ = handle.sender.try_send(Message::Detach);
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
