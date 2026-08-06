use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use js_sys::{Array, Object, Reflect, WebAssembly};
use shoop_audio_protocol::{
    Command, CommandEnvelope, Event, EventEnvelope, WaveformChunk, WireLoopMode, WireSnapshot,
    WireTrackControl, COMMAND_CAPACITY, MAX_AUDIO_CHANNELS, PROTOCOL_VERSION, STATUS_INTERVAL_MS,
    WAVEFORM_CHUNK_SAMPLES,
};
use shoop_backend::{
    Backend, BackendDriverState, BackendLoopId, BackendLoopMode, BackendLoopState, BackendSnapshot,
    BackendStatus, BackendTrackControl, BackendTrackCreation, BackendTrackId, BackendTrackState,
    DirectTrackRequest,
};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    AudioContext, AudioContextState, AudioWorkletNode, AudioWorkletNodeOptions, Event as WebEvent,
    HtmlButtonElement, MediaStream, MediaStreamAudioSourceNode, MediaStreamConstraints,
    MediaStreamTrack, MessageEvent, MessagePort, Response,
};
use web_time::Instant;

const WORKLET_NAME: &str = "shoop-audio-processor";
const WORKLET_SCRIPT_URL: &str = "./audio_worklet.js";
const WORKLET_WASM_URL: &str = "./generated/shoop_audio_worklet.wasm";
const MAX_QUANTUM: u32 = 2048;

#[derive(Default)]
pub(crate) struct Transport {
    generation: u64,
    driver_state: BackendDriverState,
    error: Option<String>,
    port: Option<MessagePort>,
    journal: Vec<Command>,
    inbound: VecDeque<EventEnvelope>,
    next_sequence: u64,
    in_flight: usize,
    overflows: u32,
}

impl Transport {
    fn new() -> Self {
        Self {
            driver_state: BackendDriverState::AwaitingGesture,
            next_sequence: 1,
            ..Default::default()
        }
    }

    fn journal(&mut self, command: Command) -> Result<()> {
        if let Some(existing) = self
            .journal
            .iter_mut()
            .rev()
            .find(|existing| command.supersedes_in_journal(existing))
        {
            *existing = command.clone();
        } else {
            if self.journal.len() >= COMMAND_CAPACITY {
                self.overflows = self.overflows.saturating_add(1);
                return Err(anyhow!("browser audio command journal is full"));
            }
            self.journal.push(command.clone());
        }
        if self.port.is_some() {
            self.send(command)?;
        }
        Ok(())
    }

    fn ephemeral(&mut self, command: Command) -> Result<()> {
        if self.port.is_none() {
            return Err(anyhow!("browser audio worklet is not running"));
        }
        self.send(command)
    }

    fn send(&mut self, command: Command) -> Result<()> {
        if self.in_flight >= COMMAND_CAPACITY {
            self.overflows = self.overflows.saturating_add(1);
            return Err(anyhow!("browser audio command queue is full"));
        }
        let envelope = CommandEnvelope::new(self.next_sequence, command);
        let json = serde_json::to_string(&envelope)?;
        self.port
            .as_ref()
            .ok_or_else(|| anyhow!("browser audio worklet is unavailable"))?
            .post_message(&JsValue::from_str(&json))
            .map_err(|error| anyhow!("could not post worklet command: {error:?}"))?;
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.in_flight += 1;
        Ok(())
    }

    fn attach(&mut self, port: MessagePort, generation: u64) -> Result<()> {
        self.generation = generation;
        self.port = Some(port);
        self.inbound.clear();
        self.in_flight = 0;
        self.next_sequence = 1;
        let journal = self.journal.clone();
        for command in journal {
            self.send(command)?;
        }
        Ok(())
    }

    fn receive(&mut self, generation: u64, json: &str) {
        if generation != self.generation {
            return;
        }
        let event = match serde_json::from_str::<EventEnvelope>(json) {
            Ok(event) if event.version == PROTOCOL_VERSION => event,
            Ok(event) => {
                self.fail(format!(
                    "worklet protocol version {} does not match {PROTOCOL_VERSION}",
                    event.version
                ));
                return;
            }
            Err(error) => {
                self.fail(format!("malformed worklet event: {error}"));
                return;
            }
        };
        self.in_flight = self.in_flight.saturating_sub(1);
        if let Event::Error { message } = &event.event {
            self.fail(message.clone());
        }
        if self.inbound.len() >= COMMAND_CAPACITY {
            self.overflows = self.overflows.saturating_add(1);
            self.fail("browser audio event queue is full".to_owned());
            return;
        }
        self.inbound.push_back(event);
    }

    fn fail(&mut self, message: String) {
        if self.error.is_none() {
            self.error = Some(message);
        }
        self.driver_state = BackendDriverState::Failed;
        self.port = None;
    }
}

