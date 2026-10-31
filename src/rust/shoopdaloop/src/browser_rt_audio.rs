//! The new engine backend in the browser: the main-thread half of its host. Enabling
//! browser audio opens the context, prepares the engine in an AudioWorklet from the
//! application's capacity settings and the ports known then, and starts the adapter over a
//! record carrier on the node's `MessagePort`. Web MIDI travels on the worklet's MIDI lanes.
//!
//! The engine's host ports are fixed when it is prepared: microphone channels are reserved
//! when audio starts without one, and Web MIDI ports allowed or connected later need a
//! reload.

use shoop_egui::NewEnginePools;
use shoop_rt_engine::{EngineDescription, Segment, MAX_HOST_PORTS};
use shoop_rt_host_worklet::main_thread::web_host_ports;
use shoop_rt_host_worklet::WorkletConfig;

/// The Web Audio render quantum.
const WORKLET_MAX_FRAMES: usize = 128;
/// Loop memory the browser prepares at most; larger settings are reduced with a warning.
const BROWSER_MAX_LOOP_MEMORY_MB: u32 = 256;
/// Capture channels prepared when audio starts without the microphone, so it can be
/// connected later without preparing the engine again.
#[cfg(any(target_arch = "wasm32", test))]
const RESERVED_CAPTURE_CHANNELS: usize = 2;

/// What the browser offers when the engine is prepared.
pub(crate) struct AudioSetup {
    pub sample_rate: u32,
    pub capture: usize,
    pub destination: usize,
    pub midi_inputs: Vec<String>,
    pub midi_outputs: Vec<String>,
}

/// The worklet's configuration for the capacity settings and a browser audio setup, with a
/// warning for every setting the browser cannot use as given.
pub(crate) fn worklet_config(
    pools: &NewEnginePools,
    setup: &AudioSetup,
) -> (WorkletConfig, Vec<String>) {
    let mut warnings = Vec::new();
    let loop_memory_mb = if pools.loop_memory_mb > BROWSER_MAX_LOOP_MEMORY_MB {
        warnings.push(format!(
            "The new engine backend reserves at most {BROWSER_MAX_LOOP_MEMORY_MB} MiB of loop memory in the browser instead of {} MiB",
            pools.loop_memory_mb
        ));
        BROWSER_MAX_LOOP_MEMORY_MB
    } else {
        pools.loop_memory_mb
    };
    for (ports, direction) in [
        (&setup.midi_inputs, "input"),
        (&setup.midi_outputs, "output"),
    ] {
        if ports.len() > MAX_HOST_PORTS {
            warnings.push(format!(
                "The new engine backend opens only the first {MAX_HOST_PORTS} of {} Web MIDI {direction}s",
                ports.len()
            ));
        }
    }
    let inputs = setup
        .midi_inputs
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let outputs = setup
        .midi_outputs
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let host = web_host_ports(setup.capture, setup.destination, &inputs, &outputs);
    let master_connections = match setup.destination {
        0 => Vec::new(),
        outputs => (0..2u8)
            .map(|channel| (channel, usize::from(channel).min(outputs - 1) as u16))
            .collect(),
    };
    let defaults = EngineDescription::default();
    let engine = EngineDescription {
        sample_rate: setup.sample_rate,
        max_frames: WORKLET_MAX_FRAMES,
        host,
        segments: vec![Segment {
            strips: pools.tracks,
            loops: pools.loops,
        }],
        strip_channels: u8::try_from(pools.channels).unwrap_or(u8::MAX),
        master_channels: 2,
        master_connections,
        // The loop edge smoothing setting's largest value, 100 ms.
        smoothing_limit_frames: setup.sample_rate / 10,
        resource_pages: (loop_memory_mb as usize * (1 << 20) / defaults.page_bytes).max(1),
        pending_transitions: defaults.pending_transitions.max(2 * pools.loops as usize),
        ..defaults
    };
    (WorkletConfig::new(engine), warnings)
}

#[cfg(target_arch = "wasm32")]
mod platform {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::rc::{Rc, Weak};
    use std::time::Duration;

    use anyhow::{anyhow, Result};
    use js_sys::{Array, Function, Object, Reflect, Uint8Array, WebAssembly};
    use shoop_backend::{Backend, BackendDriverState};
    use shoop_egui::{AudioDriverState, NewEnginePools};
    use shoop_rt_backend::{NoWait, RtBackend};
    use shoop_rt_host_worklet::lane::{self, MidiEvent};
    use shoop_rt_host_worklet::main_thread::{MidiClock, MidiInBatch, RecordBatches};
    use shoop_rt_protocol::carrier::{CarrierError, RecordClient, RecordPort};
    use shoop_rt_protocol::client::ProtocolClient;
    use shoop_rt_protocol::DisconnectReason;
    use shoop_scripting::MidiEndpointDirection;
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::{spawn_local, JsFuture};
    use web_sys::{
        AudioContext, AudioContextState, AudioWorkletNode, Event as WebEvent, HtmlButtonElement,
        MediaStream, MediaStreamAudioSourceNode, MediaStreamConstraints, MediaStreamTrack,
        MessageEvent, MessagePort, Response,
    };
    use web_time::Instant;

    use super::{worklet_config, AudioSetup, RESERVED_CAPTURE_CHANNELS};
    use crate::browser_midi::{BrowserMidiHub, BrowserMidiState, TrackForwarder};

