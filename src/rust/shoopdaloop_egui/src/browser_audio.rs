use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use crate::browser_midi::{BrowserMidiHub, TrackMidiInput};
use anyhow::{anyhow, Result};
use js_sys::{Array, Object, Reflect, WebAssembly};
use shoop_audio_protocol::{
    Command, CommandEnvelope, Event, EventEnvelope, WaveformChunk, WireGrabRequest, WireHostPort,
    WireLoopMode, WireMidiEvent, WirePortDataType, WirePortDirection, WirePortRole, WireSnapshot,
    WireTrackControl, WireTrackFxControl, WireTrackTopology, COMMAND_CAPACITY,
    MAX_DEVICE_AUDIO_CHANNELS, MIDI_BATCH_CAPACITY, PROTOCOL_VERSION, SESSION_TRANSFER_CHUNK_BYTES,
    SESSION_TRANSFER_MAX_BYTES, STATUS_INTERVAL_MS, WAVEFORM_CHUNK_SAMPLES,
};
use shoop_backend::{
    default_tiny_synth_fx_state, encode_tiny_synth_fx_state, tiny_synth_fx_descriptor, Backend,
    BackendConfirmedLink, BackendConnectionFailure, BackendDriverState, BackendGrabRequest,
    BackendHostPortDescriptor, BackendLoopId, BackendLoopMode, BackendLoopState, BackendMidiEvent,
    BackendPortDataType, BackendPortDescriptor, BackendPortDirection, BackendPortId,
    BackendPortRole, BackendSessionData, BackendSessionReplacement, BackendSnapshot, BackendStatus,
    BackendTrackControl, BackendTrackCreation, BackendTrackFxControl, BackendTrackId,
    BackendTrackState, BackendTrackTopology, DirectTrackRequest, TinySynthFxControl,
    TrackProcessorTypeId, TrackRequest,
};
use shoop_egui::{
    AudioDriverConfig, AudioDriverDescriptor, AudioDriverKind, AudioDriverRuntimeState,
    FxLifecycle, ResolvedAudioDriverConfig, TinySynthFxState, TrackFxState,
    TrackProcessorDescriptor, TrackProcessorEditorState,
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
const EMBEDDED_WORKLET_ASSETS: &str = "shoopEmbeddedAudioWorklet";
const MAX_QUANTUM: u32 = 2048;

#[derive(Clone, Copy, PartialEq, Eq)]
enum AudioInputMode {
    Microphone,
    OutputOnly,
}

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

    fn attach(
        &mut self,
        port: MessagePort,
        generation: u64,
        input_channels: u32,
        output_channels: u32,
    ) -> Result<()> {
        self.generation = generation;
        self.port = Some(port);
        self.inbound.clear();
        self.in_flight = 0;
        self.next_sequence = 1;
        self.send(Command::ConfigureDeviceChannels {
            input_channels,
            output_channels,
        })?;
        let journal = self.journal.clone();
        for command in journal
            .iter()
            .filter(|command| matches!(command, Command::ConfigureMidiEndpoints { .. }))
            .cloned()
        {
            self.send(command)?;
        }
        for command in journal
            .into_iter()
            .filter(|command| !matches!(command, Command::ConfigureMidiEndpoints { .. }))
        {
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
    input_mode: Option<AudioInputMode>,
}

pub struct BrowserAudioController {
    inner: Rc<RefCell<BrowserControllerInner>>,
    microphone_enable_handler: Closure<dyn FnMut(WebEvent)>,
    output_enable_handler: Closure<dyn FnMut(WebEvent)>,
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
            input_mode: None,
        }));
        let weak = Rc::downgrade(&inner);
        let microphone_enable_handler = Closure::wrap(Box::new(move |_event: WebEvent| {
            if let Some(inner) = weak.upgrade() {
                begin_enable(inner, AudioInputMode::Microphone);
            }
        }) as Box<dyn FnMut(_)>);
        microphone_enable_button()?
            .set_onclick(Some(microphone_enable_handler.as_ref().unchecked_ref()));

        let weak = Rc::downgrade(&inner);
        let output_enable_handler = Closure::wrap(Box::new(move |_event: WebEvent| {
            if let Some(inner) = weak.upgrade() {
                begin_enable(inner, AudioInputMode::OutputOnly);
            }
        }) as Box<dyn FnMut(_)>);
        output_enable_button()?.set_onclick(Some(output_enable_handler.as_ref().unchecked_ref()));

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
            microphone_enable_handler,
            output_enable_handler,
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

    pub fn audio_context(&self) -> Option<AudioContext> {
        self.inner.borrow().context.clone()
    }

    pub fn update_presentation(&self) {
        let (state, generation, owned_media_tracks, input_mode) = {
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
            (
                state,
                inner.generation,
                owned_media_tracks,
                inner.input_mode,
            )
        };
        if let Some(element) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id("runtime_status"))
        {
            let _ = element.set_attribute("data-audio-generation", &generation.to_string());
            let _ =
                element.set_attribute("data-owned-media-tracks", &owned_media_tracks.to_string());
        }
        let active = matches!(
            state,
            BackendDriverState::RequestingPermission
                | BackendDriverState::Starting
                | BackendDriverState::Running
                | BackendDriverState::Suspended
        );
        for (button, mode, enable_text, retry_text) in [
            (
                microphone_enable_button(),
                AudioInputMode::Microphone,
                "Enable microphone audio",
                "Retry microphone audio",
            ),
            (
                output_enable_button(),
                AudioInputMode::OutputOnly,
                "Enable output-only audio",
                "Retry output-only audio",
            ),
        ] {
            if let Ok(button) = button {
                button.set_hidden(active);
                button.set_text_content(Some(
                    if input_mode == Some(mode)
                        && matches!(
                            state,
                            BackendDriverState::Denied
                                | BackendDriverState::Failed
                                | BackendDriverState::Stopped
                        )
                    {
                        retry_text
                    } else {
                        enable_text
                    },
                ));
            }
        }
    }

    pub fn shutdown(&self) {
        shutdown_inner(&mut self.inner.borrow_mut());
    }
}