struct BrowserControllerInner {
    generation: u64,
    startup_started: Option<Instant>,
    transport: Rc<RefCell<Transport>>,
    context: Option<AudioContext>,
    stream: Option<MediaStream>,
    source: Option<MediaStreamAudioSourceNode>,
    node: Option<AudioWorkletNode>,
    message_handler: Option<Closure<dyn FnMut(MessageEvent)>>,
    processor_error_handler: Option<Closure<dyn FnMut(WebEvent)>>,
    context_state_handler: Option<Closure<dyn FnMut(WebEvent)>>,
    track_ended_handlers: Vec<Closure<dyn FnMut(WebEvent)>>,
}

pub struct BrowserAudioController {
    inner: Rc<RefCell<BrowserControllerInner>>,
    enable_handler: Closure<dyn FnMut(WebEvent)>,
    suspend_handler: Closure<dyn FnMut(WebEvent)>,
    resume_handler: Closure<dyn FnMut(WebEvent)>,
    fail_handler: Closure<dyn FnMut(WebEvent)>,
    track_end_handler: Closure<dyn FnMut(WebEvent)>,
    saturate_handler: Closure<dyn FnMut(WebEvent)>,
    shutdown_handler: Closure<dyn FnMut(WebEvent)>,
}

impl BrowserAudioController {
    pub fn new(transport: Rc<RefCell<Transport>>) -> Result<Self> {
        let window = web_sys::window().ok_or_else(|| anyhow!("browser window is unavailable"))?;
        if !supports_hosted_audio(&window) {
            transport.borrow_mut().driver_state = BackendDriverState::Unsupported;
        }
        let inner = Rc::new(RefCell::new(BrowserControllerInner {
            generation: 0,
            startup_started: None,
            transport,
            context: None,
            stream: None,
            source: None,
            node: None,
            message_handler: None,
            processor_error_handler: None,
            context_state_handler: None,
            track_ended_handlers: Vec::new(),
        }));
        let weak = Rc::downgrade(&inner);
        let enable_handler = Closure::wrap(Box::new(move |_event: WebEvent| {
            if let Some(inner) = weak.upgrade() {
                begin_enable(inner);
            }
        }) as Box<dyn FnMut(_)>);
        let button = enable_button()?;
        button.set_onclick(Some(enable_handler.as_ref().unchecked_ref()));

        let weak = Rc::downgrade(&inner);
        let suspend_handler = Closure::wrap(Box::new(move |_event: WebEvent| {
            if let Some(inner) = weak.upgrade() {
                if let Some(context) = &inner.borrow().context {
                    let _ = context.suspend();
                }
            }
        }) as Box<dyn FnMut(_)>);
        let weak = Rc::downgrade(&inner);
        let resume_handler = Closure::wrap(Box::new(move |_event: WebEvent| {
            if let Some(inner) = weak.upgrade() {
                if let Some(context) = &inner.borrow().context {
                    let _ = context.resume();
                }
            }
        }) as Box<dyn FnMut(_)>);
        let weak = Rc::downgrade(&inner);
        let fail_handler = Closure::wrap(Box::new(move |_event: WebEvent| {
            if let Some(inner) = weak.upgrade() {
                let transport = inner.borrow().transport.clone();
                let _ = transport.borrow_mut().ephemeral(Command::Shutdown);
            }
        }) as Box<dyn FnMut(_)>);
        let weak = Rc::downgrade(&inner);
        let track_end_handler = Closure::wrap(Box::new(move |_event: WebEvent| {
            let stream = weak
                .upgrade()
                .and_then(|inner| inner.borrow().stream.clone());
            let Some(value) = stream.and_then(|stream| stream.get_audio_tracks().iter().next())
            else {
                return;
            };
            if let Ok(track) = value.dyn_into::<MediaStreamTrack>() {
                track.stop();
                if let Ok(event) = WebEvent::new("ended") {
                    let _ = track.dispatch_event(&event);
                }
            }
        }) as Box<dyn FnMut(_)>);
        let weak = Rc::downgrade(&inner);
        let saturate_handler = Closure::wrap(Box::new(move |_event: WebEvent| {
            if let Some(inner) = weak.upgrade() {
                let transport = inner.borrow().transport.clone();
                for _ in 0..=COMMAND_CAPACITY {
                    let _ = transport.borrow_mut().ephemeral(Command::Poll);
                }
            }
        }) as Box<dyn FnMut(_)>);
        let weak = Rc::downgrade(&inner);
        let shutdown_handler = Closure::wrap(Box::new(move |_event: WebEvent| {
            if let Some(inner) = weak.upgrade() {
                shutdown_inner(&mut inner.borrow_mut());
            }
        }) as Box<dyn FnMut(_)>);
        for (name, handler) in [
            ("shoop-test-audio-suspend", &suspend_handler),
            ("shoop-test-audio-resume", &resume_handler),
            ("shoop-test-audio-fail", &fail_handler),
            ("shoop-test-audio-track-end", &track_end_handler),
            ("shoop-test-audio-saturate", &saturate_handler),
            ("shoop-test-audio-shutdown", &shutdown_handler),
        ] {
            window
                .add_event_listener_with_callback(name, handler.as_ref().unchecked_ref())
                .map_err(|error| anyhow!("could not install {name} listener: {error:?}"))?;
        }
        Ok(Self {
            inner,
            enable_handler,
            suspend_handler,
            resume_handler,
            fail_handler,
            track_end_handler,
            saturate_handler,
            shutdown_handler,
        })
    }