    const PROCESSOR_NAME: &str = "shoop-rt-audio-processor";
    const PROCESSOR_SCRIPT_URL: &str = "./rt_audio_worklet.js";
    const MODULE_URL: &str = "./generated/shoop_rt_host_worklet.wasm";
    const EMBEDDED_ASSETS: &str = "shoopEmbeddedAudioWorklet";
    /// Most capture or destination channels used as host ports.
    const MAX_DEVICE_CHANNELS: u32 = 8;
    const STARTUP_TIMEOUT: Duration = Duration::from_secs(20);
    /// How long starting audio waits for a pending Web MIDI permission request.
    const MIDI_PERMISSION_WAIT: Duration = Duration::from_secs(15);
    /// Frames between a Web MIDI input event and the frame it is delivered at, so it reaches
    /// the worklet before that quantum.
    const MIDI_IN_LEAD_FRAMES: u64 = 2 * 128;

    pub type NewEngineBackend = RtBackend<RecordClient<SharedPort>, NoWait>;

    /// What the main thread posts to the processor, collected during one turn.
    struct Outgoing {
        records: RecordBatches,
        midi: MidiInBatch,
        target: MessagePort,
        flush_scheduled: bool,
    }

    /// The protocol client's record port over the processor's `MessagePort`. Records and
    /// MIDI posted during a main-thread turn leave together at its end.
    #[derive(Clone)]
    pub struct SharedPort(Rc<RefCell<Outgoing>>);

    impl SharedPort {
        fn new(target: MessagePort, midi_capacity: usize) -> Self {
            Self(Rc::new(RefCell::new(Outgoing {
                records: RecordBatches::new(),
                midi: MidiInBatch::new(midi_capacity),
                target,
                flush_scheduled: false,
            })))
        }

        fn receive(&self, batch: &[u8]) {
            self.0.borrow_mut().records.receive(batch);
        }

        fn close(&self, reason: DisconnectReason) {
            self.0.borrow_mut().records.close(reason);
        }

        fn push_midi(&self, event: MidiEvent<'_>) {
            if self.0.borrow_mut().midi.push(event) {
                self.schedule_flush();
            }
        }

        fn midi_dropped(&self) -> u64 {
            self.0.borrow().midi.dropped()
        }

        fn schedule_flush(&self) {
            {
                let mut outgoing = self.0.borrow_mut();
                if outgoing.flush_scheduled {
                    return;
                }
                outgoing.flush_scheduled = true;
            }
            let port = self.clone();
            spawn_local(async move { port.flush() });
        }

        fn flush(&self) {
            let (target, records, midi) = {
                let mut outgoing = self.0.borrow_mut();
                outgoing.flush_scheduled = false;
                (
                    outgoing.target.clone(),
                    outgoing.records.take_outgoing(),
                    outgoing.midi.take(),
                )
            };
            for (kind, bytes) in [("records", records), ("midi", midi)] {
                let Some(bytes) = bytes else {
                    continue;
                };
                if let Err(error) = post_bytes(&target, kind, &bytes) {
                    tracing::error!(error = ?error, kind, "frontend.new_engine.post_failed");
                    self.close(DisconnectReason::Fault);
                    return;
                }
            }
        }
    }

    impl RecordPort for SharedPort {
        fn post(&mut self, record: &[u8]) -> Result<(), CarrierError> {
            self.0.borrow_mut().records.post(record)?;
            self.schedule_flush();
            Ok(())
        }

        fn take(&mut self, buf: &mut [u8]) -> Option<usize> {
            self.0.borrow_mut().records.take(buf)
        }
    }

