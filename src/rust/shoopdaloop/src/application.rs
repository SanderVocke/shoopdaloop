//! Local composition. The service owns musical state; the client owns UI and Lua state.
use anyhow::{anyhow, Result};
use shoop_app::{ConnectionId, Service, ServiceOptions};
use shoop_app_client::{
    runtime::{Runtime as ClientRuntime, Startup},
    Client,
};
use shoop_app_protocol::local;
use shoop_backend::Backend;
use shoop_egui::{AppIntent, AppSnapshot};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
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
#[derive(Default)]
struct FileMailbox {
    preview: Option<ApplicationAudioPreview>,
    outputs: VecDeque<ApplicationFileOutput>,
    requests: VecDeque<ApplicationFileRequest>,
}
impl FileMailbox {
    fn collect(&mut self, runtime: &mut ClientRuntime) {
        if let Some(preview) = runtime.take_audio_preview() {
            self.preview = Some(preview);
        }
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
        let service = Service::new(
            backend,
            ServiceOptions {
                master_auto_connect: options.master_auto_connect,
                loop_smoothing_ms: options.loop_smoothing_ms,
            },
        )?;
        let (client_port, port) = local::channel();
        let client = Client::new();
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
        self.outputs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .preview
            .take()
    }
}

#[cfg(target_arch = "wasm32")]
pub struct BrowserApplicationRuntime {
    inner: std::rc::Rc<std::cell::RefCell<BrowserPump>>,
    window: web_sys::Window,
    interval: i32,
    _callback: wasm_bindgen::closure::Closure<dyn FnMut()>,
}
#[cfg(target_arch = "wasm32")]
struct BrowserPump {
    runtime: CooperativeApplicationRuntime,
    last: web_time::Instant,
}
#[cfg(target_arch = "wasm32")]
impl BrowserPump {
    fn tick(&mut self) {
        let now = web_time::Instant::now();
        let elapsed = now.saturating_duration_since(self.last);
        self.last = now;
        self.runtime.tick(elapsed);
    }
}
#[cfg(target_arch = "wasm32")]
impl BrowserApplicationRuntime {
    pub fn start_with_options_and_midi(
        backend: Box<dyn Backend>,
        options: ApplicationStartOptions,
        midi: Box<dyn shoop_scripting::MidiControlService>,
    ) -> Result<Self> {
        use wasm_bindgen::JsCast;
        let window = web_sys::window().ok_or_else(|| anyhow!("browser window is unavailable"))?;
        let inner = std::rc::Rc::new(std::cell::RefCell::new(BrowserPump {
            runtime: CooperativeApplicationRuntime::start_with_options_and_midi(
                backend, options, midi,
            )?,
            last: web_time::Instant::now(),
        }));
        let weak = std::rc::Rc::downgrade(&inner);
        let callback = wasm_bindgen::closure::Closure::<dyn FnMut()>::new(move || {
            if let Some(inner) = weak.upgrade() {
                inner.borrow_mut().tick();
            }
        });
        let interval = window
            .set_interval_with_callback_and_timeout_and_arguments_0(
                callback.as_ref().unchecked_ref(),
                4,
            )
            .map_err(|error| anyhow!("could not schedule the application pump: {error:?}"))?;
        Ok(Self {
            inner,
            window,
            interval,
            _callback: callback,
        })
    }
    pub fn wake(&self) -> std::rc::Rc<dyn Fn()> {
        let weak = std::rc::Rc::downgrade(&self.inner);
        std::rc::Rc::new(move || {
            if let Some(inner) = weak.upgrade() {
                if let Ok(mut pump) = inner.try_borrow_mut() {
                    pump.tick();
                }
            }
        })
    }
    pub fn is_ready(&self) -> bool {
        self.inner.borrow().runtime.is_ready()
    }
    pub fn has_pending_operations(&self) -> bool {
        let pump = self.inner.borrow();
        pump.runtime.client.has_pending_commands()
            || pump.runtime.client.client().has_unfinished_operations()
    }
    pub fn dispatch(&mut self, intent: AppIntent) -> Result<(), DispatchError> {
        self.inner.borrow_mut().runtime.dispatch(intent)
    }
    pub fn snapshot(&self) -> Arc<AppSnapshot> {
        self.inner.borrow().runtime.snapshot()
    }
    pub fn tick(&mut self, _elapsed: Duration) {
        self.inner.borrow_mut().tick();
    }
    pub fn take_file_output(&self) -> Option<ApplicationFileOutput> {
        self.inner.borrow().runtime.take_file_output()
    }
    pub fn take_file_request(&self) -> Option<ApplicationFileRequest> {
        self.inner.borrow().runtime.take_file_request()
    }
    pub fn take_audio_preview(&self) -> Option<ApplicationAudioPreview> {
        self.inner.borrow().runtime.take_audio_preview()
    }
}
#[cfg(target_arch = "wasm32")]
impl Drop for BrowserApplicationRuntime {
    fn drop(&mut self) {
        self.window.clear_interval_with_handle(self.interval);
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
        Reconnect(shoop_host::remote::Target, mpsc::Sender<Result<(), String>>),
        PrepareShutdown(mpsc::Sender<Result<shoop_app_protocol::ClientMessage, String>>),
        ResumeControls,
        Detach,
    }
    #[derive(Clone, Debug)]
    pub struct ConnectionStatus {
        pub state: shoop_app_client::State,
        pub error: Option<String>,
        queued_audio_intents: usize,
        audio_commands_pending: bool,
    }
    #[derive(Clone)]
    pub struct ApplicationHandle {
        sender: SyncSender<Message>,
        snapshot: Arc<RwLock<Arc<AppSnapshot>>>,
        outputs: FileOutputs,
        status: Arc<RwLock<ConnectionStatus>>,
    }
    impl ApplicationHandle {
        pub fn reconnect(&self, entry: shoop_host::remote::Target) -> Result<()> {
            let (sender, receiver) = mpsc::channel();
            self.sender
                .try_send(Message::Reconnect(entry, sender))
                .map_err(|_| anyhow!("session client is busy; retry reconnect"))?;
            receiver
                .recv_timeout(Duration::from_secs(8))
                .map_err(|_| anyhow!("session reconnect is still pending"))?
                .map_err(anyhow::Error::msg)
        }
        pub fn dispatch(&self, intent: AppIntent) -> Result<(), DispatchError> {
            let mut status = self
                .status
                .write()
                .unwrap_or_else(|error| error.into_inner());
            if status.state != shoop_app_client::State::Ready
                && !matches!(
                    intent,
                    AppIntent::CompleteIoOutput { .. } | AppIntent::FailIoTask { .. }
                )
            {
                return Err(DispatchError::Disconnected);
            }
            let audio = shoop_app_client::runtime::writes_audio_preferences(&intent);
            self.sender
                .try_send(Message::Intent(intent))
                .map_err(|error| match error {
                    mpsc::TrySendError::Full(_) => DispatchError::Full,
                    mpsc::TrySendError::Disconnected(_) => DispatchError::Disconnected,
                })?;
            if audio {
                status.queued_audio_intents += 1;
            }
            Ok(())
        }
        pub fn has_pending_audio_driver_commands(&self) -> bool {
            let status = self.status();
            status.queued_audio_intents != 0 || status.audio_commands_pending
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
            self.outputs
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .preview
                .take()
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
            graph: Option<Box<dyn shoop_app::HostGraphController>>,
            driver: Option<Box<dyn shoop_app::AudioDriverController>>,
        ) -> Result<Self> {
            Self::start_thread(true, move || {
                let mut runtime = CooperativeApplicationRuntime::start_with_options_and_midi(
                    backend,
                    options,
                    Box::new(shoop_scripting::NativeMidiService::new()),
                )?;
                if let Some(graph) = graph {
                    runtime.service.set_host_graph_controller(graph)?;
                }
                if let Some(driver) = driver {
                    runtime.service.set_audio_driver_controller(driver)?;
                }
                Ok(SessionRuntime::Local(runtime))
            })
        }
        pub fn attach(
            entry: impl Into<shoop_host::remote::Target>,
            options: ApplicationStartOptions,
        ) -> Result<Self> {
            let entry = entry.into();
            Self::start_thread(false, move || {
                let connection = shoop_host::connection::connect(&entry, Duration::from_secs(3))?;
                let client = Client::new();
                let mut runtime = ClientRuntime::new(
                    client,
                    Box::new(RemoteDelivery(connection)),
                    Box::new(shoop_scripting::NativeMidiService::new()),
                    Startup::Connected(options.startup_scripts),
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
                queued_audio_intents: 0,
                audio_commands_pending: false,
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
                                    queued_audio_intents: 0,
                                    audio_commands_pending: false,
                                };
                            let _ = ready_tx.send(Err(error.to_string()));
                            return;
                        }
                    };
                    runtime.set_outputs(outputs);
                    let mut ready_tx = Some(ready_tx);
                    let mut last = Instant::now();
                    let mut started = last;
                    let mut pending_intent = None;
                    loop {
                        let mut processed_audio_intents = 0;
                        for _ in 0..shoop_app_protocol::MAX_MESSAGES_PER_TURN {
                            let message = match pending_intent.take() {
                                Some(intent) => Ok(Message::Intent(intent)),
                                None => receiver.try_recv(),
                            };
                            match message {
                                Ok(Message::Reconnect(entry, sender)) => {
                                    let result = (|| -> Result<()> {
                                        anyhow::ensure!(matches!(runtime, SessionRuntime::Remote(..)), "local clients cannot reconnect");
                                        anyhow::ensure!(matches!(runtime.client().state(), shoop_app_client::State::Lost | shoop_app_client::State::Detached), "session client is still connected");
                                        let observed = shoop_host::connection::describe(&entry.management, Duration::from_secs(3))?;
                                        anyhow::ensure!(entry.instance != 0 && entry.instance == observed.instance, "Host instance changed; open a new connection and reload the project file. Previous client script data remains retained here.");
                                        let connection = shoop_host::connection::connect(&entry, Duration::from_secs(3))?;
                                        runtime.client_mut().reconnect(Box::new(RemoteDelivery(connection))).map_err(|error| anyhow!("could not reattach: {error:?}"))
                                    })();
                                    if result.is_ok() { started = Instant::now(); }
                                    let _ = sender.send(result.map_err(|error| error.to_string()));
                                }
                                Ok(Message::Intent(intent)) => {
                                    let audio = shoop_app_client::runtime::writes_audio_preferences(
                                        &intent,
                                    );
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
                                    processed_audio_intents += usize::from(audio);
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
                        {
                            let mut state =
                                status.write().unwrap_or_else(|error| error.into_inner());
                            state.state = runtime.client().state();
                            state.queued_audio_intents -= processed_audio_intents;
                            state.audio_commands_pending =
                                runtime.client().has_pending_audio_driver_commands();
                        }
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
pub use native::{ApplicationHandle, ApplicationRuntime, ConnectionStatus};

#[cfg(all(test, target_arch = "wasm32", feature = "wasm-test-browser"))]
mod browser_tests {
    use super::*;
    use shoop_rt_backend::{EngineClock, RtBackend};
    use shoop_rt_engine::{testing::OfflineRender, EngineDescription, HostPorts, Segment};

    struct Clock(OfflineRender);
    impl EngineClock for Clock {
        fn advance(&mut self, elapsed: Duration) {
            self.0.run_for(elapsed, |_, _| 0.).unwrap();
        }
        fn wait(&mut self) -> bool {
            self.0.run(16, |_, _| 0.).unwrap();
            true
        }
    }
    async fn pause() {
        let promise = js_sys::Promise::new(&mut |resolve, _| {
            web_sys::window()
                .unwrap()
                .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 4)
                .unwrap();
        });
        wasm_bindgen_futures::JsFuture::from(promise).await.unwrap();
    }
    #[shoop_wasm_test_support::shoop_test]
    async fn service_and_client_progress_without_ui_paints_and_release_their_pump() {
        let (render, client) = OfflineRender::new(&EngineDescription {
            quantum: 1,
            sample_rate: 1000,
            max_frames: 16,
            host: HostPorts::numbered(2, 2, 1),
            segments: vec![Segment {
                strips: 8,
                loops: 80,
            }],
            strip_channels: 2,
            resource_pages: 1024,
            page_bytes: 256,
            midi_events: 64,
            midi_bytes: 512,
            ..Default::default()
        })
        .unwrap();
        let backend = RtBackend::connect(client, Clock(render)).unwrap();
        let mut app = BrowserApplicationRuntime::start_with_options_and_midi(
            Box::new(backend),
            ApplicationStartOptions {
                master_auto_connect: false,
                loop_smoothing_ms: 0,
                ..Default::default()
            },
            Box::new(shoop_scripting::NullMidiService),
        )
        .unwrap();
        for _ in 0..250 {
            if app.is_ready() {
                break;
            }
            pause().await;
        }
        assert!(app.is_ready());
        app.dispatch(AppIntent::Global(shoop_egui::GlobalControlAction::SetSolo(
            true,
        )))
        .unwrap();
        for _ in 0..250 {
            if app.snapshot().global_controls.solo {
                break;
            }
            pause().await;
        }
        assert!(app.snapshot().global_controls.solo);
        let weak = std::rc::Rc::downgrade(&app.inner);
        let wake = app.wake();
        drop(app);
        assert!(weak.upgrade().is_none());
        wake();
        pause().await;
    }
}