    pub fn state(&self) -> BackendDriverState {
        self.inner.borrow().transport.borrow().driver_state
    }

    pub fn update_presentation(&self) {
        let (state, generation, owned_media_tracks) = {
            let mut inner = self.inner.borrow_mut();
            let mut state = inner.transport.borrow().driver_state;
            if matches!(
                state,
                BackendDriverState::Starting | BackendDriverState::Suspended
            ) && inner
                .startup_started
                .is_some_and(|started| started.elapsed() >= Duration::from_secs(15))
            {
                inner
                    .transport
                    .borrow_mut()
                    .fail("browser audio startup timed out".to_owned());
                state = BackendDriverState::Failed;
            }
            if state == BackendDriverState::Failed && inner.context.is_some() {
                shutdown_graph(&mut inner);
            }
            let owned_media_tracks = inner
                .stream
                .as_ref()
                .map(|stream| stream.get_tracks().length())
                .unwrap_or(0);
            (state, inner.generation, owned_media_tracks)
        };
        if let Some(element) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id("runtime_status"))
        {
            let _ = element.set_attribute("data-audio-generation", &generation.to_string());
            let _ =
                element.set_attribute("data-owned-media-tracks", &owned_media_tracks.to_string());
        }
        if let Ok(button) = enable_button() {
            button.set_hidden(matches!(
                state,
                BackendDriverState::RequestingPermission
                    | BackendDriverState::Starting
                    | BackendDriverState::Running
                    | BackendDriverState::Suspended
            ));
            button.set_text_content(Some(
                if matches!(
                    state,
                    BackendDriverState::Denied
                        | BackendDriverState::Failed
                        | BackendDriverState::Stopped
                ) {
                    "Retry microphone audio"
                } else {
                    "Enable microphone audio"
                },
            ));
        }
    }

    pub fn shutdown(&self) {
        shutdown_inner(&mut self.inner.borrow_mut());
    }
}

impl Drop for BrowserAudioController {
    fn drop(&mut self) {
        let _ = &self.enable_handler;
        if let Ok(button) = enable_button() {
            button.set_onclick(None);
        }
        if let Some(window) = web_sys::window() {
            for (name, handler) in [
                ("shoop-test-audio-suspend", &self.suspend_handler),
                ("shoop-test-audio-resume", &self.resume_handler),
                ("shoop-test-audio-fail", &self.fail_handler),
                ("shoop-test-audio-track-end", &self.track_end_handler),
                ("shoop-test-audio-saturate", &self.saturate_handler),
                ("shoop-test-audio-shutdown", &self.shutdown_handler),
            ] {
                let _ = window
                    .remove_event_listener_with_callback(name, handler.as_ref().unchecked_ref());
            }
        }
        self.shutdown();
    }
}

fn enable_button() -> Result<HtmlButtonElement> {
    web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id("enable_audio"))
        .ok_or_else(|| anyhow!("missing #enable_audio button"))?
        .dyn_into::<HtmlButtonElement>()
        .map_err(|_| anyhow!("#enable_audio is not a button"))
}

fn begin_enable(inner: Rc<RefCell<BrowserControllerInner>>) {
    if matches!(
        inner.borrow().transport.borrow().driver_state,
        BackendDriverState::RequestingPermission
            | BackendDriverState::Starting
            | BackendDriverState::Running
            | BackendDriverState::Suspended
    ) {
        return;
    }
    let Some(window) = web_sys::window() else {
        inner
            .borrow()
            .transport
            .borrow_mut()
            .fail("browser window is unavailable".to_owned());
        return;
    };
    if !supports_hosted_audio(&window) {
        let transport = inner.borrow().transport.clone();
        transport.borrow_mut().driver_state = BackendDriverState::Unsupported;
        transport.borrow_mut().error = Some(
            "Microphone audio requires HTTPS or localhost; use ?offline=1 for explicit dummy mode"
                .to_owned(),
        );
        return;
    }

    let context = match AudioContext::new() {
        Ok(context) => context,
        Err(error) => {
            inner
                .borrow()
                .transport
                .borrow_mut()
                .fail(format!("AudioContext is unavailable: {error:?}"));
            return;
        }
    };
    let resume = match context.resume() {
        Ok(resume) => resume,
        Err(error) => {
            inner
                .borrow()
                .transport
                .borrow_mut()
                .fail(format!("could not begin AudioContext resume: {error:?}"));
            return;
        }
    };
    let constraints = microphone_constraints();
    let media = match window
        .navigator()
        .media_devices()
        .and_then(|devices| devices.get_user_media_with_constraints(&constraints))
    {
        Ok(media) => media,
        Err(error) => {
            inner
                .borrow()
                .transport
                .borrow_mut()
                .fail(format!("getUserMedia is unavailable: {error:?}"));
            return;
        }
    };

    let generation = {
        let mut inner = inner.borrow_mut();
        shutdown_inner(&mut inner);
        inner.generation = inner.generation.saturating_add(1);
        inner.startup_started = Some(Instant::now());
        inner.context = Some(context.clone());
        let mut transport = inner.transport.borrow_mut();
        transport.driver_state = BackendDriverState::RequestingPermission;
        transport.error = None;
        inner.generation
    };

    wasm_bindgen_futures::spawn_local(async move {
        let result = start_audio_graph(inner.clone(), generation, context, resume, media).await;
        if let Err(error) = result {
            let state = if js_error_name(&error) == "NotAllowedError" {
                BackendDriverState::Denied
            } else {
                BackendDriverState::Failed
            };
            let mut inner = inner.borrow_mut();
            if inner.generation == generation {
                inner.transport.borrow_mut().driver_state = state;
                inner.transport.borrow_mut().error = Some(js_error_message(&error));
                shutdown_graph(&mut inner);
            }
        }
    });
}