    fn post_bytes(target: &MessagePort, kind: &str, bytes: &[u8]) -> Result<(), JsValue> {
        let buffer = Uint8Array::from(bytes).buffer();
        let message = Object::new();
        Reflect::set(&message, &"kind".into(), &kind.into())?;
        Reflect::set(&message, &"bytes".into(), &buffer)?;
        target.post_message_with_transferable(&message, &Array::of1(&buffer))
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum InputMode {
        Microphone,
        OutputOnly,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Phase {
        AwaitingGesture,
        RequestingPermission,
        Starting,
        /// The backend belongs to the application.
        Running,
        Denied,
        Failed(String),
        /// Failed after the application started, or unavailable in this mode: only a
        /// reload can start the engine.
        Ended(String),
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Microphone {
        Off,
        Requesting,
        Connected,
        Denied,
        Ended,
        Failed,
    }

    struct Graph {
        context: AudioContext,
        node: Option<AudioWorkletNode>,
        stream: Option<MediaStream>,
        source: Option<MediaStreamAudioSourceNode>,
        port: Option<SharedPort>,
        message_handler: Option<Closure<dyn FnMut(MessageEvent)>>,
        processor_error_handler: Option<Closure<dyn FnMut(WebEvent)>>,
        state_handler: Option<Closure<dyn FnMut(WebEvent)>>,
        track_ended_handlers: Vec<Closure<dyn FnMut(WebEvent)>>,
    }

    struct Inner {
        phase: Phase,
        generation: u64,
        started: Option<Instant>,
        pools: NewEnginePools,
        hub: BrowserMidiHub,
        graph: Option<Graph>,
        backend: Option<NewEngineBackend>,
        warnings: Vec<String>,
        input_mode: Option<InputMode>,
        microphone: Microphone,
        /// Web MIDI endpoint identities of the engine's host MIDI inputs, by port index.
        midi_inputs: BTreeMap<String, u8>,
        /// Web MIDI endpoint identities of the engine's host MIDI outputs, by port index.
        midi_outputs: Vec<String>,
        midi_revision: Option<u64>,
        repaint: Option<eframe::egui::Context>,
    }

    impl Inner {
        /// Ends the current attempt. After the application started, only a reload can
        /// start the engine again.
        fn fail(&mut self, message: String) {
            tracing::error!(error = %message, "frontend.new_engine.failed");
            if let Some(port) = self.graph.as_ref().and_then(|graph| graph.port.as_ref()) {
                port.close(DisconnectReason::Fault);
            }
            self.phase = if matches!(self.phase, Phase::Running | Phase::Ended(_)) {
                Phase::Ended(message)
            } else {
                Phase::Failed(message)
            };
            self.end_attempt();
        }

        fn end_attempt(&mut self) {
            self.generation += 1;
            self.started = None;
            self.backend = None;
            shutdown_graph(self);
        }

        fn repaint(&self) {
            if let Some(context) = &self.repaint {
                context.request_repaint();
            }
        }
    }

    /// Owns the browser audio graph of the new engine and hands its backend to the
    /// application once connected.
    pub struct BrowserRtAudio {
        inner: Rc<RefCell<Inner>>,
        microphone_handler: Closure<dyn FnMut(WebEvent)>,
        output_handler: Closure<dyn FnMut(WebEvent)>,
    }

    impl BrowserRtAudio {
        pub fn new(hub: BrowserMidiHub, pools: NewEnginePools) -> Result<Self> {
            let inner = Rc::new(RefCell::new(Inner {
                phase: Phase::AwaitingGesture,
                generation: 0,
                started: None,
                pools,
                hub,
                graph: None,
                backend: None,
                warnings: Vec::new(),
                input_mode: None,
                microphone: Microphone::Off,
                midi_inputs: BTreeMap::new(),
                midi_outputs: Vec::new(),
                midi_revision: None,
                repaint: None,
            }));
            let hub = inner.borrow().hub.clone();
            hub.forward_tracks(midi_forwarder(Rc::downgrade(&inner)));
            let handler = |mode| {
                let weak = Rc::downgrade(&inner);
                Closure::wrap(Box::new(move |_event: WebEvent| {
                    if let Some(inner) = weak.upgrade() {
                        begin_enable(&inner, mode);
                    }
                }) as Box<dyn FnMut(_)>)
            };
            let microphone_handler = handler(InputMode::Microphone);
            let output_handler = handler(InputMode::OutputOnly);
            button("enable_audio")?.set_onclick(Some(microphone_handler.as_ref().unchecked_ref()));
            button("enable_output_audio")?
                .set_onclick(Some(output_handler.as_ref().unchecked_ref()));
            if let Some(hint) = element("new_engine_permission_hint") {
                let _ = hint.remove_attribute("hidden");
            }
            Ok(Self {
                inner,
                microphone_handler,
                output_handler,
            })
        }

        /// Refuse to start, for example in a mode the new engine backend has no host for.
        pub fn refuse(&self, message: String) {
            self.inner.borrow_mut().phase = Phase::Ended(message);
        }

        pub fn set_repaint_context(&self, context: eframe::egui::Context) {
            self.inner.borrow_mut().repaint = Some(context);
        }

        pub fn audio_context(&self) -> Option<AudioContext> {
            self.inner
                .borrow()
                .graph
                .as_ref()
                .map(|graph| graph.context.clone())
        }

        pub fn microphone_connected(&self) -> bool {
            self.inner.borrow().microphone == Microphone::Connected
        }

        pub fn running(&self) -> bool {
            self.inner.borrow().phase == Phase::Running
        }

        /// The driver state while the application has not started.
        pub fn driver_state(&self) -> AudioDriverState {
            match self.inner.borrow().phase {
                Phase::AwaitingGesture => AudioDriverState::AwaitingGesture,
                Phase::RequestingPermission => AudioDriverState::RequestingPermission,
                Phase::Starting => AudioDriverState::Starting,
                Phase::Running => AudioDriverState::Running,
                Phase::Denied => AudioDriverState::Denied,
                Phase::Failed(_) | Phase::Ended(_) => AudioDriverState::Failed,
            }
        }

        /// What the user needs to know while the application has not started.
        pub fn notice(&self) -> (bool, String) {
            match &self.inner.borrow().phase {
                Phase::AwaitingGesture => (
                    false,
                    "The new engine backend starts when browser audio is enabled (Settings → Audio → browser permissions)".to_owned(),
                ),
                Phase::RequestingPermission | Phase::Starting | Phase::Running => {
                    (false, "Starting the new engine backend…".to_owned())
                }
                Phase::Denied => (
                    false,
                    "Microphone access was denied; enable output-only audio or retry the microphone".to_owned(),
                ),
                Phase::Failed(message) => (
                    true,
                    format!("The new engine backend could not start: {message}"),
                ),
                Phase::Ended(message) => (true, message.clone()),
            }
        }

        /// The backend once its connection is complete, with the warnings of its
        /// preparation. Pumps the connection and enforces the startup timeout.
        pub fn take_backend(&self) -> Option<(Box<dyn Backend>, Vec<String>)> {
            let mut inner = self.inner.borrow_mut();
            if inner.phase == Phase::Starting
                && inner
                    .started
                    .is_some_and(|started| started.elapsed() >= STARTUP_TIMEOUT)
            {
                inner.fail("browser audio startup timed out".to_owned());
                return None;
            }
            let backend = inner.backend.as_mut()?;
            let state = backend.poll().map(|snapshot| snapshot.status.driver_state);
            if backend.is_ready() {
                let backend = inner.backend.take()?;
                inner.phase = Phase::Running;
                inner.started = None;
                tracing::info!("frontend.new_engine.connected");
                return Some((Box::new(backend), std::mem::take(&mut inner.warnings)));
            }
            match state {
                Ok(BackendDriverState::Failed) => {
                    inner.fail("the engine refused the connection".to_owned())
                }
                Err(error) => inner.fail(format!("{error:#}")),
                Ok(_) => {}
            }
            None
        }

        pub fn update_presentation(&self) {
            let inner = self.inner.borrow();
            let microphone = inner.microphone;
            let running = inner.phase == Phase::Running;
            let busy = matches!(inner.phase, Phase::RequestingPermission | Phase::Starting);
            let ended = matches!(inner.phase, Phase::Ended(_));
            let failed_mode = matches!(inner.phase, Phase::Failed(_) | Phase::Denied)
                .then_some(inner.input_mode)
                .flatten();
            let microphone_granted = running && microphone == Microphone::Connected;
            for (id, granted, disabled, text) in [
                (
                    "enable_output_audio",
                    running,
                    running || busy || ended,
                    if failed_mode == Some(InputMode::OutputOnly) {
                        "Retry output-only audio"
                    } else {
                        "Enable output-only audio"
                    },
                ),
                (
                    "enable_audio",
                    microphone_granted,
                    microphone_granted || busy || ended || microphone == Microphone::Requesting,
                    if failed_mode == Some(InputMode::Microphone)
                        || matches!(
                            microphone,
                            Microphone::Denied | Microphone::Ended | Microphone::Failed
                        )
                    {
                        "Retry microphone audio"
                    } else {
                        "Enable microphone audio"
                    },
                ),
            ] {
                let Ok(button) = button(id) else {
                    continue;
                };
                button.set_hidden(false);
                button.set_disabled(disabled);
                if granted {
                    let _ = button.set_attribute("data-permission-granted", "");
                    button.set_text_content(Some("Granted"));
                } else {
                    let _ = button.remove_attribute("data-permission-granted");
                    button.set_text_content(Some(text));
                }
            }
            let output_status = match &inner.phase {
                Phase::RequestingPermission | Phase::Starting => "Starting…",
                Phase::Failed(_) => "Failed",
                Phase::Ended(_) => "Reload the page to start the new engine again",
                _ => "",
            };
            let microphone_status = match (&inner.phase, microphone) {
                (Phase::RequestingPermission, _) | (_, Microphone::Requesting) => {
                    "Requesting permission…"
                }
                (Phase::Denied, _) | (_, Microphone::Denied) => "Denied",
                (_, Microphone::Ended) => "Microphone ended",
                (_, Microphone::Failed) => "Failed",
                _ => "",
            };
            set_text("audio_output_permission_status", output_status);
            set_text("microphone_permission_status", microphone_status);
            if inner
                .graph
                .as_ref()
                .is_some_and(|graph| graph.node.is_some())
                && inner.midi_revision.is_some()
                && inner.midi_revision != Some(inner.hub.endpoint_snapshot().revision)
            {
                set_text(
                    "midi_permission_status",
                    "Reload the page to use changed MIDI ports with the new engine",
                );
            }
            if let Some(status) = element("runtime_status") {
                let _ = status.set_attribute("data-new-engine", &format!("{:?}", inner.phase));
                let _ =
                    status.set_attribute("data-new-engine-microphone", &format!("{microphone:?}"));
                let graph = inner.graph.as_ref();
                let dropped = graph
                    .and_then(|graph| graph.port.as_ref())
                    .map_or(0, SharedPort::midi_dropped);
                let _ = status.set_attribute("data-new-engine-midi-dropped", &dropped.to_string());
                let media_tracks = graph
                    .and_then(|graph| graph.stream.as_ref())
                    .map_or(0, |stream| stream.get_tracks().length());
                let _ = status.set_attribute("data-owned-media-tracks", &media_tracks.to_string());
            }
        }
    }

    impl Drop for BrowserRtAudio {
        fn drop(&mut self) {
            let _ = (&self.microphone_handler, &self.output_handler);
            for id in ["enable_audio", "enable_output_audio"] {
                if let Ok(button) = button(id) {
                    button.set_onclick(None);
                }
            }
            let mut inner = self.inner.borrow_mut();
            inner.backend = None;
            if let Some(port) = inner.graph.as_ref().and_then(|graph| graph.port.as_ref()) {
                port.close(DisconnectReason::Shutdown);
            }
            shutdown_graph(&mut inner);
        }
    }

    fn element(id: &str) -> Option<web_sys::Element> {
        web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id(id))
    }

    fn button(id: &str) -> Result<HtmlButtonElement> {
        element(id)
            .ok_or_else(|| anyhow!("missing #{id} button"))?
            .dyn_into::<HtmlButtonElement>()
            .map_err(|_| anyhow!("#{id} is not a button"))
    }

    fn set_text(id: &str, text: &str) {
        if let Some(element) = element(id) {
            element.set_text_content(Some(text));
        }
    }

    fn begin_enable(inner: &Rc<RefCell<Inner>>, mode: InputMode) {
        let (phase, upgrade) = {
            let state = inner.borrow();
            let upgrade = mode == InputMode::Microphone
                && state.phase == Phase::Running
                && !matches!(
                    state.microphone,
                    Microphone::Connected | Microphone::Requesting
                );
            (state.phase.clone(), upgrade)
        };
        if upgrade {
            connect_microphone(inner);
            return;
        }
        if !matches!(
            phase,
            Phase::AwaitingGesture | Phase::Denied | Phase::Failed(_)
        ) {
            return;
        }
        let context = match AudioContext::new() {
            Ok(context) => context,
            Err(error) => {
                inner
                    .borrow_mut()
                    .fail(format!("AudioContext is unavailable: {error:?}"));
                return;
            }
        };
        let resume = match context.resume() {
            Ok(resume) => resume,
            Err(error) => {
                let _ = context.close();
                inner
                    .borrow_mut()
                    .fail(format!("could not begin AudioContext resume: {error:?}"));
                return;
            }
        };
        let media = match mode {
            InputMode::Microphone => match request_microphone() {
                Ok(media) => Some(media),
                Err(error) => {
                    let _ = context.close();
                    inner.borrow_mut().fail(error.to_string());
                    return;
                }
            },
            InputMode::OutputOnly => None,
        };
        let generation = {
            let mut state = inner.borrow_mut();
            shutdown_graph(&mut state);
            state.generation += 1;
            state.started = Some(Instant::now());
            state.input_mode = Some(mode);
            state.microphone = Microphone::Off;
            state.phase = match mode {
                InputMode::Microphone => Phase::RequestingPermission,
                InputMode::OutputOnly => Phase::Starting,
            };
            state.graph = Some(Graph {
                context: context.clone(),
                node: None,
                stream: None,
                source: None,
                port: None,
                message_handler: None,
                processor_error_handler: None,
                state_handler: None,
                track_ended_handlers: Vec::new(),
            });
            state.generation
        };
        let inner = Rc::clone(inner);
        spawn_local(async move {
            if let Err(error) = start_graph(&inner, generation, context, resume, media).await {
                let mut state = inner.borrow_mut();
                if state.generation != generation {
                    return;
                }
                if js_error_name(&error) == "NotAllowedError" {
                    state.phase = Phase::Denied;
                    state.end_attempt();
                } else {
                    state.fail(js_error_message(&error));
                }
            }
        });
    }

    fn request_microphone() -> Result<js_sys::Promise> {
        let constraints = Object::new();
        for name in ["echoCancellation", "noiseSuppression", "autoGainControl"] {
            let _ = Reflect::set(&constraints, &name.into(), &JsValue::FALSE);
        }
        let media = MediaStreamConstraints::new();
        media.set_audio(&constraints.into());
        web_sys::window()
            .ok_or_else(|| anyhow!("browser window is unavailable"))?
            .navigator()
            .media_devices()
            .and_then(|devices| devices.get_user_media_with_constraints(&media))
            .map_err(|error| anyhow!("getUserMedia is unavailable: {error:?}"))
    }

    fn current(inner: &Rc<RefCell<Inner>>, generation: u64) -> bool {
        inner.borrow().generation == generation
    }

    async fn start_graph(
        inner: &Rc<RefCell<Inner>>,
        generation: u64,
        context: AudioContext,
        resume: js_sys::Promise,
        media: Option<js_sys::Promise>,
    ) -> std::result::Result<(), JsValue> {
        let stream = match media {
            Some(media) => Some(JsFuture::from(media).await?.dyn_into::<MediaStream>()?),
            None => None,
        };
        if !current(inner, generation) {
            if let Some(stream) = &stream {
                stop_stream(stream);
            }
            return Ok(());
        }
        {
            let mut state = inner.borrow_mut();
            state.phase = Phase::Starting;
            state.started = Some(Instant::now());
            if let (Some(graph), Some(stream)) = (state.graph.as_mut(), &stream) {
                graph.stream = Some(stream.clone());
            }
        }
        let module = load_module(&context).await?;
        let hub = inner.borrow().hub.clone();
        wait_for_midi_permission(&hub).await;
        if !current(inner, generation) {
            return Ok(());
        }
        inner.borrow_mut().started = Some(Instant::now());

        let capture = stream
            .as_ref()
            .map(|stream| capture_channel_count(stream).unwrap_or(1) as usize)
            .unwrap_or(RESERVED_CAPTURE_CHANNELS)
            .clamp(1, MAX_DEVICE_CHANNELS as usize);
        let destination = context
            .destination()
            .channel_count()
            .clamp(1, MAX_DEVICE_CHANNELS) as usize;
        let endpoints = hub.endpoints();
        let sources = endpoints
            .iter()
            .filter(|endpoint| endpoint.direction == MidiEndpointDirection::Output)
            .collect::<Vec<_>>();
        let sinks = endpoints
            .iter()
            .filter(|endpoint| endpoint.direction == MidiEndpointDirection::Input)
            .collect::<Vec<_>>();
        let setup = AudioSetup {
            sample_rate: context.sample_rate().round() as u32,
            capture,
            destination,
            midi_inputs: sources.iter().map(|port| port.name.clone()).collect(),
            midi_outputs: sinks.iter().map(|port| port.name.clone()).collect(),
        };
        let pools = inner.borrow().pools;
        let (config, warnings) = worklet_config(&pools, &setup);
        let record = config.encode().map_err(|error| {
            JsValue::from_str(&format!("invalid engine configuration: {error:?}"))
        })?;

        let processor_options = Object::new();
        Reflect::set(&processor_options, &"wasmModule".into(), module.as_ref())?;
        Reflect::set(
            &processor_options,
            &"config".into(),
            &Uint8Array::from(record.as_slice()),
        )?;
        let options = Object::new();
        let output_channels = Array::of1(&JsValue::from_f64(destination as f64));
        for (name, value) in [
            ("numberOfInputs", JsValue::from_f64(1.0)),
            ("numberOfOutputs", JsValue::from_f64(1.0)),
            ("channelCount", JsValue::from_f64(capture as f64)),
            ("channelCountMode", "explicit".into()),
            ("channelInterpretation", "discrete".into()),
            ("outputChannelCount", output_channels.into()),
            ("processorOptions", processor_options.into()),
        ] {
            Reflect::set(&options, &name.into(), &value)?;
        }
        let node =
            AudioWorkletNode::new_with_options(&context, PROCESSOR_NAME, options.unchecked_ref())?;
        node.connect_with_audio_node(&context.destination())?;
        let target = node.port()?;
        let port = SharedPort::new(target.clone(), config.midi_in_bytes);

        let weak = Rc::downgrade(inner);
        let message_port = port.clone();
        let message_handler = Closure::wrap(Box::new(move |event: MessageEvent| {
            if let Some(inner) = weak.upgrade() {
                receive(&inner, &message_port, &event.data());
            }
        }) as Box<dyn FnMut(_)>);
        target.set_onmessage(Some(message_handler.as_ref().unchecked_ref()));
        target.start();

        let weak = Rc::downgrade(inner);
        let processor_error_handler = Closure::wrap(Box::new(move |_event: WebEvent| {
            if let Some(inner) = weak.upgrade() {
                let mut state = inner.borrow_mut();
                if state.generation == generation {
                    state.fail("the AudioWorklet processor terminated".to_owned());
                }
            }
        }) as Box<dyn FnMut(_)>);
        node.set_onprocessorerror(Some(processor_error_handler.as_ref().unchecked_ref()));

        let weak = Rc::downgrade(inner);
        let state_context = context.clone();
        let state_handler = Closure::wrap(Box::new(move |_event: WebEvent| {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let mut state = inner.borrow_mut();
            if state.generation == generation && state_context.state() == AudioContextState::Closed
            {
                state.fail("the AudioContext closed".to_owned());
            }
            state.repaint();
        }) as Box<dyn FnMut(_)>);
        context.set_onstatechange(Some(state_handler.as_ref().unchecked_ref()));

        let client = ProtocolClient::new(RecordClient::new(port.clone()));
        let backend = RtBackend::begin(client, NoWait)
            .map_err(|error| JsValue::from_str(&format!("{error:#}")))?;
        tracing::info!(
            sample_rate = setup.sample_rate,
            capture,
            destination,
            midi_inputs = setup.midi_inputs.len(),
            midi_outputs = setup.midi_outputs.len(),
            "frontend.new_engine.prepared"
        );

        {
            let mut state = inner.borrow_mut();
            if state.generation != generation {
                return Ok(());
            }
            let midi_revision = state.hub.endpoint_snapshot().revision;
            state.midi_revision = Some(midi_revision);
            state.midi_inputs = sources
                .iter()
                .zip(0u8..)
                .map(|(endpoint, index)| (endpoint.id.clone(), index))
                .collect();
            state.midi_outputs = sinks.iter().map(|endpoint| endpoint.id.clone()).collect();
            state.warnings = warnings;
            state.backend = Some(backend);
            let Some(graph) = state.graph.as_mut() else {
                return Ok(());
            };
            graph.node = Some(node);
            graph.port = Some(port);
            graph.message_handler = Some(message_handler);
            graph.processor_error_handler = Some(processor_error_handler);
            graph.state_handler = Some(state_handler);
        }
        if let Some(stream) = stream {
            attach_microphone(inner, generation, stream)?;
        }

        let weak = Rc::downgrade(inner);
        spawn_local(async move {
            if let Err(error) = JsFuture::from(resume).await {
                if let Some(inner) = weak.upgrade() {
                    let mut state = inner.borrow_mut();
                    if state.generation == generation {
                        state.fail(js_error_message(&error));
                    }
                }
            }
        });
        Ok(())
    }

    fn connect_microphone(inner: &Rc<RefCell<Inner>>) {
        let promise = match request_microphone() {
            Ok(promise) => promise,
            Err(error) => {
                tracing::warn!(error = %error, "frontend.new_engine.microphone_unavailable");
                inner.borrow_mut().microphone = Microphone::Failed;
                return;
            }
        };
        let generation = {
            let mut state = inner.borrow_mut();
            state.microphone = Microphone::Requesting;
            state.generation
        };
        let inner = Rc::clone(inner);
        spawn_local(async move {
            let result = async {
                let stream = JsFuture::from(promise).await?.dyn_into::<MediaStream>()?;
                attach_microphone(&inner, generation, stream)
            }
            .await;
            if let Err(error) = result {
                let mut state = inner.borrow_mut();
                if state.generation == generation {
                    state.microphone = if js_error_name(&error) == "NotAllowedError" {
                        Microphone::Denied
                    } else {
                        tracing::warn!(
                            error = %js_error_message(&error),
                            "frontend.new_engine.microphone_failed"
                        );
                        Microphone::Failed
                    };
                }
                state.repaint();
            }
        });
    }

    /// Connects a microphone stream to the running node's input.
    fn attach_microphone(
        inner: &Rc<RefCell<Inner>>,
        generation: u64,
        stream: MediaStream,
    ) -> std::result::Result<(), JsValue> {
        let mut state = inner.borrow_mut();
        let current = state.generation == generation;
        let Some(graph) = state.graph.as_mut().filter(|_| current) else {
            stop_stream(&stream);
            return Ok(());
        };
        let Some(node) = &graph.node else {
            stop_stream(&stream);
            return Ok(());
        };
        let source = graph.context.create_media_stream_source(&stream)?;
        source.connect_with_audio_node(node)?;
        if let Some(previous) = graph.source.replace(source) {
            let _ = previous.disconnect();
        }
        if let Some(previous) = graph.stream.replace(stream.clone()) {
            if previous != stream {
                stop_stream(&previous);
            }
        }
        graph.track_ended_handlers.clear();
        for value in stream.get_tracks().iter() {
            let track = value.dyn_into::<MediaStreamTrack>()?;
            let weak = Rc::downgrade(inner);
            let handler = Closure::wrap(Box::new(move |_event: WebEvent| {
                let Some(inner) = weak.upgrade() else {
                    return;
                };
                let mut state = inner.borrow_mut();
                if state.generation != generation {
                    return;
                }
                if let Some(graph) = state.graph.as_mut() {
                    if let Some(source) = graph.source.take() {
                        let _ = source.disconnect();
                    }
                }
                state.microphone = Microphone::Ended;
                tracing::warn!("frontend.new_engine.microphone_ended");
                state.repaint();
            }) as Box<dyn FnMut(_)>);
            track.set_onended(Some(handler.as_ref().unchecked_ref()));
            graph.track_ended_handlers.push(handler);
        }
        state.microphone = Microphone::Connected;
        Ok(())
    }

    fn receive(inner: &Rc<RefCell<Inner>>, port: &SharedPort, data: &JsValue) {
        let field = |name: &str| Reflect::get(data, &name.into()).unwrap_or(JsValue::UNDEFINED);
        match field("kind").as_string().as_deref() {
            Some("records") => port.receive(&Uint8Array::new(&field("bytes")).to_vec()),
            Some("midi") => send_midi_out(inner, &Uint8Array::new(&field("bytes")).to_vec()),
            Some("failed") => {
                let message = format!(
                    "the AudioWorklet failed to {}: {}",
                    field("stage").as_string().unwrap_or_default(),
                    field("message").as_string().unwrap_or_default()
                );
                inner.borrow_mut().fail(message);
            }
            _ => {}
        }
        inner.borrow().repaint();
    }

    fn send_midi_out(inner: &Rc<RefCell<Inner>>, bytes: &[u8]) {
        let state = inner.borrow();
        let Some(context) = state.graph.as_ref().map(|graph| &graph.context) else {
            return;
        };
        let clock = midi_clock(context);
        for event in lane::events(bytes) {
            let Ok(event) = event else {
                break;
            };
            let Some(endpoint) = state.midi_outputs.get(usize::from(event.port)) else {
                continue;
            };
            let time = clock.output_time_ms(event.frame).max(clock.performance_ms);
            if let Err(error) = state.hub.send_at(endpoint, event.bytes, Some(time)) {
                tracing::debug!(error = %error, "frontend.new_engine.midi_out_failed");
            }
        }
    }

    fn midi_forwarder(inner: Weak<RefCell<Inner>>) -> TrackForwarder {
        Rc::new(move |endpoint_id, data, time_ms| {
            let Some(inner) = inner.upgrade() else {
                return;
            };
            let state = inner.borrow();
            let Some(&index) = state.midi_inputs.get(endpoint_id) else {
                return;
            };
            let Some(graph) = state.graph.as_ref() else {
                return;
            };
            let Some(port) = graph.port.as_ref() else {
                return;
            };
            let frame = midi_clock(&graph.context).input_frame(time_ms, MIDI_IN_LEAD_FRAMES);
            port.push_midi(MidiEvent {
                frame,
                port: index,
                bytes: data,
            });
        })
    }

    fn number(object: &JsValue, name: &str) -> f64 {
        Reflect::get(object, &name.into())
            .ok()
            .and_then(|value| value.as_f64())
            .filter(|value| value.is_finite())
            .unwrap_or(0.0)
    }

    fn performance_now() -> f64 {
        web_sys::window()
            .and_then(|window| window.performance())
            .map_or(0.0, |performance| performance.now())
    }

    /// The relation between performance time and the context's frames now.
    fn midi_clock(context: &AudioContext) -> MidiClock {
        let sample_rate = f64::from(context.sample_rate());
        let stamp = Reflect::get(context, &"getOutputTimestamp".into())
            .ok()
            .and_then(|function| function.dyn_into::<Function>().ok())
            .and_then(|function| function.call0(context).ok());
        match stamp {
            Some(stamp) if number(&stamp, "performanceTime") > 0.0 => MidiClock {
                context_seconds: number(&stamp, "contextTime"),
                performance_ms: number(&stamp, "performanceTime"),
                sample_rate,
                latency_seconds: number(context, "baseLatency") + number(context, "outputLatency"),
            },
            _ => MidiClock {
                context_seconds: context.current_time(),
                performance_ms: performance_now(),
                sample_rate,
                latency_seconds: 0.0,
            },
        }
    }

    async fn sleep(duration: Duration) {
        let promise = js_sys::Promise::new(&mut |resolve, _reject| {
            if let Some(window) = web_sys::window() {
                let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                    &resolve,
                    duration.as_millis() as i32,
                );
            }
        });
        let _ = JsFuture::from(promise).await;
    }

    /// Lets a Web MIDI permission request that is still open decide which ports the engine
    /// is prepared with.
    async fn wait_for_midi_permission(hub: &BrowserMidiHub) {
        let started = Instant::now();
        while hub.state() == BrowserMidiState::RequestingPermission
            && started.elapsed() < MIDI_PERMISSION_WAIT
        {
            sleep(Duration::from_millis(50)).await;
        }
    }

    async fn load_module(
        context: &AudioContext,
    ) -> std::result::Result<WebAssembly::Module, JsValue> {
        let window =
            web_sys::window().ok_or_else(|| JsValue::from_str("browser window disappeared"))?;
        let embedded = Reflect::get(window.as_ref(), &EMBEDDED_ASSETS.into())?;
        let bytes = if !embedded.is_null() && !embedded.is_undefined() {
            let script = Reflect::get(&embedded, &"rtModuleUrl".into())?
                .as_string()
                .ok_or_else(|| JsValue::from_str("embedded new engine processor URL is invalid"))?;
            JsFuture::from(context.audio_worklet()?.add_module(&script)?).await?;
            Reflect::get(&embedded, &"rtWasmBytes".into())?
        } else {
            JsFuture::from(context.audio_worklet()?.add_module(PROCESSOR_SCRIPT_URL)?).await?;
            let response = JsFuture::from(window.fetch_with_str(MODULE_URL))
                .await?
                .dyn_into::<Response>()?;
            if !response.ok() {
                return Err(JsValue::from_str(
                    "could not fetch the new engine's AudioWorklet module",
                ));
            }
            JsFuture::from(response.array_buffer()?).await?
        };
        JsFuture::from(WebAssembly::compile(&bytes))
            .await?
            .dyn_into::<WebAssembly::Module>()
    }

    fn capture_channel_count(stream: &MediaStream) -> Option<u32> {
        stream
            .get_audio_tracks()
            .iter()
            .next()
            .and_then(|value| value.dyn_into::<MediaStreamTrack>().ok())
            .and_then(|track| track.get_settings().get_channel_count())
            .and_then(|channels| u32::try_from(channels).ok())
            .filter(|channels| *channels > 0)
    }

    fn stop_stream(stream: &MediaStream) {
        for value in stream.get_tracks().iter() {
            if let Ok(track) = value.dyn_into::<MediaStreamTrack>() {
                track.set_onended(None);
                track.stop();
            }
        }
    }

    fn shutdown_graph(state: &mut Inner) {
        let Some(graph) = state.graph.take() else {
            return;
        };
        if let Some(stream) = &graph.stream {
            stop_stream(stream);
        }
        if let Some(source) = &graph.source {
            let _ = source.disconnect();
        }
        if let Some(node) = &graph.node {
            node.set_onprocessorerror(None);
            if let Ok(port) = node.port() {
                let _ = post_stop(&port);
                port.set_onmessage(None);
                port.close();
            }
            let _ = node.disconnect();
        }
        graph.context.set_onstatechange(None);
        let _ = graph.context.close();
        state.microphone = Microphone::Off;
        // One of the graph's handlers may be running now; release them afterwards.
        spawn_local(async move { drop(graph) });
    }

    fn post_stop(port: &MessagePort) -> std::result::Result<(), JsValue> {
        let message = Object::new();
        Reflect::set(&message, &"kind".into(), &"stop".into())?;
        port.post_message(&message)
    }

    fn js_error_name(error: &JsValue) -> String {
        Reflect::get(error, &"name".into())
            .ok()
            .and_then(|name| name.as_string())
            .unwrap_or_default()
    }

    fn js_error_message(error: &JsValue) -> String {
        Reflect::get(error, &"message".into())
            .ok()
            .and_then(|message| message.as_string())
            .filter(|message| !message.is_empty())
            .or_else(|| error.as_string())
            .unwrap_or_else(|| format!("browser audio startup failed: {error:?}"))
    }
}

#[cfg(target_arch = "wasm32")]
pub use platform::BrowserRtAudio;

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;