impl Drop for BrowserAudioController {
    fn drop(&mut self) {
        let _ = (&self.microphone_enable_handler, &self.output_enable_handler);
        if let Ok(button) = microphone_enable_button() {
            button.set_onclick(None);
        }
        if let Ok(button) = output_enable_button() {
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

fn audio_button(id: &str) -> Result<HtmlButtonElement> {
    web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(id))
        .ok_or_else(|| anyhow!("missing #{id} button"))?
        .dyn_into::<HtmlButtonElement>()
        .map_err(|_| anyhow!("#{id} is not a button"))
}

fn microphone_enable_button() -> Result<HtmlButtonElement> {
    audio_button("enable_audio")
}

fn output_enable_button() -> Result<HtmlButtonElement> {
    audio_button("enable_output_audio")
}

fn begin_enable(inner: Rc<RefCell<BrowserControllerInner>>, input_mode: AudioInputMode) {
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
    inner.borrow_mut().input_mode = Some(input_mode);
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
            let _ = context.close();
            inner
                .borrow()
                .transport
                .borrow_mut()
                .fail(format!("could not begin AudioContext resume: {error:?}"));
            return;
        }
    };
    let media = if input_mode == AudioInputMode::Microphone {
        let constraints = microphone_constraints();
        match window
            .navigator()
            .media_devices()
            .and_then(|devices| devices.get_user_media_with_constraints(&constraints))
        {
            Ok(media) => Some(media),
            Err(error) => {
                let _ = context.close();
                inner
                    .borrow()
                    .transport
                    .borrow_mut()
                    .fail(format!("getUserMedia is unavailable: {error:?}"));
                return;
            }
        }
    } else {
        None
    };