async fn start_audio_graph(
    inner: Rc<RefCell<BrowserControllerInner>>,
    generation: u64,
    context: AudioContext,
    resume: js_sys::Promise,
    media: js_sys::Promise,
) -> std::result::Result<(), JsValue> {
    let stream = JsFuture::from(media).await?.dyn_into::<MediaStream>()?;
    publish_track_settings(&stream);
    if inner.borrow().generation != generation {
        stop_stream(&stream);
        return Ok(());
    }
    {
        let mut inner = inner.borrow_mut();
        inner.startup_started = Some(Instant::now());
        inner.transport.borrow_mut().driver_state = BackendDriverState::Starting;
    }

    JsFuture::from(context.audio_worklet()?.add_module(WORKLET_SCRIPT_URL)?).await?;
    let response = JsFuture::from(
        web_sys::window()
            .ok_or_else(|| JsValue::from_str("browser window disappeared"))?
            .fetch_with_str(WORKLET_WASM_URL),
    )
    .await?
    .dyn_into::<Response>()?;
    if !response.ok() {
        stop_stream(&stream);
        return Err(JsValue::from_str(
            "could not fetch the AudioWorklet Wasm module",
        ));
    }
    let bytes = JsFuture::from(response.array_buffer()?).await?;
    let module = WebAssembly::Module::new(&bytes)?;

    let processor_options = Object::new();
    Reflect::set(&processor_options, &"wasmModule".into(), module.as_ref())?;
    Reflect::set(
        &processor_options,
        &"maxQuantum".into(),
        &JsValue::from_f64(MAX_QUANTUM as f64),
    )?;
    let options = AudioWorkletNodeOptions::new();
    options.set_number_of_inputs(1);
    options.set_number_of_outputs(1);
    let output_channels = Array::new();
    output_channels.push(&JsValue::from_f64(MAX_AUDIO_CHANNELS as f64));
    options.set_output_channel_count(&output_channels.into());
    options.set_processor_options(Some(&processor_options));
    let node = AudioWorkletNode::new_with_options(&context, WORKLET_NAME, &options)?;
    let source = context.create_media_stream_source(&stream)?;
    source.connect_with_audio_node(&node)?;
    node.connect_with_audio_node(&context.destination())?;

    let port = node.port()?;
    let transport = inner.borrow().transport.clone();
    let message_handler = Closure::wrap(Box::new(move |event: MessageEvent| {
        if let Some(json) = event.data().as_string() {
            transport.borrow_mut().receive(generation, &json);
        } else {
            transport
                .borrow_mut()
                .fail("worklet emitted a non-string event".to_owned());
        }
    }) as Box<dyn FnMut(_)>);
    port.set_onmessage(Some(message_handler.as_ref().unchecked_ref()));
    port.start();

    let weak = Rc::downgrade(&inner);
    let processor_error_handler = Closure::wrap(Box::new(move |_event: WebEvent| {
        if let Some(inner) = weak.upgrade() {
            let inner = inner.borrow();
            if inner.generation == generation {
                inner
                    .transport
                    .borrow_mut()
                    .fail("AudioWorklet processor terminated".to_owned());
            }
        }
    }) as Box<dyn FnMut(_)>);
    node.set_onprocessorerror(Some(processor_error_handler.as_ref().unchecked_ref()));

    let weak = Rc::downgrade(&inner);
    let context_for_state = context.clone();
    let context_state_handler = Closure::wrap(Box::new(move |_event: WebEvent| {
        if let Some(inner) = weak.upgrade() {
            let mut inner = inner.borrow_mut();
            if inner.generation != generation {
                return;
            }
            let state = match context_for_state.state() {
                AudioContextState::Running => BackendDriverState::Running,
                AudioContextState::Suspended => BackendDriverState::Suspended,
                AudioContextState::Closed => BackendDriverState::Stopped,
                _ => BackendDriverState::Failed,
            };
            if state == BackendDriverState::Running {
                inner.startup_started = None;
            }
            inner.transport.borrow_mut().driver_state = state;
        }
    }) as Box<dyn FnMut(_)>);
    context.set_onstatechange(Some(context_state_handler.as_ref().unchecked_ref()));

    let mut track_ended_handlers = Vec::new();
    for value in stream.get_tracks().iter() {
        let track = value.dyn_into::<MediaStreamTrack>()?;
        let weak = Rc::downgrade(&inner);
        let handler = Closure::wrap(Box::new(move |_event: WebEvent| {
            if let Some(inner) = weak.upgrade() {
                let inner = inner.borrow();
                if inner.generation == generation {
                    inner
                        .transport
                        .borrow_mut()
                        .fail("microphone media track ended".to_owned());
                }
            }
        }) as Box<dyn FnMut(_)>);
        track.set_onended(Some(handler.as_ref().unchecked_ref()));
        track_ended_handlers.push(handler);
    }

    if inner.borrow().generation != generation {
        stop_stream(&stream);
        return Ok(());
    }
    {
        let mut inner = inner.borrow_mut();
        inner.stream = Some(stream);
        inner.source = Some(source);
        inner.node = Some(node);
        inner.message_handler = Some(message_handler);
        inner.processor_error_handler = Some(processor_error_handler);
        inner.context_state_handler = Some(context_state_handler);
        inner.track_ended_handlers = track_ended_handlers;
        inner
            .transport
            .borrow_mut()
            .attach(port, generation)
            .map_err(|error| {
                JsValue::from_str(&format!("could not initialize worklet protocol: {error}"))
            })?;
        let state = if context.state() == AudioContextState::Running {
            inner.startup_started = None;
            BackendDriverState::Running
        } else {
            BackendDriverState::Suspended
        };
        inner.transport.borrow_mut().driver_state = state;
    }
    let weak = Rc::downgrade(&inner);
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(error) = JsFuture::from(resume).await {
            if let Some(inner) = weak.upgrade() {
                let inner = inner.borrow();
                if inner.generation == generation {
                    inner.transport.borrow_mut().fail(js_error_message(&error));
                }
            }
        }
    });
    Ok(())
}