    fn setup() -> AudioSetup {
        AudioSetup {
            sample_rate: 44_100,
            capture: 1,
            destination: 2,
            midi_inputs: vec!["Keys".to_owned()],
            midi_outputs: vec!["Synth".to_owned(), "Keys".to_owned()],
        }
    }

    #[shoop_test]
    fn capacity_settings_and_the_audio_setup_prepare_the_worklet() {
        let pools = NewEnginePools {
            tracks: 5,
            loops: 40,
            channels: 2,
            loop_memory_mb: 32,
        };
        let (config, warnings) = worklet_config(&pools, &setup());
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(config.validate(), Ok(()));
        let engine = &config.engine;
        assert_eq!(engine.sample_rate, 44_100);
        assert_eq!(engine.max_frames, 128);
        assert_eq!(
            engine.segments,
            [Segment {
                strips: 5,
                loops: 40
            }]
        );
        assert_eq!(engine.strip_channels, 2);
        assert_eq!(engine.master_channels, 2);
        assert_eq!(engine.master_connections, [(0, 0), (1, 1)]);
        assert_eq!(engine.resource_pages * engine.page_bytes, 32 << 20);
        assert_eq!(engine.smoothing_limit_frames, 4410);
        assert!(engine.pending_transitions >= 80);
        let names = |list: &[shoop_rt_protocol::Name]| {
            list.iter()
                .map(|name| name.as_str().to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(&engine.host.audio_inputs), ["webaudio:capture_1"]);
        assert_eq!(names(&engine.host.midi_inputs), ["webmidi:Keys in"]);
        assert_eq!(
            names(&engine.host.midi_outputs),
            ["webmidi:Synth out", "webmidi:Keys out"]
        );
        let record = config.encode().unwrap();
        assert_eq!(WorkletConfig::decode(&record), Ok(config));
    }

    #[shoop_test]
    fn the_browser_bounds_loop_memory_and_ports_with_warnings() {
        let pools = NewEnginePools {
            loop_memory_mb: 2047,
            ..NewEnginePools::default()
        };
        let many = (0..MAX_HOST_PORTS + 1)
            .map(|index| format!("Port {index}"))
            .collect::<Vec<_>>();
        let (config, warnings) = worklet_config(
            &pools,
            &AudioSetup {
                destination: 1,
                midi_inputs: many,
                ..setup()
            },
        );
        assert_eq!(
            config.engine.resource_pages * config.engine.page_bytes,
            (BROWSER_MAX_LOOP_MEMORY_MB as usize) << 20
        );
        assert_eq!(config.engine.master_connections, [(0, 0), (1, 0)]);
        assert_eq!(config.engine.host.midi_inputs.len(), MAX_HOST_PORTS);
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(warnings[0].contains("2047 MiB"), "{warnings:?}");
        let (default_config, default_warnings) =
            worklet_config(&NewEnginePools::default(), &setup());
        assert!(default_warnings.is_empty(), "{default_warnings:?}");
        assert_eq!(default_config.engine.host.audio_outputs.len(), 2);
        assert!(RESERVED_CAPTURE_CHANNELS >= 2);
    }
}