    let generation = {
        let mut inner = inner.borrow_mut();
        shutdown_inner(&mut inner);
        inner.generation = inner.generation.saturating_add(1);
        inner.startup_started = Some(Instant::now());
        inner.context = Some(context.clone());
        inner.input_mode = Some(input_mode);
        let mut transport = inner.transport.borrow_mut();
        transport.driver_state = if input_mode == AudioInputMode::Microphone {
            BackendDriverState::RequestingPermission
        } else {
            BackendDriverState::Starting
        };
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
    media: Option<js_sys::Promise>,
) -> std::result::Result<(), JsValue> {
    let stream = match media {
        Some(media) => {
            let stream = JsFuture::from(media).await?.dyn_into::<MediaStream>()?;
            publish_track_settings(&stream);
            Some(stream)
        }
        None => None,
    };
    if inner.borrow().generation != generation {
        if let Some(stream) = &stream {
            stop_stream(stream);
        }
        return Ok(());
    }
    {
        let mut inner = inner.borrow_mut();
        inner.startup_started = Some(Instant::now());
        inner.transport.borrow_mut().driver_state = BackendDriverState::Starting;
    }

    let module = load_worklet_module(&context).await?;

    let processor_options = Object::new();
    Reflect::set(&processor_options, &"wasmModule".into(), module.as_ref())?;
    Reflect::set(
        &processor_options,
        &"maxQuantum".into(),
        &JsValue::from_f64(MAX_QUANTUM as f64),
    )?;
    let options = AudioWorkletNodeOptions::new();
    options.set_number_of_inputs(if stream.is_some() { 1 } else { 0 });
    options.set_number_of_outputs(1);
    let output_channels = Array::new();
    output_channels.push(&JsValue::from_f64(MAX_DEVICE_AUDIO_CHANNELS as f64));
    options.set_output_channel_count(&output_channels.into());
    options.set_processor_options(Some(&processor_options));
    let node = AudioWorkletNode::new_with_options(&context, WORKLET_NAME, &options)?;
    let source = if let Some(stream) = &stream {
        let source = context.create_media_stream_source(stream)?;
        source.connect_with_audio_node(&node)?;
        Some(source)
    } else {
        None
    };
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
    for value in stream
        .as_ref()
        .map(MediaStream::get_tracks)
        .unwrap_or_default()
        .iter()
    {
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
        if let Some(stream) = &stream {
            stop_stream(stream);
        }
        return Ok(());
    }
    {
        let input_channels = stream
            .as_ref()
            .and_then(capture_channel_count)
            .unwrap_or(u32::from(stream.is_some()))
            .min(MAX_DEVICE_AUDIO_CHANNELS as u32);
        let output_channels = context
            .destination()
            .channel_count()
            .min(MAX_DEVICE_AUDIO_CHANNELS as u32);
        let mut inner = inner.borrow_mut();
        inner.stream = stream;
        inner.source = source;
        inner.node = Some(node);
        inner.message_handler = Some(message_handler);
        inner.processor_error_handler = Some(processor_error_handler);
        inner.context_state_handler = Some(context_state_handler);
        inner.track_ended_handlers = track_ended_handlers;
        inner
            .transport
            .borrow_mut()
            .attach(port, generation, input_channels, output_channels)
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

async fn load_worklet_module(
    context: &AudioContext,
) -> std::result::Result<WebAssembly::Module, JsValue> {
    let window =
        web_sys::window().ok_or_else(|| JsValue::from_str("browser window disappeared"))?;
    let embedded = Reflect::get(window.as_ref(), &EMBEDDED_WORKLET_ASSETS.into())?;
    if !embedded.is_null() && !embedded.is_undefined() {
        let module_url = Reflect::get(&embedded, &"moduleUrl".into())?
            .as_string()
            .ok_or_else(|| JsValue::from_str("embedded AudioWorklet module URL is invalid"))?;
        let wasm_bytes = Reflect::get(&embedded, &"wasmBytes".into())?;
        JsFuture::from(context.audio_worklet()?.add_module(&module_url)?).await?;
        return JsFuture::from(WebAssembly::compile(&wasm_bytes))
            .await?
            .dyn_into::<WebAssembly::Module>();
    }

    JsFuture::from(context.audio_worklet()?.add_module(WORKLET_SCRIPT_URL)?).await?;
    let response = JsFuture::from(window.fetch_with_str(WORKLET_WASM_URL))
        .await?
        .dyn_into::<Response>()?;
    if !response.ok() {
        return Err(JsValue::from_str(
            "could not fetch the AudioWorklet Wasm module",
        ));
    }
    let bytes = JsFuture::from(response.array_buffer()?).await?;
    JsFuture::from(WebAssembly::compile(&bytes))
        .await?
        .dyn_into::<WebAssembly::Module>()
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

fn capture_channel_count(stream: &MediaStream) -> Option<u32> {
    stream
        .get_audio_tracks()
        .iter()
        .next()
        .and_then(|value| value.dyn_into::<MediaStreamTrack>().ok())
        .and_then(|track| track.get_settings().get_channel_count())
        .and_then(|channels| u32::try_from(channels).ok())
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

struct SessionCaptureAssembly {
    generation: u64,
    total_bytes: Option<usize>,
    bytes: Vec<u8>,
    in_flight: bool,
}

struct SessionReplaceAssembly {
    generation: u64,
    session: BackendSessionData,
    bytes: Vec<u8>,
    next_offset: usize,
    commit_sent: bool,
    complete: bool,
}

pub struct WebAudioBackend {
    transport: Rc<RefCell<Transport>>,
    snapshot: BackendSnapshot,
    next_track_id: u64,
    next_loop_id: u64,
    next_port_id: u64,
    last_poll: Instant,
    waveform_revisions: BTreeMap<BackendLoopId, u64>,
    waveforms: BTreeMap<BackendLoopId, WaveformAssembly>,
    next_session_generation: u64,
    session_capture: Option<SessionCaptureAssembly>,
    session_replace: Option<SessionReplaceAssembly>,
    midi: BrowserMidiHub,
    midi_revision: u64,
}

impl WebAudioBackend {
    pub fn new(midi: BrowserMidiHub) -> (Self, Rc<RefCell<Transport>>) {
        let transport = Rc::new(RefCell::new(Transport::new()));
        (
            Self {
                transport: transport.clone(),
                snapshot: BackendSnapshot {
                    status: BackendStatus {
                        driver_state: BackendDriverState::AwaitingGesture,
                        ..Default::default()
                    },
                    audio_drivers: AudioDriverRuntimeState {
                        supported: false,
                        catalog: Arc::from([AudioDriverDescriptor {
                            kind: AudioDriverKind::WebAudio,
                            available: true,
                            ..Default::default()
                        }]),
                        active: Some(ResolvedAudioDriverConfig {
                            configured: AudioDriverConfig::WebAudio,
                            sample_rate: 0,
                            buffer_size: 0,
                            instance_name: "Web Audio".to_owned(),
                        }),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                next_track_id: 1,
                next_loop_id: 1,
                next_port_id: 1,
                last_poll: Instant::now(),
                waveform_revisions: BTreeMap::new(),
                waveforms: BTreeMap::new(),
                next_session_generation: 1,
                session_capture: None,
                session_replace: None,
                midi,
                midi_revision: u64::MAX,
            },
            transport,
        )
    }

    fn submit(&mut self, command: Command) -> Result<()> {
        self.transport.borrow_mut().journal(command)
    }

    fn sync_midi_endpoints(&mut self) -> Result<()> {
        let endpoint_snapshot = self.midi.endpoint_snapshot();
        if endpoint_snapshot.revision == self.midi_revision {
            return Ok(());
        }
        let endpoints = self.midi.endpoints();
        let wire_endpoints = endpoints
            .iter()
            .map(|endpoint| WireHostPort {
                id: endpoint.id.clone(),
                name: endpoint.name.clone(),
                data_type: WirePortDataType::Midi,
                direction: match endpoint.direction {
                    shoop_scripting::MidiEndpointDirection::Input => WirePortDirection::Input,
                    shoop_scripting::MidiEndpointDirection::Output => WirePortDirection::Output,
                },
            })
            .collect::<Vec<_>>();
        self.submit(Command::ConfigureMidiEndpoints {
            endpoints: wire_endpoints,
        })?;
        self.snapshot
            .connections
            .host_ports
            .retain(|_, host| host.data_type != BackendPortDataType::Midi);
        for endpoint in endpoints {
            self.snapshot.connections.host_ports.insert(
                endpoint.id.clone(),
                BackendHostPortDescriptor {
                    id: endpoint.id,
                    name: endpoint.name,
                    data_type: BackendPortDataType::Midi,
                    direction: match endpoint.direction {
                        shoop_scripting::MidiEndpointDirection::Input => {
                            BackendPortDirection::Input
                        }
                        shoop_scripting::MidiEndpointDirection::Output => {
                            BackendPortDirection::Output
                        }
                    },
                },
            );
        }
        self.snapshot.connections.revision = self.snapshot.connections.revision.wrapping_add(1);
        self.midi_revision = endpoint_snapshot.revision;
        Ok(())
    }

    fn pump_midi_input(&mut self, running: bool) -> Result<()> {
        let messages = self.midi.drain_track_messages(MIDI_BATCH_CAPACITY);
        if !running {
            return Ok(());
        }
        let mut batches: BTreeMap<String, Vec<WireMidiEvent>> = BTreeMap::new();
        for TrackMidiInput { endpoint_id, data } in messages {
            batches
                .entry(endpoint_id)
                .or_default()
                .push(WireMidiEvent { frame: 0, data });
        }
        for (host_port_id, events) in batches {
            self.transport
                .borrow_mut()
                .ephemeral(Command::PushMidiInput {
                    host_port_id,
                    events,
                })?;
        }
        Ok(())
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

    fn request_session_capture_chunk(&mut self) -> Result<()> {
        let Some(capture) = self.session_capture.as_mut() else {
            return Ok(());
        };
        let Some(total_bytes) = capture.total_bytes else {
            return Ok(());
        };
        if capture.in_flight || capture.bytes.len() >= total_bytes {
            return Ok(());
        }
        self.transport
            .borrow_mut()
            .ephemeral(Command::ReadSessionCapture {
                generation: capture.generation,
                offset: capture.bytes.len(),
                max_bytes: SESSION_TRANSFER_CHUNK_BYTES,
            })?;
        capture.in_flight = true;
        Ok(())
    }

    fn apply_session_capture_ready(&mut self, generation: u64, total_bytes: usize) -> Result<()> {
        let Some(capture) = self.session_capture.as_mut() else {
            return Ok(());
        };
        if capture.generation != generation || total_bytes > SESSION_TRANSFER_MAX_BYTES {
            return Err(anyhow!("invalid session capture metadata"));
        }
        capture.total_bytes = Some(total_bytes);
        capture.bytes.reserve(total_bytes);
        capture.in_flight = false;
        self.request_session_capture_chunk()
    }

    fn apply_session_capture_chunk(
        &mut self,
        generation: u64,
        offset: usize,
        total_bytes: usize,
        final_chunk: bool,
        bytes: Vec<u8>,
    ) -> Result<()> {
        let Some(capture) = self.session_capture.as_mut() else {
            return Ok(());
        };
        if capture.generation != generation
            || capture.total_bytes != Some(total_bytes)
            || capture.bytes.len() != offset
            || bytes.len() > SESSION_TRANSFER_CHUNK_BYTES
            || offset.saturating_add(bytes.len()) > total_bytes
            || final_chunk != (offset.saturating_add(bytes.len()) >= total_bytes)
        {
            return Err(anyhow!("invalid session capture chunk"));
        }
        capture.bytes.extend_from_slice(&bytes);
        capture.in_flight = false;
        self.request_session_capture_chunk()
    }

    fn pump_session_replace(&mut self) -> Result<()> {
        let Some(replace) = self.session_replace.as_mut() else {
            return Ok(());
        };
        if replace.complete {
            return Ok(());
        }
        while replace.next_offset < replace.bytes.len()
            && self.transport.borrow().in_flight < COMMAND_CAPACITY / 2
        {
            let end = replace
                .next_offset
                .saturating_add(SESSION_TRANSFER_CHUNK_BYTES)
                .min(replace.bytes.len());
            self.transport
                .borrow_mut()
                .ephemeral(Command::WriteSessionReplace {
                    generation: replace.generation,
                    offset: replace.next_offset,
                    bytes: replace.bytes[replace.next_offset..end].to_vec(),
                })?;
            replace.next_offset = end;
        }
        if replace.next_offset == replace.bytes.len()
            && !replace.commit_sent
            && self.transport.borrow().in_flight < COMMAND_CAPACITY
        {
            self.transport
                .borrow_mut()
                .ephemeral(Command::CommitSessionReplace {
                    generation: replace.generation,
                })?;
            replace.commit_sent = true;
        }
        Ok(())
    }

    fn apply_replaced_session(
        &mut self,
        session: &BackendSessionData,
        replacement: &BackendSessionReplacement,
    ) {
        self.snapshot.tracks.clear();
        self.snapshot.loops.clear();
        self.snapshot.connections.application_ports.clear();
        self.snapshot.connections.confirmed_links.clear();
        self.waveforms.clear();
        for source_track in &session.tracks {
            let Some(created) = replacement.tracks.get(&source_track.source_id) else {
                continue;
            };
            self.snapshot
                .tracks
                .insert(created.track_id, source_track.state.clone());
            for (source_loop, loop_id) in source_track.loops.iter().zip(&created.loops) {
                self.snapshot.loops.insert(
                    *loop_id,
                    BackendLoopState {
                        mode: BackendLoopMode::Stopped,
                        length: source_loop.length,
                        stereo: source_track.state.audio_channels == 2,
                        gain: source_loop.gain,
                        balance: source_loop.balance,
                        audio_peaks: vec![-200.0; source_track.state.audio_channels as usize],
                        ..Default::default()
                    },
                );
            }
            for (source_port, created_port) in source_track.ports.iter().zip(&created.ports) {
                self.snapshot
                    .connections
                    .application_ports
                    .insert(created_port.id, created_port.clone());
                debug_assert_eq!(
                    replacement.ports.get(&source_port.source_id),
                    Some(&created_port.id)
                );
            }
        }
        self.next_track_id = replacement
            .tracks
            .values()
            .map(|created| created.track_id.raw())
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        self.next_loop_id = replacement
            .loops
            .values()
            .map(|id| id.raw())
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        self.next_port_id = replacement
            .ports
            .values()
            .map(|id| id.raw())
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        self.snapshot.connections.revision = self.snapshot.connections.revision.wrapping_add(1);
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
        if let Some(active) = self.snapshot.audio_drivers.active.as_mut() {
            active.sample_rate = wire.sample_rate;
            active.buffer_size = wire.quantum;
        }
        self.snapshot.connections.available = true;
        self.snapshot.connections.application_ports = wire
            .application_ports
            .into_iter()
            .map(|port| {
                let id = BackendPortId::from_raw(port.id);
                (
                    id,
                    BackendPortDescriptor {
                        id,
                        name: port.name,
                        data_type: from_wire_data_type(port.data_type),
                        direction: from_wire_direction(port.direction),
                        role: from_wire_role(port.role),
                    },
                )
            })
            .collect();
        self.snapshot.connections.host_ports = wire
            .host_ports
            .into_iter()
            .map(|port| {
                (
                    port.id.clone(),
                    BackendHostPortDescriptor {
                        id: port.id,
                        name: port.name,
                        data_type: from_wire_data_type(port.data_type),
                        direction: from_wire_direction(port.direction),
                    },
                )
            })
            .collect();
        self.snapshot.connections.confirmed_links = wire
            .confirmed_links
            .into_iter()
            .map(|link| BackendConfirmedLink {
                application_port_id: BackendPortId::from_raw(link.application_port_id),
                host_port_id: link.host_port_id,
            })
            .collect();
        self.snapshot.connections.revision = self.snapshot.connections.revision.wrapping_add(1);
        self.snapshot.tracks.extend(
            wire.tracks
                .into_iter()
                .map(|track| {
                    (
                        BackendTrackId::from_raw(track.id),
                        BackendTrackState {
                            topology: match track.topology {
                                WireTrackTopology::Direct {
                                    audio_channels,
                                    midi,
                                } => BackendTrackTopology::Direct {
                                    audio_channels,
                                    midi,
                                },
                                WireTrackTopology::TinySynthFx { audio_channels } => {
                                    BackendTrackTopology::DryWetProcessor {
                                        processor_type: TrackProcessorTypeId::TINY_SYNTH_FX
                                            .to_owned(),
                                        dry_audio_channels: audio_channels,
                                        wet_audio_channels: audio_channels,
                                        dry_midi: true,
                                    }
                                }
                            },
                            fx: track.fx.map(|fx| TrackFxState {
                                processor_type: TrackProcessorTypeId::new(
                                    TrackProcessorTypeId::TINY_SYNTH_FX,
                                ),
                                active: fx.active,
                                visible: fx.visible,
                                lifecycle: FxLifecycle::Running,
                                generation: 0,
                                crash_summary: None,
                                logs: Arc::from([]),
                                editor: Some(TrackProcessorEditorState::TinySynthFx(
                                    TinySynthFxState {
                                        selected_preset_id: fx.tiny.selected_preset_id,
                                        master_gain_db: fx.tiny.master_gain_db,
                                        reverb_enabled: fx.tiny.reverb_enabled,
                                        reverb_amount: fx.tiny.reverb_amount,
                                        distortion_enabled: fx.tiny.distortion_enabled,
                                        distortion_drive: fx.tiny.distortion_drive,
                                    },
                                )),
                            }),
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
                            balance: loop_.balance,
                            audio_peaks: loop_.audio_peaks,
                            midi_activity: loop_.midi_activity,
                        },
                    )
                })
                .collect::<BTreeMap<_, _>>(),
        );
    }
}

fn browser_replacement_mapping(session: &BackendSessionData) -> BackendSessionReplacement {
    let mut replacement = BackendSessionReplacement::default();
    let mut next_track_id = 1_u64;
    let mut next_loop_id = 1_u64;
    let mut next_port_id = 1_u64;
    for source_track in &session.tracks {
        let track_id = BackendTrackId::from_raw(next_track_id);
        next_track_id = next_track_id.saturating_add(1);
        let loops = source_track
            .loops
            .iter()
            .map(|source_loop| {
                let id = BackendLoopId::from_raw(next_loop_id);
                next_loop_id = next_loop_id.saturating_add(1);
                replacement.loops.insert(source_loop.source_id, id);
                id
            })
            .collect::<Vec<_>>();
        let ports = source_track
            .ports
            .iter()
            .map(|source_port| {
                let mut descriptor = source_port.descriptor.clone();
                descriptor.id = BackendPortId::from_raw(next_port_id);
                next_port_id = next_port_id.saturating_add(1);
                replacement
                    .ports
                    .insert(source_port.source_id, descriptor.id);
                descriptor
            })
            .collect::<Vec<_>>();
        replacement.tracks.insert(
            source_track.source_id,
            BackendTrackCreation {
                track_id,
                loops,
                ports,
            },
        );
    }
    replacement
}

fn browser_port_descriptors(
    base: &str,
    audio_channels: u32,
    midi: bool,
    next_port_id: &mut u64,
) -> Vec<BackendPortDescriptor> {
    let mut ports = Vec::with_capacity(audio_channels as usize * 2 + 2);
    let mut add = |name: String, data_type, direction, role| {
        let id = BackendPortId::from_raw(*next_port_id);
        *next_port_id = next_port_id.saturating_add(1);
        ports.push(BackendPortDescriptor {
            id,
            name,
            data_type,
            direction,
            role,
        });
    };
    for index in 0..audio_channels {
        let suffix = if audio_channels == 1 {
            String::new()
        } else {
            format!("_{}", index + 1)
        };
        add(
            format!("{base}_direct_in{suffix}"),
            BackendPortDataType::Audio,
            BackendPortDirection::Input,
            BackendPortRole::AudioInput,
        );
        add(
            format!("{base}_direct_out{suffix}"),
            BackendPortDataType::Audio,
            BackendPortDirection::Output,
            BackendPortRole::AudioOutput,
        );
    }
    if midi {
        add(
            format!("{base}_direct_midi_in"),
            BackendPortDataType::Midi,
            BackendPortDirection::Input,
            BackendPortRole::MidiInput,
        );
        add(
            format!("{base}_direct_midi_out"),
            BackendPortDataType::Midi,
            BackendPortDirection::Output,
            BackendPortRole::MidiOutput,
        );
    }
    ports
}

fn browser_tiny_port_descriptors(
    base: &str,
    audio_channels: u32,
    next_port_id: &mut u64,
) -> Vec<BackendPortDescriptor> {
    let mut ports = Vec::with_capacity(audio_channels as usize * 2 + 1);
    let mut add = |name: String,
                   data_type: BackendPortDataType,
                   direction: BackendPortDirection,
                   role: BackendPortRole| {
        let id = BackendPortId::from_raw(*next_port_id);
        *next_port_id = next_port_id.saturating_add(1);
        ports.push(BackendPortDescriptor {
            id,
            name,
            data_type,
            direction,
            role,
        });
    };
    for index in 0..audio_channels {
        add(
            format!("{base}_audio_dry_in_{}", index + 1),
            BackendPortDataType::Audio,
            BackendPortDirection::Input,
            BackendPortRole::AudioInput,
        );
        add(
            format!("{base}_audio_wet_out_{}", index + 1),
            BackendPortDataType::Audio,
            BackendPortDirection::Output,
            BackendPortRole::AudioOutput,
        );
    }
    add(
        format!("{base}_dry_midi_in"),
        BackendPortDataType::Midi,
        BackendPortDirection::Input,
        BackendPortRole::MidiInput,
    );
    ports
}

impl Backend for WebAudioBackend {
    fn track_processor_catalog(&mut self) -> Result<Arc<[TrackProcessorDescriptor]>> {
        Ok(vec![tiny_synth_fx_descriptor()].into())
    }

    fn create_track(&mut self, request: TrackRequest) -> Result<BackendTrackCreation> {
        match &request.topology {
            BackendTrackTopology::Direct {
                audio_channels,
                midi,
            } => self.create_direct_track(DirectTrackRequest {
                port_name_base: request.port_name_base,
                audio_channels: *audio_channels,
                midi: *midi,
                initial_loops: request.initial_loops,
            }),
            BackendTrackTopology::DryWetProcessor {
                processor_type,
                dry_audio_channels,
                wet_audio_channels,
                dry_midi,
            } if processor_type == TrackProcessorTypeId::TINY_SYNTH_FX
                && dry_audio_channels == wet_audio_channels
                && *dry_midi =>
            {
                let track_id = BackendTrackId::from_raw(self.next_track_id);
                let ports = browser_tiny_port_descriptors(
                    &request.port_name_base,
                    *dry_audio_channels,
                    &mut self.next_port_id,
                );
                let loops: Vec<_> = (0..request.initial_loops)
                    .map(|offset| BackendLoopId::from_raw(self.next_loop_id + offset as u64))
                    .collect();
                self.submit(Command::CreateTrack {
                    expected_track_id: track_id.raw(),
                    expected_loop_ids: loops.iter().map(|id| id.raw()).collect(),
                    port_name_base: request.port_name_base,
                    topology: WireTrackTopology::TinySynthFx {
                        audio_channels: *dry_audio_channels,
                    },
                })?;
                self.next_track_id = self.next_track_id.saturating_add(1);
                self.next_loop_id = self.next_loop_id.saturating_add(loops.len() as u64);
                self.snapshot.tracks.insert(
                    track_id,
                    BackendTrackState {
                        topology: request.topology.clone(),
                        fx: Some(default_tiny_synth_fx_state()),
                        audio_channels: *wet_audio_channels,
                        midi: true,
                        input_peaks: vec![-200.0; *dry_audio_channels as usize],
                        output_peaks: vec![-200.0; *wet_audio_channels as usize],
                        ..Default::default()
                    },
                );
                for port in &ports {
                    self.snapshot
                        .connections
                        .application_ports
                        .insert(port.id, port.clone());
                }
                self.snapshot.connections.revision =
                    self.snapshot.connections.revision.wrapping_add(1);
                for loop_id in &loops {
                    self.snapshot.loops.insert(
                        *loop_id,
                        BackendLoopState {
                            mode: BackendLoopMode::Stopped,
                            stereo: *wet_audio_channels == 2,
                            gain: 1.0,
                            audio_peaks: vec![
                                -200.0;
                                dry_audio_channels.saturating_add(*wet_audio_channels)
                                    as usize
                            ],
                            ..Default::default()
                        },
                    );
                }
                Ok(BackendTrackCreation {
                    track_id,
                    loops,
                    ports,
                })
            }
            _ => Err(anyhow!("requested browser track processor is unavailable")),
        }
    }

    fn create_loop(&mut self) -> Result<BackendLoopId> {
        Err(anyhow!("standalone browser loops are unsupported"))
    }

    fn create_direct_track(&mut self, request: DirectTrackRequest) -> Result<BackendTrackCreation> {
        let track_id = BackendTrackId::from_raw(self.next_track_id);
        let ports = browser_port_descriptors(
            &request.port_name_base,
            request.audio_channels,
            request.midi,
            &mut self.next_port_id,
        );
        let loops: Vec<_> = (0..request.initial_loops)
            .map(|offset| BackendLoopId::from_raw(self.next_loop_id + offset as u64))
            .collect();
        self.submit(Command::CreateTrack {
            expected_track_id: track_id.raw(),
            expected_loop_ids: loops.iter().map(|id| id.raw()).collect(),
            port_name_base: request.port_name_base.clone(),
            topology: WireTrackTopology::Direct {
                audio_channels: request.audio_channels,
                midi: request.midi,
            },
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
        for port in &ports {
            self.snapshot
                .connections
                .application_ports
                .insert(port.id, port.clone());
        }
        self.snapshot.connections.revision = self.snapshot.connections.revision.wrapping_add(1);
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
        Ok(BackendTrackCreation {
            track_id,
            loops,
            ports,
        })
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

    fn set_track_fx_control(
        &mut self,
        track_id: BackendTrackId,
        control: BackendTrackFxControl,
    ) -> Result<()> {
        let fx = self
            .snapshot
            .tracks
            .get(&track_id)
            .ok_or_else(|| anyhow!("unknown browser backend track {track_id:?}"))?
            .fx
            .as_ref()
            .ok_or_else(|| anyhow!("track has no processor"))?;
        if let BackendTrackFxControl::TinySynthFx(tiny) = &control {
            if !matches!(
                fx.editor.as_ref(),
                Some(TrackProcessorEditorState::TinySynthFx(_))
            ) {
                return Err(anyhow!("track has no Tiny Synth/FX editor state"));
            }
            match tiny {
                TinySynthFxControl::SelectPreset(id)
                    if !matches!(
                        tiny_synth_fx_descriptor().editor,
                        Some(shoop_egui::TrackProcessorEditorDescriptor::TinySynthFx {
                            presets
                        }) if presets.iter().any(|preset| preset.id == *id)
                    ) =>
                {
                    return Err(anyhow!("unknown Tiny Synth/FX preset {id}"));
                }
                TinySynthFxControl::SetMasterGainDb(value)
                    if !value.is_finite()
                        || !(shoop_egui::MIN_TINY_SYNTH_FX_GAIN_DB
                            ..=shoop_egui::MAX_TINY_SYNTH_FX_GAIN_DB)
                            .contains(value) =>
                {
                    return Err(anyhow!("invalid Tiny Synth/FX master gain"));
                }
                TinySynthFxControl::SetReverbAmount(value)
                    if !value.is_finite() || !(0.0..=1.0).contains(value) =>
                {
                    return Err(anyhow!("invalid Tiny Synth/FX reverb amount"));
                }
                TinySynthFxControl::SetDistortionDrive(value)
                    if !value.is_finite() || !(1.0..=20.0).contains(value) =>
                {
                    return Err(anyhow!("invalid Tiny Synth/FX distortion drive"));
                }
                _ => {}
            }
        }
        self.submit(Command::SetTrackFxControl {
            track_id: track_id.raw(),
            control: to_wire_track_fx_control(control.clone()),
        })?;

        let fx = self
            .snapshot
            .tracks
            .get_mut(&track_id)
            .expect("track checked")
            .fx
            .as_mut()
            .expect("processor checked");
        match control {
            BackendTrackFxControl::SetActive(value) => fx.active = value,
            BackendTrackFxControl::SetVisible(value) => fx.visible = value,
            BackendTrackFxControl::ToggleOrRecover => fx.visible = !fx.visible,
            BackendTrackFxControl::RestoreState(_) | BackendTrackFxControl::ClearLogs => {}
            BackendTrackFxControl::TinySynthFx(tiny) => {
                let Some(TrackProcessorEditorState::TinySynthFx(editor)) = fx.editor.as_mut()
                else {
                    unreachable!("Tiny Synth/FX editor was checked before submission");
                };
                match tiny {
                    TinySynthFxControl::SelectPreset(value) => {
                        editor.selected_preset_id = Some(value)
                    }
                    TinySynthFxControl::SetMasterGainDb(value) => editor.master_gain_db = value,
                    TinySynthFxControl::SetReverbEnabled(value) => editor.reverb_enabled = value,
                    TinySynthFxControl::SetReverbAmount(value) => editor.reverb_amount = value,
                    TinySynthFxControl::SetDistortionEnabled(value) => {
                        editor.distortion_enabled = value
                    }
                    TinySynthFxControl::SetDistortionDrive(value) => {
                        editor.distortion_drive = value
                    }
                    TinySynthFxControl::Panic => {}
                }
            }
        }
        Ok(())
    }

    fn track_fx_state_string(&mut self, track_id: BackendTrackId) -> Result<Option<String>> {
        let track = self
            .snapshot
            .tracks
            .get(&track_id)
            .ok_or_else(|| anyhow!("unknown browser backend track {track_id:?}"))?;
        let Some(fx) = &track.fx else {
            return Ok(None);
        };
        let Some(TrackProcessorEditorState::TinySynthFx(editor)) = &fx.editor else {
            return Ok(None);
        };
        let sample_rate = self.snapshot.status.sample_rate.max(1) as f32;
        Ok(Some(encode_tiny_synth_fx_state(sample_rate, editor)?))
    }

    fn inject_midi_input(
        &mut self,
        track_id: BackendTrackId,
        events: &[BackendMidiEvent],
    ) -> Result<()> {
        let track = self
            .snapshot
            .tracks
            .get(&track_id)
            .ok_or_else(|| anyhow!("unknown browser backend track {track_id:?}"))?;
        if !track.topology.has_midi() {
            return Err(anyhow!(
                "browser backend track has no MIDI input {track_id:?}"
            ));
        }
        if events.len() > MIDI_BATCH_CAPACITY
            || events
                .iter()
                .any(|event| event.time != 0 || event.data.is_empty() || event.data.len() > 4)
        {
            return Err(anyhow!("invalid browser MIDI input injection batch"));
        }
        self.submit(Command::InjectTrackMidiInput {
            track_id: track_id.raw(),
            events: events
                .iter()
                .map(|event| WireMidiEvent {
                    frame: event.time,
                    data: event.data.clone(),
                })
                .collect(),
        })
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

    fn set_loop_balance(&mut self, loop_id: BackendLoopId, balance: f32) -> Result<()> {
        if !self.snapshot.loops.contains_key(&loop_id) {
            return Err(anyhow!("unknown browser backend loop {loop_id:?}"));
        }
        self.submit(Command::SetLoopBalance {
            loop_id: loop_id.raw(),
            balance,
        })?;
        self.snapshot
            .loops
            .get_mut(&loop_id)
            .expect("loop checked")
            .balance = balance.clamp(-1.0, 1.0);
        Ok(())
    }

    fn grab_loops(&mut self, requests: &[BackendGrabRequest]) -> Result<()> {
        for request in requests {
            if !self.snapshot.loops.contains_key(&request.loop_id) {
                return Err(anyhow!(
                    "unknown browser backend loop {:?}",
                    request.loop_id
                ));
            }
        }
        self.submit(Command::GrabLoops {
            requests: requests
                .iter()
                .map(|request| WireGrabRequest {
                    loop_id: request.loop_id.raw(),
                    reverse_start_cycle: request.reverse_start_cycle,
                    cycles_length: request.cycles_length,
                    go_to_cycle: request.go_to_cycle,
                    go_to_mode: to_wire_loop_mode(request.go_to_mode),
                })
                .collect(),
        })?;
        for request in requests {
            self.waveforms.remove(&request.loop_id);
        }
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

    fn capture_session(&mut self) -> Result<BackendSessionData> {
        if let Some(capture) = &self.session_capture {
            if capture.total_bytes == Some(capture.bytes.len()) && !capture.in_flight {
                let session = serde_json::from_slice(&capture.bytes)
                    .map_err(|error| anyhow!("invalid worklet session capture: {error}"))?;
                self.session_capture = None;
                return Ok(session);
            }
            return Err(anyhow!("session capture pending"));
        }
        let generation = self.next_session_generation;
        self.next_session_generation = self.next_session_generation.saturating_add(1);
        self.transport
            .borrow_mut()
            .ephemeral(Command::BeginSessionCapture { generation })?;
        self.session_capture = Some(SessionCaptureAssembly {
            generation,
            total_bytes: None,
            bytes: Vec::new(),
            in_flight: true,
        });
        Err(anyhow!("session capture pending"))
    }

    fn replace_session(
        &mut self,
        session: &BackendSessionData,
    ) -> Result<BackendSessionReplacement> {
        if let Some(replace) = &self.session_replace {
            if &replace.session != session {
                return Err(anyhow!("another session replacement is active"));
            }
            if replace.complete {
                let replacement = browser_replacement_mapping(session);
                self.apply_replaced_session(session, &replacement);
                self.session_replace = None;
                return Ok(replacement);
            }
            self.pump_session_replace()?;
            return Err(anyhow!("session replacement pending"));
        }
        let bytes = serde_json::to_vec(session)?;
        if bytes.len() > SESSION_TRANSFER_MAX_BYTES {
            return Err(anyhow!("prepared session exceeds browser transfer limit"));
        }
        let generation = self.next_session_generation;
        self.next_session_generation = self.next_session_generation.saturating_add(1);
        self.transport
            .borrow_mut()
            .ephemeral(Command::BeginSessionReplace {
                generation,
                total_bytes: bytes.len(),
            })?;
        self.session_replace = Some(SessionReplaceAssembly {
            generation,
            session: session.clone(),
            bytes,
            next_offset: 0,
            commit_sent: false,
            complete: false,
        });
        self.pump_session_replace()?;
        Err(anyhow!("session replacement pending"))
    }

    fn set_port_connected(
        &mut self,
        port_id: BackendPortId,
        external_port: &str,
        connected: bool,
    ) -> Result<()> {
        let port = self
            .snapshot
            .connections
            .application_ports
            .get(&port_id)
            .ok_or_else(|| anyhow!("unknown browser application port {port_id:?}"))?;
        let host = self
            .snapshot
            .connections
            .host_ports
            .get(external_port)
            .ok_or_else(|| anyhow!("browser host port disappeared: {external_port}"))?;
        if port.data_type != host.data_type || port.direction == host.direction {
            return Err(anyhow!(
                "browser host port is incompatible: {external_port}"
            ));
        }
        self.submit(Command::SetPortConnected {
            application_port_id: port_id.raw(),
            host_port_id: external_port.to_owned(),
            connected,
        })
    }

    fn advance(&mut self, _elapsed: Duration) {}

    fn poll(&mut self) -> Result<BackendSnapshot> {
        self.sync_midi_endpoints()?;
        let state = self.transport.borrow().driver_state;
        let running = matches!(state, BackendDriverState::Running);
        self.pump_midi_input(running)?;
        self.snapshot.status.driver_state = state;
        self.snapshot.status.command_overflows = self.transport.borrow().overflows;
        if matches!(
            state,
            BackendDriverState::Running | BackendDriverState::Suspended
        ) && self.last_poll.elapsed() >= Duration::from_millis(u64::from(STATUS_INTERVAL_MS))
            && self.transport.borrow().in_flight < COMMAND_CAPACITY / 2
        {
            self.transport.borrow_mut().ephemeral(Command::Poll)?;
            self.transport
                .borrow_mut()
                .ephemeral(Command::DrainMidiOutput {
                    max_events: MIDI_BATCH_CAPACITY,
                })?;
            self.last_poll = Instant::now();
        }
        let events: Vec<_> = self.transport.borrow_mut().inbound.drain(..).collect();
        for envelope in events {
            match envelope.event {
                Event::Ack | Event::Stopped => {}
                Event::Error { message } => return Err(anyhow!(message)),
                Event::ConnectionMutationFailed {
                    application_port_id,
                    host_port_id,
                    desired_connected,
                    message,
                } => self
                    .snapshot
                    .connections
                    .failures
                    .push(BackendConnectionFailure {
                        port_id: BackendPortId::from_raw(application_port_id),
                        external_port: host_port_id,
                        desired_connected,
                        message,
                    }),
                Event::MidiOutput {
                    events,
                    dropped,
                    refused_input,
                } => {
                    let current_overflows = self.transport.borrow().overflows;
                    self.transport.borrow_mut().overflows = current_overflows
                        .saturating_add(dropped)
                        .saturating_add(refused_input);
                    for event in events {
                        if let Err(error) = self.midi.send(&event.host_port_id, &event.data) {
                            self.snapshot
                                .connections
                                .failures
                                .push(BackendConnectionFailure {
                                    port_id: BackendPortId::from_raw(event.application_port_id),
                                    external_port: event.host_port_id,
                                    desired_connected: true,
                                    message: error.to_string(),
                                });
                        }
                    }
                }
                Event::Snapshot(snapshot) => self.apply_wire_snapshot(snapshot),
                Event::Waveform(chunk) => self.apply_waveform_chunk(chunk)?,
                Event::SessionCaptureReady {
                    generation,
                    total_bytes,
                } => self.apply_session_capture_ready(generation, total_bytes)?,
                Event::SessionCaptureChunk {
                    generation,
                    offset,
                    total_bytes,
                    final_chunk,
                    bytes,
                } => self.apply_session_capture_chunk(
                    generation,
                    offset,
                    total_bytes,
                    final_chunk,
                    bytes,
                )?,
                Event::SessionReplaceComplete { generation } => {
                    if let Some(replace) = self.session_replace.as_mut() {
                        if replace.generation == generation {
                            replace.complete = true;
                        }
                    }
                }
                Event::SessionTransferAborted { generation } => {
                    if self
                        .session_capture
                        .as_ref()
                        .is_some_and(|capture| capture.generation == generation)
                    {
                        self.session_capture = None;
                    }
                    if self
                        .session_replace
                        .as_ref()
                        .is_some_and(|replace| replace.generation == generation)
                    {
                        self.session_replace = None;
                    }
                }
            }
        }
        self.pump_session_replace()?;
        if let Some(error) = self.transport.borrow_mut().error.take() {
            return Err(anyhow!(error));
        }
        Ok(self.snapshot.clone())
    }

    fn wait_idle(&mut self) {}
}

fn from_wire_data_type(value: WirePortDataType) -> BackendPortDataType {
    match value {
        WirePortDataType::Audio => BackendPortDataType::Audio,
        WirePortDataType::Midi => BackendPortDataType::Midi,
    }
}

fn from_wire_direction(value: WirePortDirection) -> BackendPortDirection {
    match value {
        WirePortDirection::Input => BackendPortDirection::Input,
        WirePortDirection::Output => BackendPortDirection::Output,
    }
}

fn from_wire_role(value: WirePortRole) -> BackendPortRole {
    match value {
        WirePortRole::AudioInput => BackendPortRole::AudioInput,
        WirePortRole::AudioOutput => BackendPortRole::AudioOutput,
        WirePortRole::AudioSend => BackendPortRole::AudioSend,
        WirePortRole::AudioReturn => BackendPortRole::AudioReturn,
        WirePortRole::MidiInput => BackendPortRole::MidiInput,
        WirePortRole::MidiOutput => BackendPortRole::MidiOutput,
        WirePortRole::MidiSend => BackendPortRole::MidiSend,
    }
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

fn to_wire_track_fx_control(control: BackendTrackFxControl) -> WireTrackFxControl {
    match control {
        BackendTrackFxControl::SetActive(value) => WireTrackFxControl::SetActive(value),
        BackendTrackFxControl::SetVisible(value) => WireTrackFxControl::SetVisible(value),
        BackendTrackFxControl::ToggleOrRecover => WireTrackFxControl::ToggleOrRecover,
        BackendTrackFxControl::RestoreState(value) => WireTrackFxControl::RestoreState(value),
        BackendTrackFxControl::ClearLogs => WireTrackFxControl::ClearLogs,
        BackendTrackFxControl::TinySynthFx(control) => match control {
            TinySynthFxControl::SelectPreset(value) => WireTrackFxControl::TinySelectPreset(value),
            TinySynthFxControl::SetMasterGainDb(value) => {
                WireTrackFxControl::TinySetMasterGainDb(value)
            }
            TinySynthFxControl::SetReverbEnabled(value) => {
                WireTrackFxControl::TinySetReverbEnabled(value)
            }
            TinySynthFxControl::SetReverbAmount(value) => {
                WireTrackFxControl::TinySetReverbAmount(value)
            }
            TinySynthFxControl::SetDistortionEnabled(value) => {
                WireTrackFxControl::TinySetDistortionEnabled(value)
            }
            TinySynthFxControl::SetDistortionDrive(value) => {
                WireTrackFxControl::TinySetDistortionDrive(value)
            }
            TinySynthFxControl::Panic => WireTrackFxControl::TinyPanic,
        },
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