fn microphone_constraints() -> MediaStreamConstraints {
    let raw = Object::new();
    let _ = Reflect::set(&raw, &"echoCancellation".into(), &JsValue::FALSE);
    let _ = Reflect::set(&raw, &"noiseSuppression".into(), &JsValue::FALSE);
    let _ = Reflect::set(&raw, &"autoGainControl".into(), &JsValue::FALSE);
    let constraints = MediaStreamConstraints::new();
    constraints.set_audio(&raw.into());
    constraints
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
        .unwrap_or_else(|| format!("browser audio startup failed: {error:?}"))
}

fn supports_hosted_audio(window: &web_sys::Window) -> bool {
    window.is_secure_context()
        && window
            .location()
            .protocol()
            .is_ok_and(|protocol| protocol != "file:")
}

fn publish_track_settings(stream: &MediaStream) {
    let Some(element) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id("runtime_status"))
    else {
        return;
    };
    let Some(value) = stream.get_audio_tracks().iter().next() else {
        return;
    };
    let Ok(track) = value.dyn_into::<MediaStreamTrack>() else {
        return;
    };
    let settings = track.get_settings();
    if let Some(channels) = settings.get_channel_count() {
        let _ = element.set_attribute("data-capture-channels", &channels.to_string());
    }
    if let Some(enabled) = settings.get_echo_cancellation() {
        let _ = element.set_attribute("data-echo-cancellation", &enabled.to_string());
    }
    if let Some(enabled) = settings.get_noise_suppression() {
        let _ = element.set_attribute("data-noise-suppression", &enabled.to_string());
    }
    if let Some(enabled) = settings.get_auto_gain_control() {
        let _ = element.set_attribute("data-auto-gain-control", &enabled.to_string());
    }
}

fn stop_stream(stream: &MediaStream) {
    for value in stream.get_tracks().iter() {
        if let Ok(track) = value.dyn_into::<MediaStreamTrack>() {
            track.stop();
        }
    }
}

fn shutdown_graph(inner: &mut BrowserControllerInner) {
    inner.startup_started = None;
    if let Some(stream) = inner.stream.take() {
        for value in stream.get_tracks().iter() {
            if let Ok(track) = value.dyn_into::<MediaStreamTrack>() {
                track.set_onended(None);
            }
        }
        stop_stream(&stream);
    }
    if let Some(source) = inner.source.take() {
        let _ = source.disconnect();
    }
    if let Some(node) = inner.node.take() {
        node.set_onprocessorerror(None);
        if let Ok(port) = node.port() {
            port.set_onmessage(None);
            port.close();
        }
        let _ = node.disconnect();
    }
    if let Some(context) = inner.context.take() {
        context.set_onstatechange(None);
        let _ = context.close();
    }
    inner.message_handler = None;
    inner.processor_error_handler = None;
    inner.context_state_handler = None;
    inner.track_ended_handlers.clear();
}

fn shutdown_inner(inner: &mut BrowserControllerInner) {
    shutdown_graph(inner);
    let mut transport = inner.transport.borrow_mut();
    transport.port = None;
    if !matches!(
        transport.driver_state,
        BackendDriverState::Denied | BackendDriverState::Unsupported | BackendDriverState::Failed
    ) {
        transport.driver_state = BackendDriverState::Stopped;
    }
}

struct WaveformAssembly {
    revision: u64,
    channels: Vec<Vec<f32>>,
    next_channel: usize,
    next_offset: usize,
    complete: bool,
    in_flight: bool,
}

pub struct WebAudioBackend {
    transport: Rc<RefCell<Transport>>,
    snapshot: BackendSnapshot,
    next_track_id: u64,
    next_loop_id: u64,
    last_poll: Instant,
    waveform_revisions: BTreeMap<BackendLoopId, u64>,
    waveforms: BTreeMap<BackendLoopId, WaveformAssembly>,
}

impl WebAudioBackend {
    pub fn new() -> (Self, Rc<RefCell<Transport>>) {
        let transport = Rc::new(RefCell::new(Transport::new()));
        (
            Self {
                transport: transport.clone(),
                snapshot: BackendSnapshot {
                    status: BackendStatus {
                        driver_state: BackendDriverState::AwaitingGesture,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                next_track_id: 1,
                next_loop_id: 1,
                last_poll: Instant::now(),
                waveform_revisions: BTreeMap::new(),
                waveforms: BTreeMap::new(),
            },
            transport,
        )
    }

    fn submit(&mut self, command: Command) -> Result<()> {
        self.transport.borrow_mut().journal(command)
    }

    fn request_waveform_chunk(&mut self, loop_id: BackendLoopId) -> Result<()> {
        let Some(assembly) = self.waveforms.get_mut(&loop_id) else {
            return Ok(());
        };
        if assembly.complete || assembly.in_flight {
            return Ok(());
        }
        self.transport
            .borrow_mut()
            .ephemeral(Command::RequestWaveform {
                loop_id: loop_id.raw(),
                revision: assembly.revision,
                channel: assembly.next_channel,
                offset: assembly.next_offset,
                max_samples: WAVEFORM_CHUNK_SAMPLES,
            })?;
        assembly.in_flight = true;
        Ok(())
    }

    fn apply_waveform_chunk(&mut self, chunk: WaveformChunk) -> Result<()> {
        let loop_id = BackendLoopId::from_raw(chunk.loop_id);
        let Some(assembly) = self.waveforms.get_mut(&loop_id) else {
            return Ok(());
        };
        if assembly.revision != chunk.revision
            || assembly.next_channel != chunk.channel
            || assembly.next_offset != chunk.offset
        {
            return Ok(());
        }
        assembly.in_flight = false;
        if assembly.channels.len() < chunk.channel_count {
            assembly.channels.resize_with(chunk.channel_count, Vec::new);
        }
        if let Some(channel) = assembly.channels.get_mut(chunk.channel) {
            channel.extend_from_slice(&chunk.samples);
        }
        if chunk.final_chunk {
            assembly.next_channel += 1;
            assembly.next_offset = 0;
            assembly.complete = assembly.next_channel >= chunk.channel_count;
        } else {
            assembly.next_offset = chunk.offset.saturating_add(chunk.samples.len());
        }
        self.request_waveform_chunk(loop_id)
    }

    fn apply_wire_snapshot(&mut self, wire: WireSnapshot) {
        let state = self.transport.borrow().driver_state;
        self.snapshot.status = BackendStatus {
            sample_rate: wire.sample_rate,
            buffer_size: wire.quantum,
            callback_count: wire.callback_count,
            processed_frames: wire.processed_frames,
            input_peak: wire.input_peak,
            output_peak: wire.output_peak,
            xruns: wire.xruns,
            callback_budget_overruns: wire.callback_budget_overruns,
            render_discontinuities: wire.render_discontinuities,
            memory_growths: wire.memory_growths,
            command_overflows: wire
                .command_overflows
                .saturating_add(self.transport.borrow().overflows),
            storage_low_channels: wire.storage_low_channels,
            storage_exhaustions: wire.storage_exhaustions,
            driver_state: state,
            ..Default::default()
        };
        self.snapshot.tracks.extend(
            wire.tracks
                .into_iter()
                .map(|track| {
                    (
                        BackendTrackId::from_raw(track.id),
                        BackendTrackState {
                            audio_channels: track.audio_channels,
                            midi: track.midi,
                            output_gain_db: track.output_gain_db,
                            output_balance: track.output_balance,
                            output_muted: track.output_muted,
                            input_gain_db: track.input_gain_db,
                            input_balance: track.input_balance,
                            input_monitoring: track.input_monitoring,
                            input_peaks: track.input_peaks,
                            output_peaks: track.output_peaks,
                            ..Default::default()
                        },
                    )
                })
                .collect::<BTreeMap<_, _>>(),
        );
        self.snapshot.loops.extend(
            wire.loops
                .into_iter()
                .map(|loop_| {
                    (
                        BackendLoopId::from_raw(loop_.id),
                        BackendLoopState {
                            mode: from_wire_loop_mode(loop_.mode),
                            length: loop_.length,
                            position: loop_.position,
                            next_mode: loop_.next_mode.map(from_wire_loop_mode),
                            next_transition_delay: loop_.next_transition_delay,
                            stereo: loop_.stereo,
                            gain: loop_.gain,
                            audio_peaks: loop_.audio_peaks,
                            midi_activity: loop_.midi_activity,
                        },
                    )
                })
                .collect::<BTreeMap<_, _>>(),
        );
    }
}

impl Backend for WebAudioBackend {
    fn create_loop(&mut self) -> Result<BackendLoopId> {
        Err(anyhow!("standalone browser loops are unsupported"))
    }

    fn create_direct_track(&mut self, request: DirectTrackRequest) -> Result<BackendTrackCreation> {
        let track_id = BackendTrackId::from_raw(self.next_track_id);
        let loops: Vec<_> = (0..request.initial_loops)
            .map(|offset| BackendLoopId::from_raw(self.next_loop_id + offset as u64))
            .collect();
        self.submit(Command::CreateTrack {
            expected_track_id: track_id.raw(),
            expected_loop_ids: loops.iter().map(|id| id.raw()).collect(),
            port_name_base: request.port_name_base,
            audio_channels: request.audio_channels,
            midi: request.midi,
        })?;
        self.next_track_id = self.next_track_id.saturating_add(1);
        self.next_loop_id = self.next_loop_id.saturating_add(loops.len() as u64);
        self.snapshot.tracks.insert(
            track_id,
            BackendTrackState {
                audio_channels: request.audio_channels,
                midi: request.midi,
                input_peaks: vec![-200.0; request.audio_channels as usize],
                output_peaks: vec![-200.0; request.audio_channels as usize],
                ..Default::default()
            },
        );
        for loop_id in &loops {
            self.snapshot.loops.insert(
                *loop_id,
                BackendLoopState {
                    mode: BackendLoopMode::Stopped,
                    stereo: request.audio_channels == 2,
                    gain: 1.0,
                    audio_peaks: vec![-200.0; request.audio_channels as usize],
                    ..Default::default()
                },
            );
        }
        Ok(BackendTrackCreation { track_id, loops })
    }

    fn add_loop_to_track(&mut self, track_id: BackendTrackId) -> Result<BackendLoopId> {
        if !self.snapshot.tracks.contains_key(&track_id) {
            return Err(anyhow!("unknown browser backend track {track_id:?}"));
        }
        let loop_id = BackendLoopId::from_raw(self.next_loop_id);
        self.submit(Command::AddLoop {
            track_id: track_id.raw(),
            expected_loop_id: loop_id.raw(),
        })?;
        self.next_loop_id = self.next_loop_id.saturating_add(1);
        let track = &self.snapshot.tracks[&track_id];
        self.snapshot.loops.insert(
            loop_id,
            BackendLoopState {
                mode: BackendLoopMode::Stopped,
                stereo: track.audio_channels == 2,
                gain: 1.0,
                audio_peaks: vec![-200.0; track.audio_channels as usize],
                ..Default::default()
            },
        );
        Ok(loop_id)
    }

    fn set_track_control(
        &mut self,
        track_id: BackendTrackId,
        control: BackendTrackControl,
    ) -> Result<()> {
        if !self.snapshot.tracks.contains_key(&track_id) {
            return Err(anyhow!("unknown browser backend track {track_id:?}"));
        }
        self.submit(Command::SetTrackControl {
            track_id: track_id.raw(),
            control: to_wire_track_control(control),
        })?;
        let track = self
            .snapshot
            .tracks
            .get_mut(&track_id)
            .expect("track checked");
        match control {
            BackendTrackControl::OutputGainDb(value) => track.output_gain_db = value,
            BackendTrackControl::OutputBalance(value) => track.output_balance = value,
            BackendTrackControl::OutputMute(value) => track.output_muted = value,
            BackendTrackControl::InputGainDb(value) => track.input_gain_db = value,
            BackendTrackControl::InputBalance(value) => track.input_balance = value,
            BackendTrackControl::InputMonitoring(value) => track.input_monitoring = value,
        }
        Ok(())
    }

    fn set_loop_gain(&mut self, loop_id: BackendLoopId, gain: f32) -> Result<()> {
        if !self.snapshot.loops.contains_key(&loop_id) {
            return Err(anyhow!("unknown browser backend loop {loop_id:?}"));
        }
        self.submit(Command::SetLoopGain {
            loop_id: loop_id.raw(),
            gain,
        })?;
        self.snapshot
            .loops
            .get_mut(&loop_id)
            .expect("loop checked")
            .gain = gain;
        Ok(())
    }

    fn loop_audio_data(&mut self, loop_id: BackendLoopId) -> Result<Option<Vec<Arc<[f32]>>>> {
        if !self.snapshot.loops.contains_key(&loop_id) {
            return Err(anyhow!("unknown browser backend loop {loop_id:?}"));
        }
        if let Some(assembly) = self.waveforms.get(&loop_id) {
            if assembly.complete {
                return Ok(Some(
                    assembly
                        .channels
                        .iter()
                        .map(|channel| Arc::from(channel.clone()))
                        .collect(),
                ));
            }
            return Ok(None);
        }
        let revision = self
            .waveform_revisions
            .entry(loop_id)
            .and_modify(|revision| *revision = revision.saturating_add(1))
            .or_insert(1);
        self.waveforms.insert(
            loop_id,
            WaveformAssembly {
                revision: *revision,
                channels: Vec::new(),
                next_channel: 0,
                next_offset: 0,
                complete: false,
                in_flight: false,
            },
        );
        self.request_waveform_chunk(loop_id)?;
        Ok(None)
    }

    fn set_loop_sync_source(
        &mut self,
        loop_id: BackendLoopId,
        source: Option<BackendLoopId>,
    ) -> Result<()> {
        self.submit(Command::SetLoopSyncSource {
            loop_id: loop_id.raw(),
            source: source.map(BackendLoopId::raw),
        })
    }

    fn transition_loop(
        &mut self,
        loop_id: BackendLoopId,
        mode: BackendLoopMode,
        cycles_delay: Option<u32>,
    ) -> Result<()> {
        self.submit(Command::TransitionLoop {
            loop_id: loop_id.raw(),
            mode: to_wire_loop_mode(mode),
            cycles_delay,
        })?;
        self.waveforms.remove(&loop_id);
        Ok(())
    }

    fn clear_loop(&mut self, loop_id: BackendLoopId) -> Result<()> {
        self.submit(Command::ClearLoop {
            loop_id: loop_id.raw(),
        })?;
        self.waveforms.remove(&loop_id);
        Ok(())
    }

    fn advance(&mut self, _elapsed: Duration) {}

    fn poll(&mut self) -> Result<BackendSnapshot> {
        let state = self.transport.borrow().driver_state;
        self.snapshot.status.driver_state = state;
        self.snapshot.status.command_overflows = self.transport.borrow().overflows;
        if matches!(
            state,
            BackendDriverState::Running | BackendDriverState::Suspended
        ) && self.last_poll.elapsed() >= Duration::from_millis(u64::from(STATUS_INTERVAL_MS))
            && self.transport.borrow().in_flight < COMMAND_CAPACITY / 2
        {
            self.transport.borrow_mut().ephemeral(Command::Poll)?;
            self.last_poll = Instant::now();
        }
        let events: Vec<_> = self.transport.borrow_mut().inbound.drain(..).collect();
        for envelope in events {
            match envelope.event {
                Event::Ack | Event::Stopped => {}
                Event::Error { message } => return Err(anyhow!(message)),
                Event::Snapshot(snapshot) => self.apply_wire_snapshot(snapshot),
                Event::Waveform(chunk) => self.apply_waveform_chunk(chunk)?,
            }
        }
        if let Some(error) = self.transport.borrow_mut().error.take() {
            return Err(anyhow!(error));
        }
        Ok(self.snapshot.clone())
    }

    fn wait_idle(&mut self) {}
}

fn to_wire_track_control(control: BackendTrackControl) -> WireTrackControl {
    match control {
        BackendTrackControl::OutputGainDb(value) => WireTrackControl::OutputGainDb(value),
        BackendTrackControl::OutputBalance(value) => WireTrackControl::OutputBalance(value),
        BackendTrackControl::OutputMute(value) => WireTrackControl::OutputMute(value),
        BackendTrackControl::InputGainDb(value) => WireTrackControl::InputGainDb(value),
        BackendTrackControl::InputBalance(value) => WireTrackControl::InputBalance(value),
        BackendTrackControl::InputMonitoring(value) => WireTrackControl::InputMonitoring(value),
    }
}

fn to_wire_loop_mode(mode: BackendLoopMode) -> WireLoopMode {
    match mode {
        BackendLoopMode::Unknown => WireLoopMode::Unknown,
        BackendLoopMode::Stopped => WireLoopMode::Stopped,
        BackendLoopMode::Playing => WireLoopMode::Playing,
        BackendLoopMode::Recording => WireLoopMode::Recording,
        BackendLoopMode::Replacing => WireLoopMode::Replacing,
        BackendLoopMode::PlayingDryThroughWet => WireLoopMode::PlayingDryThroughWet,
        BackendLoopMode::RecordingDryIntoWet => WireLoopMode::RecordingDryIntoWet,
    }
}

fn from_wire_loop_mode(mode: WireLoopMode) -> BackendLoopMode {
    match mode {
        WireLoopMode::Unknown => BackendLoopMode::Unknown,
        WireLoopMode::Stopped => BackendLoopMode::Stopped,
        WireLoopMode::Playing => BackendLoopMode::Playing,
        WireLoopMode::Recording => BackendLoopMode::Recording,
        WireLoopMode::Replacing => BackendLoopMode::Replacing,
        WireLoopMode::PlayingDryThroughWet => BackendLoopMode::PlayingDryThroughWet,
        WireLoopMode::RecordingDryIntoWet => BackendLoopMode::RecordingDryIntoWet,
    }
}
